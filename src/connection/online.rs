#![allow(unused)]
use std::net::SocketAddr;
use tokio::{net::{TcpStream}, sync::RwLock};
use tokio_rustls::server::TlsStream;

use crate::server::SecuredStream;

pub struct Onlines {
    conns: RwLock<Vec<ConnectedLine>>
}

impl Onlines {
    pub fn new() -> Self {
        Self { conns: RwLock::new(Vec::with_capacity(8)) }
    }

    pub async fn push_connection(&self, stream: ConnectedLine)  {
        let mut conn_writer = self.conns.write().await;
        conn_writer.push(stream);
    }
}

pub struct ConnectedLine {
    id: u32,
    socket: SecuredStream,
    addr: SocketAddr
}

impl ConnectedLine {
    pub fn new(id: u32, socket: SecuredStream, addr: SocketAddr) -> Self {
        Self { id, socket, addr }
    }
}