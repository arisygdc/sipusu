use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::{line::ConnHandshake, ConnectionID};
use tokio::{io::AsyncWriteExt, net::TcpStream};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{connection::line::SessionFlag, message_broker::{client::{Client, SocketInner}, mediator::BrokerMediator}, protocol::mqtt::ConnectPacket, server::Wire};

pub type SecuredStream = TlsStream<TcpStream>;
impl ConnHandshake for SecuredStream {}
impl ConnHandshake for TcpStream {}

#[allow(dead_code)]
pub struct Proxy {
    broker: BrokerMediator,
    access_total: AtomicU32,
}

impl Proxy {
    pub async fn new(broker: BrokerMediator) -> io::Result<Self> {
        let access_total = AtomicU32::new(1);
        Ok(Self { access_total, broker })
    }
    
    async fn establish_connection(&self, ack: &mut impl ConnHandshake, addr: &SocketAddr) -> io::Result<ConnectPacket> {
        let req_ack = ack.read_ack().await?;

        let session = match self.broker.wakeup_exists(&req_ack.client_id, addr).await {
            Some(_) => SessionFlag::Preset,
            None => SessionFlag::New
        };
        
        ack.connack(session, 0).await.unwrap();
        Ok(req_ack)
    }

    fn request_id(&self) -> ConnectionID {
        let fetch = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Release);
        ConnectionID(fetch)
    }
}

impl Wire for Proxy
{
    async fn connect_with_tls(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) {
        let id = self.request_id();
        println!("[stream] process id {}", id);
        let mut stream = match tls.accept(stream).await {
            Ok(v) => v,
            // TODO: Specify the error and response error message
            Err(err) => {
                eprintln!("[tls] conn {}, error: {}", id, err.to_string());
                return ;
            }
        };
        
        println!("[stream] secured");
        let req_ackk = match self.establish_connection(&mut stream, &addr).await {
            Ok(o) => o,
            Err(e) => {
                // TODO: unfinished
                if let io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput = e.kind() {
                    let mut err_response = [b'n', b'o'];
                    let _ = stream.write(&mut err_response).await;
                }
                return ;
            }
        };
        let client = Client::new(id, SocketInner::Secure(stream), addr, req_ackk);

        self.broker.register(client).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let mut stream = stream;
        let id = self.request_id();
        let req_ackk = self.establish_connection(&mut stream, &addr).await.unwrap();
        let client = Client::new(id, SocketInner::Plain(stream), addr, req_ackk);
        self.broker.register(client).await;
    }
}