use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::line::{ConnectedLine, Streamer};
use tokio::net::TcpStream;
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{connection::line::{MQTTHandshake, SessionFlag}, message_broker::{client::{Client, Socket}, mediator::BrokerMediator}, protocol::mqtt::ConnectPacket, server::Wire};

pub type SecuredStream = TlsStream<TcpStream>;
impl Streamer for SecuredStream {}
impl Streamer for TcpStream {}

#[allow(dead_code)]
pub struct Proxy {
    broker: BrokerMediator,
    access_total: AtomicU32,
}

impl Proxy {
    pub async fn new(broker: BrokerMediator) -> io::Result<Self> {
        let access_total = AtomicU32::default();
        Ok(Self { access_total, broker })
    }

    async fn establish_connection(&self, id: u32, ack: &mut impl MQTTHandshake) -> io::Result<ConnectPacket> {
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

impl Wire for Proxy
{
    async fn connect_with_tls(&self, stream: TcpStream, addr: SocketAddr, tls: TlsAcceptor) {
        // stream.split();
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
        let mut line = ConnectedLine::new(id, secured_stream, addr);
        let req_ackk = self.establish_connection(id, &mut line).await.unwrap();
        let client = Client::new(line.conn_num, Socket::Secure(line.socket), line.addr, req_ackk);

        self.broker.register(client).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        let mut line = ConnectedLine::new(id, stream, addr);
        let req_ackk = self.establish_connection(id, &mut line).await.unwrap();
        let client = Client::new(line.conn_num, Socket::Plain(line.socket), line.addr, req_ackk);
        self.broker.register(client).await;
    }
}