use std::fmt::Display;

use tokio::{io::{ReadHalf, WriteHalf}, net::TcpStream};

use crate::connection::line::{SecuredStream, SocketConnection};

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