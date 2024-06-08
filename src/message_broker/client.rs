use std::{borrow::BorrowMut, io, mem, net::SocketAddr, sync::atomic::{AtomicBool, Ordering}, time::{SystemTime, UNIX_EPOCH}};
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
    dead_on: Option<u64>,
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
            dead_on: None,
            client_id: mem::take(&mut pkt.client_id),
            alive: AtomicBool::new(true),
            keep_alive: pkt.keep_alive,
            protocol_level: pkt.protocol_level,
            protocol_name: pkt.protocol_name
        }
    }

    pub(super) async fn listen(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.socket.borrow_mut() {
            Socket::Plain(v) => v.read(buffer).await,
            Socket::Secure(v) => v.read(buffer).await
        }
    }

    /// when set alive state = false
    /// it will schedule dead time
    pub(super) fn set_alive(&mut self, state: bool) {
        let cpmx = self.alive.compare_exchange(
            !state, 
            state, 
            Ordering::Acquire, 
            Ordering::Relaxed
        );

        if cpmx.is_ok() {
            let untime = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            self.dead_on = Some(untime);
        }
    }

    #[inline]
    pub(super) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn is_dead_time(&self) -> bool {
        if let Some(d) = self.dead_on {
            let untime = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            return untime > d;
        }
        false
    }
}