use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::{errors::ConnError, line::ConnHandshake, ConnectionID};
use tokio::net::TcpStream;
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{connection::{errors::ErrorKind, line::SessionFlag}, message_broker::{client::{Client, SocketInner}, mediator::BrokerMediator}, protocol::mqtt::ConnectPacket, server::Wire};

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
    
    async fn establish_connection(&self, ack: &mut impl ConnHandshake, addr: &SocketAddr) -> Result<ConnectPacket, ConnError> {
        let req_ack = ack.read_ack().await?;

        let session = match self.broker.wakeup_exists(&req_ack.client_id, addr).await {
            Some(_) => SessionFlag::Preset,
            None => SessionFlag::New
        };
        
        ack.connack(session, 0).await?;
        Ok(req_ack)
    }

    fn request_id(&self) -> ConnectionID {
        let fetch = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Release);
        ConnectionID(fetch)
    }
}

impl Wire for Proxy {
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
        let err = match self.establish_connection(&mut stream, &addr).await {
            Ok(ack) =>  {
                let client = Client::new(id, SocketInner::Secure(stream), addr, ack);
                self.broker.register(client).await;
                return ;
            }, Err(e) => e
        };

        errorcon_action(id, err)
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let mut stream = stream;
        let id = self.request_id();
        let err = match self.establish_connection(&mut stream, &addr).await {
            Ok(ack) =>  {
                let client = Client::new(id, SocketInner::Plain(stream), addr, ack);
                self.broker.register(client).await;
                return ;
            }, Err(e) => e
        };
        errorcon_action(id, err)
    }
}

fn errorcon_action(conid: ConnectionID, err: ConnError) {
    match err.get_kind() {
        ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted
            => println!("[error] connection {} reason {}", conid, err.to_string()),
        ErrorKind::InvalidData | ErrorKind::TimedOut
            => {
                println!("[error] connection {} reason {}", conid, err.to_string());
                // TODO: send response
            }
    }
}