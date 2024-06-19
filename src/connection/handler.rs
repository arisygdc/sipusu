use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::{errors::ConnError, line::{MqttHandshake, SocketConnection}, ConnectionID};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use crate::{message_broker::{client::ClientID, mediator::BrokerMediator}, protocol::v5::{connack::ConnackPacket, connect::ConnectPacket}, server::Wire};

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
    
    async fn establish_connection(&self, mut conn: SocketConnection, addr: SocketAddr) -> Result<ConnectPacket, ConnError> {
        let req_ack = conn.read_ack().await?;
        let mut connack_packet = ConnackPacket::default();
        let clid = ClientID::new(req_ack.client_id.clone());
        if !req_ack.clean_start() {
            let mut bucket = Some(conn);
            connack_packet.session_present = match self.broker.try_restore_connection(&clid, &mut bucket).await {
                Ok(_) => true,
                Err(_) => false
            };
        };
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
        let stream = match tls.accept(stream).await {
            Ok(v) => v,
            // TODO: Specify the error and response error message
            Err(err) => {
                eprintln!("[tls] conn {}, error: {}", id, err.to_string());
                return ;
            }
        };
        
        println!("[stream] secured");
        let stream = SocketConnection::Secure(stream);
        let _ = self.establish_connection(stream, addr).await;

        // errorcon_action(id, stream, err)
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let stream = stream;
        let stream = SocketConnection::Plain(stream);
        let _ = self.establish_connection(stream, addr).await;
    }
}

// fn errorcon_action<W: AsyncWriteExt>(conid: ConnectionID, stream: W, err: ConnError) {
//     match err.get_kind() {
//         ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted
//             => println!("[error] connection {} reason {}", conid, err.to_string()),
//         ErrorKind::InvalidData | ErrorKind::TimedOut
//             => {
//                 println!("[error] connection {} reason {}", conid, err.to_string());
//                 // stream.
//             }
//     }
// }

// async fn try_restore_session<C: IntoConnection>(broker: &BrokerMediator, clid: &str, conn: C) -> C {
//     let mut bucket = 
//     broker.try_restore_connection(&req_ack.client_id, &mut bucket).await
// }