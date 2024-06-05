#![allow(dead_code)]
use std::{mem, sync::Arc};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::RwLock};
use crate::{connection::{handler::SecuredStream, line::ConnectedLine}, protocol::mqtt::ConnectPacket};

pub trait Streamer: 
AsyncReadExt
+ AsyncWriteExt
+ std::marker::Unpin {}


pub struct Client<S>
    where S: Streamer + Send + Sync + 'static
{
    line: ConnectedLine<S>,
    protocol_name: String,
    protocol_level: u8,
    client_id: String,
    keep_alive: u16,
}

impl<S> Client<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn new(line: ConnectedLine<S>, conn_pkt: ConnectPacket) -> Self {
        let mut pkt = conn_pkt;
        Self {
            line,
            client_id: mem::take(&mut pkt.client_id),
            keep_alive: pkt.keep_alive,
            protocol_level: pkt.protocol_level,
            protocol_name: pkt.protocol_name
        }
    }
}

type Clients<S: Streamer + Send + Sync + 'static> = Vec<Client<S>>;

pub struct BrokerMediator<S>
    where S: Streamer + Send + Sync + 'static
{
    clients: Arc<RwLock<Clients<S>>>,
}

impl<S> BrokerMediator<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn new() -> Self {
        let clients = Arc::new(RwLock::new(Vec::new()));
        Self{ clients }
    }
}

impl<ST> BrokerMediator<ST> 
    where ST: Streamer + Send + Sync + 'static
{
    pub async fn register(&self, client: Client<ST>)
    {
        let mut wr = self.clients.write().await;
        wr.push(client)
    }
}

// impl<S> KnownClient for BrokerMediator<S> 
//     where S: Streamer + Send + Sync + 'static
// {
//     async fn forward(&self, client: Online) {
//         let socket: LineSettle<_> = match client.line.socket {
//             crate::connection::line::Socket::Default(s) => LineSettle::new(client.line.id, s, client.line.addr),
//             crate::connection::line::Socket::Secure(s) => LineSettle::new(client.line.id, s, client.line.addr),
//         };

//         let line = LineSettle {id: client.line.id, socket, client.line.addr};
//         match client.state {
//             OnlineState::Publisher => {
//                 let topic = Topic::new(client.topic);
//                 let new_publisher = Publisher::new(topic, line);
//                 let write_publisher = self.publishers.write().await;
//                 write_publisher.insert(client)
//             }, OnlineState::Subscriber => ()
//         }
//     }
// }