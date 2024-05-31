use std::{io::{self, ErrorKind}, sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::RwLock, time};
use tokio_rustls::server::TlsStream;

use crate::{authentication::{AuthData, AuthenticationStore, Authenticator}, server::Handler};

pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    // access_total: AtomicU64,
    authenticator: Authenticator,
    authenticated: Arc<ActiveConnection>
}

impl Proxy {
    pub async fn new(active_connection: Arc<ActiveConnection>) -> io::Result<Self> {
        let authenticator = Authenticator::new(String::from("user_store")).await?;
        Ok(Self { authenticated: active_connection, authenticator })
    }

    async fn authenticate(auth: &impl AuthenticationStore, auth_data: &AuthData) -> bool {
        auth.authenticate(auth_data).await
    }

    // TODO: it should produce errors
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
}

impl Handler for Proxy {
    // TODO: connection id
    // TODO: create handshake object
    async fn process_request(&self, mut stream: TlsStream<TcpStream>) {
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

        self.authenticated.push_connection(stream).await;
    }
}

pub struct ActiveConnection {
    conns: RwLock<Vec<TlsStream<TcpStream>>>
}

impl ActiveConnection {
    pub fn new() -> Self {
        Self { conns: RwLock::new(Vec::with_capacity(8)) }
    }

    async fn push_connection(&self, stream: TlsStream<TcpStream>)  {
        let mut conn_writer = self.conns.write().await;
        conn_writer.push(stream);
    }
}