pub mod online;
use std::{io::{self, ErrorKind}, net::SocketAddr, sync::atomic::AtomicU32, time::Duration};
use bytes::BytesMut;
use online::ConnectedLine;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::mpsc, time};
use tokio_rustls::server::TlsStream;

use crate::{authentication::{AuthData, AuthenticationStore, Authenticator}, server::Handler};

pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    access_total: AtomicU32,
    authenticator: Authenticator,
    push_connection: mpsc::Sender<ConnectedLine>
}

impl Proxy {
    pub async fn new(push_connection: mpsc::Sender<ConnectedLine>) -> io::Result<Self> {
        let authenticator = Authenticator::new(String::from("user_store")).await?;
        let access_total = AtomicU32::default();
        Ok(Self { push_connection, authenticator, access_total })
    }

    async fn authenticate(auth: &impl AuthenticationStore, auth_data: &AuthData) -> bool {
        auth.authenticate(auth_data).await
    }

    async fn read_stream(stream: &mut TlsStream<TcpStream>, buffer: &mut BytesMut, timeout_sec: u8) -> io::Result<()>{
        let timeout_duration = Duration::from_secs(timeout_sec as u64);

        match time::timeout(timeout_duration, stream.read(buffer)).await {
            Ok(read_result) => {
                let read_leng = read_result?;
                if read_leng == 0 {
                    println!("Client closed connection");
                    return Err(io::Error::new(ErrorKind::ConnectionAborted, "Client closed connection"));
                }
                return Ok(());
            }
            Err(_) => {
                return Err(io::Error::new(ErrorKind::TimedOut, "Reading stream timeout"));
            }
        }
    }

    #[inline]
    async fn increment_access(&self) -> u32 {
        self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire)
    }
}

impl Handler for Proxy {
    // TODO: connection id
    // TODO: create handshake object
    async fn process_request(&self, mut stream: TlsStream<TcpStream>, addr: SocketAddr) {
        let mut buffer = BytesMut::with_capacity(1024);

        if let Err(read) = Self::read_stream(&mut stream, &mut buffer, 1).await {
            if let Err(e)  = stream.write(read.to_string().as_bytes()).await {
                eprint!("{}", e.to_string());
                return ;
            }
        }

        if buffer.len() < 30 {
            return ;
        }

        let conn_id = self.increment_access().await;
        let auth_data = AuthData::decode(&buffer);
        
        if Proxy::authenticate(&self.authenticator, &auth_data).await {
            let write_all = stream.write_all("wrong username or password".as_bytes()).await;
            if let Err(_) = write_all {
                return ;
            }
        }

        if let Err(e) = stream.write(b"[SYNC]").await {
            eprintln!("[stream]{}", e.to_string());
            return ;
        }

        buffer.clear();
        if let Err(read) = Self::read_stream(&mut stream, &mut buffer, 1).await {
            if let Err(e)  = stream.write(read.to_string().as_bytes()).await {
                eprint!("{}", e.to_string());
                return ;
            }
        }

        if buffer.len() < 5 {
            return ;
        }

        let ack = &buffer[0..5];
        if !ack.eq("[ACK]".as_bytes()) {
            return ;
        }

        if let Err(e) = stream.write(b"[OK]").await {
            eprintln!("[stream]{}", e.to_string());
            return ;
        }

        let con_secstream = ConnectedLine::new(conn_id, stream, addr);
        if let Err(err) =  self.push_connection.send(con_secstream).await {
            eprintln!("[channel] online: {}", err.to_string());
            let mut line = err.0;
            if let Err(e) = line.write(b"[Err]").await {
                eprintln!("[stream]{}", e.to_string());
            }
        };
    }
}