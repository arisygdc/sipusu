use std::{borrow::BorrowMut, io, mem, net::SocketAddr, sync::atomic::{AtomicBool, Ordering}};
use tokio::{io::AsyncReadExt, net::TcpStream};
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
    pub(super) alive: AtomicBool,
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
            alive: AtomicBool::new(true),
            keep_alive: pkt.keep_alive,
            protocol_level: pkt.protocol_level,
            protocol_name: pkt.protocol_name
        }
    }

    pub(super) async fn listen(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        println!("[inner] listen {}", self.client_id);
        match self.socket.borrow_mut() {
            Socket::Plain(v) => v.read(buffer).await,
            Socket::Secure(v) => v.read(buffer).await
        }
    }

    pub(super) fn set_alive(&mut self, state: bool) -> bool {
        self.alive.swap(state, Ordering::AcqRel);
        let res = self.alive.compare_exchange(
            true,
            false, 
            Ordering::Release, 
            Ordering::Acquire
        );
        res.is_ok()
    }
}