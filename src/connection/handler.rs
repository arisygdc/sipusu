use std::{io, net::SocketAddr, sync::atomic::AtomicU32};
use super::{errors::ConnError, handshake::{MqttConnectRequest, MqttConnectedResponse}, line::SocketConnection, ConnectionID};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use crate::{message_broker::{client::{Client, ClientID, UpdateClient}, mediator::BrokerMediator}, protocol::v5::connack::ConnackPacket, server::Wire};

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
    
    async fn establish_connection(&self, connid: ConnectionID, mut conn: SocketConnection, addr: SocketAddr) -> Result<(), ConnError> {
        let req_ack = conn.read_request().await?;
        let connack_packet = ConnackPacket::default();

        let clid = ClientID::new(req_ack.client_id.clone());
        self.start_session(
            connack_packet,
            connid,
            clid, 
            req_ack.clean_start(), 
            conn,
            addr, 
            req_ack.keep_alive,
            req_ack.protocol_level
        ).await.unwrap();
        Ok(())
    }

    // TODO: add some mechanism to prevent race condition
    async fn start_session(
        &self, 
        mut response: ConnackPacket,
        connid: ConnectionID,
        clid: ClientID,
        clean_start: bool,
        conn: SocketConnection, 
        addr: SocketAddr,
        keep_alive: u16,
        protocol_level: u8
    ) -> Result<(), String> {
        let mut conn = conn;
        let session_exists = self.broker.session_exists(&clid).await;
        if !clean_start && session_exists {
            // TODO: change client state from incoming request
            // TODO: response ack
            let mut bucket = UpdateClient {
                conid: Some(connid.clone()),
                addr: Some(addr),
                keep_alive: Some(keep_alive),
                protocol_level: Some(protocol_level),
                socket: Some(conn)
            };

            let restore_feedback = self.broker.try_restore_connection(&clid, &mut bucket, |c| {
                response.session_present = true;
                c.connack(&response)
            }).await;

            if let Ok(fb) = restore_feedback {
                fb.await.unwrap();
                return  Ok(());
            }
            
            conn = bucket.socket
                .take()
                .unwrap();
        }
        
        if session_exists {
            self.broker.remove(&clid).await.unwrap();
        }

        let client = Client::new(
            connid, 
            conn, 
            addr, 
            clid, 
            keep_alive,
            protocol_level
        );

        self.broker.register(client, |c| {
            c.connack(&response)
        })
        .await.unwrap()
        .await.unwrap();
        Ok(())
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
        let _ = self.establish_connection(id, stream, addr).await;
    }

    async fn connect(&self, stream: TcpStream, addr: SocketAddr) {
        let id = self.request_id();
        println!("[stream] process id {}", id);
        let stream = stream;
        let stream = SocketConnection::Plain(stream);
        let _ = self.establish_connection(id, stream, addr).await;
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