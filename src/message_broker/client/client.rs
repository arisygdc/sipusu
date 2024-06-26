use std::{fmt::Display, io, net::SocketAddr};
use tokio::{io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf}, net::TcpStream};
use crate::{
    connection::{
        handshake::MqttConnectedResponse, 
        line::{SecuredStream, SocketConnection}, 
        ConnectionID, SocketReader, 
        SocketWriter
    }, 
    helper::time::sys_now, 
    protocol::v5::connack::ConnackPacket
};
use super::SessionController;

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

enum SocketInner<S, P> {
    Secured(S),
    Plain(P)
}

pub struct Socket<S, P> {
    r: SocketInner<ReadHalf<S>, ReadHalf<P>>,
    w: SocketInner<WriteHalf<S>, WriteHalf<P>>
}

impl Socket<SecuredStream, TcpStream> {
    fn new(socket: SocketConnection) -> Self {
        match socket {
            SocketConnection::Plain(p) => {
                let (r, w) = tokio::io::split(p);
                Socket {
                    r: SocketInner::Plain(r),
                    w: SocketInner::Plain(w),
                }
            },
            SocketConnection::Secure(s) => {
                let (r, w) = tokio::io::split(s);
                Socket {
                    r: SocketInner::Secured(r),
                    w: SocketInner::Secured(w),
                }
            }
        }
    }
}


#[allow(dead_code)]
pub struct Client {
    pub(super) conid: ConnectionID,
    pub clid: ClientID,
    pub(super) addr: SocketAddr,
    socket: Socket<SecuredStream, TcpStream>,
    protocol_level: u8,
    ttl: u64,
    keep_alive: u16,
    expr_interval: u32,
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
    pub async fn new(
        conid: ConnectionID,
        socket: SocketConnection,
        addr: SocketAddr,
        clid: ClientID,
        keep_alive: u16,
        expr_interval: u32,
        protocol_level: u8
    ) -> Self {
        let socket = Socket::new(socket);
        let keep_alive = keep_alive.max(60);
        Self {
            conid,
            addr,
            socket,
            clid,
            ttl: sys_now() + keep_alive as u64,
            expr_interval,
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
}

impl SessionController for Client {
    fn is_alive(&self, t: u64) -> bool {
        self.ttl >= t
    }

    fn is_expired(&self, t: u64) -> bool {
        (self.expr_interval as u64 + self.ttl) <= t
    }

    fn keep_alive(&mut self, t: u64) -> Result<u64, String> {
        if self.ttl <= t {
            return Err(String::from("already expired"));
        }
        self.ttl = t + self.keep_alive as u64;
        Ok(self.ttl)
    }

    fn kill(&mut self) {
        self.ttl = 0;
    }
}

impl SocketWriter for Client {
    async fn write_all(&mut self, buffer: &[u8]) -> tokio::io::Result<()> {
        match &mut self.socket.w {
            SocketInner::Plain(p) => p.write_all(buffer).await,
            SocketInner::Secured(s) => s.write_all(buffer).await
        }
    }
}

impl SocketReader for Client {
    async fn read(&mut self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        match &mut self.socket.r {
            SocketInner::Plain(p) => p.read(buffer).await,
            SocketInner::Secured(s) => s.read(buffer).await
        }
    }
}

impl MqttConnectedResponse for Client {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()> {
        let mut packet = ack.encode().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let res = self.write_all(&mut packet).await;
        res
    }
}