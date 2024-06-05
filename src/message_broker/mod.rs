#![allow(dead_code)]
use std::{mem, net::SocketAddr, sync::Arc};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::RwLock};
use crate::{connection::handler::SecuredStream, protocol::mqtt::ConnectPacket};

pub trait Streamer: 
AsyncReadExt
+ AsyncWriteExt
+ std::marker::Unpin {}

pub enum Socket {
    Secure(SecuredStream), 
    Plain(TcpStream)
}


pub struct Client {
    conn_num: u32,
    socket: Socket,
    addr: SocketAddr,
    protocol_name: String,
    protocol_level: u8,
    client_id: String,
    keep_alive: u16,
}

impl Client {
    pub fn new(
        conn_num: u32,
        socket: Socket,
        addr: SocketAddr, 
        conn_pkt: ConnectPacket
    ) -> Self {
        let mut pkt = conn_pkt;
        Self {
            conn_num,
            addr,
            socket,
            client_id: mem::take(&mut pkt.client_id),
            keep_alive: pkt.keep_alive,
            protocol_level: pkt.protocol_level,
            protocol_name: pkt.protocol_name
        }
    }
}

type Clients = Vec<Client>;

pub struct BrokerMediator {
    clients: Arc<RwLock<Clients>>,
}

impl BrokerMediator {
    pub fn new() -> Self {
        let clients = Arc::new(RwLock::new(Vec::new()));
        Self{ clients }
    }
}

impl BrokerMediator {
    pub async fn register(&self, client: Client)
    {
        let mut wr = self.clients.write().await;
        wr.push(client)
    }
}