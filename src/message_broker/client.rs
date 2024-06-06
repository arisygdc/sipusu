use std::{mem, net::SocketAddr};
use tokio::net::TcpStream;
use crate::{connection::handler::SecuredStream, protocol::mqtt::ConnectPacket};

#[derive(Debug)]
pub enum Socket {
    Secure(SecuredStream), 
    Plain(TcpStream)
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Client {
    pub(super) conn_num: u32,
    socket: Socket,
    pub(super) addr: SocketAddr,
    protocol_name: String,
    protocol_level: u8,
    pub(super) client_id: String,
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