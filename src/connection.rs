use std::{sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::RwLock, time};
use tokio_rustls::server::TlsStream;

use crate::server::Handler;

pub struct Proxy {
    // ticker: u32,
    // access_second: u32,
    // access_total: AtomicU64,
    authenticated: Arc<ActiveConnection>
}

impl Proxy {
    pub fn new(active_connection: Arc<ActiveConnection>) -> Self {
        Self { authenticated: active_connection }
    }

    // TODO
    fn authenticate() -> bool {
        true
    }
}

impl Handler for Proxy {
    async fn process_request(&self, mut stream: TlsStream<TcpStream>) {
        if !Self::authenticate() {
            let write_all = stream.write_all("wrong username or password".as_bytes()).await;
            if let Err(_) = write_all {
                return ;
            }
        }

        let mut buffer = BytesMut::with_capacity(1024);
        let timeout_duration = Duration::from_secs(1);
        match time::timeout(timeout_duration, stream.read(&mut buffer)).await {
            Ok(Ok(n)) => {
                if n == 0 {
                    println!("Client closed connection");
                    return;
                }
    
                stream.write_all(b"Ok").await.expect("Failed to write response");
                self.authenticated.push_connection(stream).await;
            }
            Ok(Err(e)) => {
                eprintln!("Error reading from stream: {}", e);
                return;
            }
            Err(_) => {
                eprintln!("Read operation timed out");
                return;
            }
        }
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