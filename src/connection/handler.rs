use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::line::{ConnectedLine, Streamer};
use tokio::net::TcpStream;
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use crate::{connection::line::{MQTTHandshake, SessionFlag}, message_broker::{client::{Client, SocketInner}, mediator::BrokerMediator}, protocol::mqtt::ConnectPacket, server::Wire};

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
        let access_total = AtomicU32::new(1);
        Ok(Self { access_total, broker })
    }

    async fn establish_connection(&self, ack: &mut impl MQTTHandshake, addr: &SocketAddr) -> io::Result<ConnectPacket> {
        // TODO: validate ack
        let req_ack = ack.read_ack().await?;

        let session = match self.broker.wakeup_exists(&req_ack.client_id, addr).await {
            Some(_) => SessionFlag::Preset,
            None => SessionFlag::New
        };
        
        ack.connack(session, 0).await.unwrap();
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
        let mut line = ConnectedLine::new(id, secured_stream);
        let req_ackk = match self.establish_connection(&mut line, &addr).await {
            Ok(o) => o,
            Err(e) => {
                // TODO: unfinished
                if let io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput = e.kind() {
                    let mut err_response = [b'n', b'o'];
                    let _ = line.write(&mut err_response).await;
                }
                return ;

            }
        };
        let client = Client::new(line.conn_num, SocketInner::Secure(line.socket), addr, req_ackk);

        self.broker.register(client).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.access_total.fetch_add(1, std::sync::atomic::Ordering::Acquire);
        let mut line = ConnectedLine::new(id, stream);
        let req_ackk = self.establish_connection(&mut line, &addr).await.unwrap();
        let client = Client::new(line.conn_num, SocketInner::Plain(line.socket), addr, req_ackk);
        self.broker.register(client).await;
    }
}