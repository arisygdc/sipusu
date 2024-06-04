use std::{io::{self, ErrorKind}, net::SocketAddr, sync::atomic::AtomicU32, time::Duration};
use bytes::BytesMut;
use super::line::{ConnectedLine, Socket};
use tokio::{io::AsyncReadExt, net::TcpStream, sync::mpsc, time};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{core::Online, server::Wire};

pub type SecuredStream = TlsStream<TcpStream>;

#[allow(dead_code)]
pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    send_connection: mpsc::Sender<Online>,
    access_total: AtomicU32,
}

impl Proxy {
    pub async fn new(send_connection: mpsc::Sender<Online>) -> io::Result<Self> {
        let access_total = AtomicU32::default();
        Ok(Self { access_total, send_connection })
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

    async fn establish_connection(&self, id: u32, stream: Socket, addr: SocketAddr) {
        let mut line = ConnectedLine::new(id, stream, addr);
        // FIXME: calling unwrap
        let (topic, state) = line.handshake().await.unwrap();
        println!("[{}] topic {} ready to send", id, topic);
        let on = Online::new(line, topic, state);
        self.send_connection.send(on).await.unwrap();
    }    
}

impl Wire for Proxy {
    async fn connect_with_tls(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        println!("[stream] process id {}", id);
        let secured_stream = match tls.accept(stream).await {
            Ok(v) => Socket::Secure(v),
            // TODO: Specify the error and response error message
            Err(err) => {
                eprintln!("[tls] conn {}, error: {}", id, err.to_string());
                return ;
            }
        };
        println!("[stream] secured");

        self.establish_connection(id, secured_stream, addr).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        self.establish_connection(id, Socket::Default(stream), addr).await;
    }
}
