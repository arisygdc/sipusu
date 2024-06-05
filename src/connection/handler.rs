use std::{io, net::SocketAddr, sync::{atomic::AtomicU32, Arc}};
use super::line::ConnectedLine;
use tokio::net::TcpStream;
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{connection::line::{MQTTHandshake, SessionFlag}, message_broker::{BrokerMediator, Client, Streamer}, protocol::mqtt::ConnectPacket, server::Wire};

pub type SecuredStream = TlsStream<TcpStream>;
impl Streamer for SecuredStream {}
impl Streamer for TcpStream {}

#[allow(dead_code)]
pub struct Proxy<S> 
    where S: Streamer + Send + Sync + 'static
{
    broker: BrokerMediator<S>,
    access_total: AtomicU32,
}

impl<S> Proxy<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub async fn new(broker: BrokerMediator<S>) -> io::Result<Self> {
        let access_total = AtomicU32::default();
        Ok(Self { access_total, broker })
    }

    async fn establish_connection(&self, id: u32, mut ack: impl MQTTHandshake, addr: SocketAddr) -> io::Result<ConnectPacket> {
        // let mut line = ConnectedLine::new(id, stream, addr);

        // FIXME: calling unwrap
        // TODO: validate ack
        let req_ack = ack.read_ack().await.unwrap();
        println!("[{}] {:?}", id, req_ack);
        ack.connack(SessionFlag::New, 0).await.unwrap();
        
        // let client = Client::new(ack, req_ack);
        // self.broker.register(client).await;
        Ok(req_ack)
    }
}

impl<S> Wire for Proxy<S> 
    where S: Streamer + Send + Sync + 'static
{
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
        let f = secured_stream.
        println!("[stream] secured");
        let line = ConnectedLine::new(id, secured_stream, addr);
        let req_ackk = self.establish_connection(id, line, addr).await.unwrap();
        let client: Client<TlsStream<TcpStream>> = Client::new(line, req_ackk);

        self.broker.register(client).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        let line = ConnectedLine::new(id, stream, addr);
        self.establish_connection(id, stream, addr).await;
    }
}

