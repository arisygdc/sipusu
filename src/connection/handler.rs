use std::{io::{self, ErrorKind}, net::SocketAddr, sync::atomic::AtomicU32, time::Duration};
use bytes::BytesMut;
use super::online::{ConnectedLine, Streamer};
use tokio::{io::AsyncReadExt, net::TcpStream, sync::mpsc, time};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::server::Wire;

pub type SecuredStream = TlsStream<TcpStream>;

impl Streamer for SecuredStream {}
impl Streamer for TcpStream {}

#[allow(dead_code)]
pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    // send_connection: mpsc::UnboundedSender<S>,
    access_total: AtomicU32,
}

impl Proxy {
    pub async fn new() -> io::Result<Self> {
        let access_total = AtomicU32::default();
        Ok(Self { access_total })
    }

    async fn read_stream(stream: &mut SecuredStream, buffer: &mut BytesMut, timeout_sec: u8) -> io::Result<()> {
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

impl Wire for Proxy {
    async fn connect_with_tls(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) {
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
        println!("[stream] secured");

        establish_connection(id, secured_stream, addr).await
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        establish_connection(id, stream, addr).await
    }
}

async fn establish_connection<S>(id: u32, stream: S, addr: SocketAddr) 
    where S: Streamer + Send + Sync + 'static
{
    let mut line = ConnectedLine::new(id, stream, addr);
    let identity = match line.handshake().await {
        Some(v) => v,
        None => return,
    };

    line.online(identity)
}