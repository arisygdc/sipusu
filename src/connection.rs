use std::{io, sync::Arc, time::Duration};
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
}

impl Handler for Proxy {
    async fn process_request(&self, mut stream: TlsStream<TcpStream>) {
        let mut buffer = BytesMut::with_capacity(1024);

        let timeout_duration = Duration::from_secs(1);
        let auth_data;
        match time::timeout(timeout_duration, stream.read(&mut buffer)).await {
            Ok(Ok(n)) => {
                if n == 0 {
                    println!("Client closed connection");
                    return;
                }

                auth_data = AuthData::decode(&buffer);
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
        
        
        if Proxy::authenticate(&self.authenticator, &auth_data).await {
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