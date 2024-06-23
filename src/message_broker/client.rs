use std::{fmt::Display, io, net::SocketAddr, sync::{atomic::AtomicU64, Arc}};
use tokio::sync::Mutex;
use crate::{connection::{handshake::MqttConnectedResponse, line::SocketConnection, ConnectionID, SocketReader, SocketWriter}, helper::time::sys_now, protocol::v5::connack::ConnackPacket};

use super::clients::SessionController;

extern crate tokio;

#[derive(Debug, Eq, Ord, Clone)]
pub struct ClientID {
    id: String,
    hash: u32,
}

impl ClientID {
    pub fn new(raw_clid: String) -> ClientID {
        Self{
            hash: Self::hash(&raw_clid),
            id: raw_clid,
        }
    }

    /// murmurhash 3
    fn hash(raw_clid: &str) -> u32 {
        const C1: u32 = 0xcc9e2d51;
        const C2: u32 = 0x1b873593;
        const SEED: u32 = 0;
        
        let mut hash = SEED;
        let data = raw_clid.as_bytes();

        let nblocks = data.len() / 4;

        for i in 0..nblocks {
            let mut k = u32::from_le_bytes([data[4 * i], data[4 * i + 1], data[4 * i + 2], data[4 * i + 3]]);
            k = k.wrapping_mul(C1);
            k = k.rotate_left(15);
            k = k.wrapping_mul(C2);
            
            hash ^= k;
            hash = hash.rotate_left(13);
            hash = hash.wrapping_mul(5).wrapping_add(0xe6546b64);
        }

        let tail = &data[nblocks * 4..];
        let mut k1 = 0;
        match tail.len() {
            3 => k1 ^= (tail[2] as u32) << 16,
            2 => k1 ^= (tail[1] as u32) << 8,
            1 => k1 ^= tail[0] as u32,
            _ => (),
        }
        k1 = k1.wrapping_mul(C1);
        k1 = k1.rotate_left(15);
        k1 = k1.wrapping_mul(C2);
        hash ^= k1;

        hash ^= data.len() as u32;
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(0x85ebca6b);
        hash ^= hash >> 13;
        hash = hash.wrapping_mul(0xc2b2ae35);
        hash ^= hash >> 16;

        hash
    }
}

impl Display for ClientID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl PartialEq for ClientID {
    fn eq(&self, other: &Self) -> bool {
        if self.id.len() != other.id.len() {
            return false;
        }
        
        self.hash == other.hash
    }

    fn ne(&self, other: &Self) -> bool {
        if self.id.len() == other.id.len() {
            return false;
        }

        self.hash != other.hash
    }
}

impl PartialOrd for ClientID {
    fn ge(&self, other: &Self) -> bool {
        self.hash >= other.hash
    }

    fn gt(&self, other: &Self) -> bool {
        self.hash > other.hash
    }

    fn le(&self, other: &Self) -> bool {
        self.hash <= other.hash
    }

    fn lt(&self, other: &Self) -> bool {
        self.hash < other.hash
    }

    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.hash.cmp(&other.hash))
    }
}

#[derive(Debug)]
pub struct Socket {
    inner: Arc<Mutex<SocketConnection>>
}

impl Socket {
    fn new(socket: SocketConnection) -> Self {
        Self { inner: Arc::new(Mutex::new(socket)) }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Client {
    pub(super) conid: ConnectionID,
    pub(super) clid: ClientID,
    pub(super) addr: SocketAddr,
    socket: Socket,
    dead_on: AtomicU64,
    protocol_level: u8,
    ttl: u64,
    keep_alive: u16,
    expr_interval: u32
}

pub struct UpdateClient {
    pub conid: Option<ConnectionID>,
    pub addr: Option<SocketAddr>,
    pub socket: Option<SocketConnection>,
    pub protocol_level: Option<u8>,
    pub keep_alive: Option<u16>,
}

// keepalive min value: 60
impl Client {
    pub fn new(
        conid: ConnectionID,
        socket: SocketConnection,
        addr: SocketAddr,
        clid: ClientID,
        keep_alive: u16,
        expr_interval: u32,
        protocol_level: u8
    ) -> Self {
        let socket = Socket { inner: Arc::new(Mutex::new(socket)) };
        let keep_alive = keep_alive.max(60);
        Self {
            conid,
            addr,
            socket,
            clid,
            ttl: 0,
            expr_interval,
            dead_on: AtomicU64::new(0),
            keep_alive,
            protocol_level,
        }
    }

    // TODO: Error type
    pub fn restore_connection(&mut self, bucket: &mut UpdateClient) -> io::Result<()> {
        let now = sys_now();
        if !self.is_alive(now) && self.is_expired(now) {
           return Err(io::Error::new(io::ErrorKind::Other, String::from("connection is expired"))); 
        }

        match bucket.socket.take() {
            Some(s) => self.socket = Socket::new(s),
            None => panic!("empty connection")
        };

        if let Some(keep_alive) = bucket.keep_alive.take() {
            self.keep_alive = keep_alive;
        }
        if let  Some(addr) = bucket.addr.take() {
            self.addr = addr
        }
        if let Some(connid) = bucket.conid.take() {
            self.conid = connid;
        }
        if let Some(pr_lvl) = bucket.protocol_level.take() {
            self.protocol_level = pr_lvl;
        }

        Ok(())
    }


    // #[inline]
    // pub(super) fn is_alive(&self) -> bool {
    //     self.state.load(Ordering::Relaxed) > ST_DEAD
    // }

    // #[inline]
    // pub(super) fn keep_alive(&self) -> Result<u64, String> {
    //     let state = self.state.load(Ordering::Release);
    //     match state.cmp(&ST_READY) {
    //         cmp::Ordering::Equal | cmp::Ordering::Greater => (),
    //         cmp::Ordering::Less => return Err(format!("cannot keep client {}, when not alive state", &self.clid))
    //     }

    //     let updated = self.dead_on.fetch_update(
    //         Ordering::Acquire, 
    //         Ordering::Relaxed, 
    //         |dt| {
    //             let now = now();
    //             let schedule = self.keep_alive as u64 + now;
    //             if cfg!(debug_asertion) {
    //                 if dt > now {
    //                     panic!();
    //                 }
    //             }
    //             Some(schedule)
    //         }
    //     );
    //     updated.map_err(|_| format!("fail to keep alive client {}", &self.clid))
    // }
}

impl SessionController for Client {
    fn is_alive(&self, t: u64) -> bool {
        self.ttl <= t
    }

    fn is_expired(&self, t: u64) -> bool {
        (self.expr_interval as u64 + self.ttl) >= t
    }

    fn keep_alive(&mut self, t: u64) -> Result<u64, String> {
        if t < self.ttl {
            return Err(String::from("given time cannot less than ttl"));
        }
        self.ttl = t + self.keep_alive as u64;
        Ok(self.ttl)
    }
}

impl SocketWriter for Client {
    async fn write_all(&mut self, buffer: &mut [u8]) -> tokio::io::Result<()> {
        let mut guard = self.socket.inner.lock().await;
        guard.write_all(buffer).await
    }
}

impl SocketReader for Client {
    async fn read(&mut self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        let mut guard = self.socket.inner.lock().await;
        guard.read(buffer).await
    }
}

impl MqttConnectedResponse for Client {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()> {
        let mut packet = ack.encode().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let res = self.write_all(&mut packet).await;
        res
    }
}