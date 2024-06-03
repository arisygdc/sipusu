use std::{io::{self, ErrorKind}, net::SocketAddr, sync::{atomic::AtomicU32, Arc}, time::Duration};
use bytes::BytesMut;
use super::online::ConnectedLine;
use tokio::{io::AsyncReadExt, net::TcpStream, time};
use tokio_rustls::{server::TlsStream, TlsAcceptor};

use crate::{authentication::{AuthData, AuthenticationStore, Authenticator}, server::Wire};

#[allow(dead_code)]
pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    access_total: AtomicU32,
    authenticator: Arc<Authenticator>,
    // push_connection: mpsc::Sender<ConnectedLine>
}

impl Proxy {
    pub async fn new() -> io::Result<Self> {
        let authenticator = Arc::new(Authenticator::new(String::from("user_store")).await?);
        let access_total = AtomicU32::default();
        Ok(Self { authenticator, access_total })
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
}

// TODO: authentication
impl Wire for Proxy {
    async fn connect(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        println!("[stream] process id {}", id);
        let secured_stream = match tls.accept(stream).await {
            Ok(v) => v,
            // TODO: Specify the error and response error message
            Err(err) => {
                eprintln!("[tls] conn {}, error: {}", id, err.to_string());
                return ;
            }
        };
        println!("[secured-stream] established");
        let line = match ConnectedLine::new(id, secured_stream, addr)
            .handshake::<Authenticator>(None).await 
        {
            Some(v) => v,
            None => return,
        };
        line.online()
    }
}