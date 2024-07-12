use std::fmt::Display;

use tokio::{io::{self, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf}, net::TcpStream};

use crate::{connection::{handshake::MqttConnectedResponse, line::{SecuredStream, SocketConnection}, SocketReader, SocketWriter}, helper::time::sys_now, protocol::v5::connack::ConnackPacket};

use super::SessionController;

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

pub struct Session {
    pub(super) ttl: u64,
    pub(super) expr_interval: u32,
    pub(super) keep_alive: u16,
}

#[derive(Default)]
pub struct Limiter {
    pub(super) receive_maximum: Option<u16>,
    pub(super) maximum_packet_size: Option<u16>,
    pub(super) topic_alias_maximum: Option<u16>,
}

impl Limiter {
    pub fn new(
        receive_maximum: Option<u16>, 
        maximum_packet_size: Option<u16>, 
        topic_alias_maximum: Option<u16>
    ) -> Self {
        Self { maximum_packet_size, receive_maximum, topic_alias_maximum }
    }
}

impl SessionController for Session {
    fn is_alive(&self, t: u64) -> bool {
        self.ttl >= t
    }

    fn is_expired(&self, t: u64) -> bool {
        self.expiration_time() <= t
    }

    fn keep_alive(&mut self, t: u64) -> Result<u64, String> {
        if self.ttl <= t {
            return Err(String::from("already expired"));
        }
        self.ttl = t + (self.keep_alive + self.keep_alive/2) as u64;
        Ok(self.ttl)
    }

    fn expiration_time(&self) -> u64 {
        self.expr_interval as u64 + self.ttl
    }

    fn ttl(&self) -> u64 {
        self.ttl
    }

    fn kill(&mut self) {
        self.ttl = sys_now() -1 ;
    }
}


pub type ClientSocket = Socket<SecuredStream, TcpStream>;
pub(super) enum SocketInner<S, P> {
    Secured(S),
    Plain(P)
}

pub struct Socket<S, P> {
    pub(super) r: SocketInner<ReadHalf<S>, ReadHalf<P>>,
    pub(super) w: SocketInner<WriteHalf<S>, WriteHalf<P>>
}

impl Socket<SecuredStream, TcpStream> {
    pub(super) fn new(socket: SocketConnection) -> Self {
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

impl SocketWriter for Socket<SecuredStream, TcpStream> {
    async fn write_all(&mut self, buffer: &[u8]) -> tokio::io::Result<()> {
        match &mut self.w {
            SocketInner::Plain(p) => p.write_all(buffer).await,
            SocketInner::Secured(s) => s.write_all(buffer).await
        }
    }
}

impl SocketReader for Socket<SecuredStream, TcpStream> {
    async fn read(&mut self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        match &mut self.r {
            SocketInner::Plain(p) => p.read(buffer).await,
            SocketInner::Secured(s) => s.read(buffer).await
        }
    }
}

impl MqttConnectedResponse for Socket<SecuredStream, TcpStream> {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()> {
        let mut packet = ack.encode().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let res = self.write_all(&mut packet).await;
        res
    }
}