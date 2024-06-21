use std::{fmt::Display, io, net::SocketAddr, sync::{atomic::{AtomicU64, AtomicU8, Ordering}, Arc}, time::{SystemTime, UNIX_EPOCH}};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, sync::Mutex};
use crate::{connection::{handshake::MqttConnectedResponse, line::SocketConnection, ConnectionID, SocketWriter}, protocol::v5::connack::ConnackPacket};

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
    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            SocketConnection::Plain(ref mut stream) 
                => stream.read(buf).await, 
            SocketConnection::Secure(ref mut stream) 
                => stream.read(buf).await
        }
    }

    async fn write_all(&self, buf: &mut [u8]) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            SocketConnection::Plain(ref mut stream) 
                => stream.write_all(buf).await,
            SocketConnection::Secure(ref mut stream) 
                => stream.write_all(buf).await
        }
    }

    fn new(socket: SocketConnection) -> Self {
        Self { inner: Arc::new(Mutex::new(socket)) }
    }
}

const ST_NEEDACK: u8 = 0x7Eu8;
const ST_DEAD: u8 = 0x7Fu8;
const ST_READY: u8 = 0x80u8;
const ST_READ: u8 = 0x81u8;
const ST_WRITE: u8 = 0x82u8;

#[derive(Debug)]
#[allow(dead_code)]
pub struct Client {
    pub(super) conid: ConnectionID,
    pub(super) clid: ClientID,
    /// state 130: soc_write
    /// state 129: soc_read
    /// state 128: alive
    /// state 127: dead
    /// state 126: response ack
    pub(super) state: AtomicU8,
    pub(super) addr: SocketAddr,
    socket: Socket,
    dead_on: AtomicU64,
    protocol_level: u8,
    keep_alive: u16,
}

pub struct UpdateClient {
    pub conid: Option<ConnectionID>,
    pub addr: Option<SocketAddr>,
    pub socket: Option<SocketConnection>,
    pub protocol_level: Option<u8>,
    pub keep_alive: Option<u16>,
}

impl Client {
    pub fn new(
        conid: ConnectionID,
        socket: SocketConnection,
        addr: SocketAddr,
        clid: ClientID,
        keep_alive: u16,
        protocol_level: u8
    ) -> Self {
        let socket = Socket { inner: Arc::new(Mutex::new(socket)) };
        Self {
            conid,
            addr,
            socket,
            clid,
            dead_on: AtomicU64::new(0),
            state: AtomicU8::new(ST_NEEDACK),
            keep_alive,
            protocol_level,
        }
    }

    pub(super) async fn listen(&self, buffer: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buffer).await
    }

    pub(super) async fn write(&self, buffer: &mut [u8]) -> io::Result<()> {
        self.socket.write_all(buffer).await
    }

    /// when set alive state = false
    /// it will schedule dead time
    pub(super) fn set_alive(&self, alive: bool) {
        let state = alive as u8 + ST_DEAD;
        let cpmx = self.state.compare_exchange(
            !state, 
            state, 
            Ordering::Acquire, 
            Ordering::Relaxed
        );

        if cpmx.is_err() {
            return;
        }

        let mut untime = 0;
        if alive == false {
            untime = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }

        self.dead_on.store(untime, Ordering::Release)
    }

    // TODO: Error type
    pub fn restore_connection(&mut self, bucket: &mut UpdateClient) -> io::Result<()> {
        let res = self.state.compare_exchange_weak(
            ST_DEAD, 
            ST_NEEDACK,
            Ordering::Acquire,
            Ordering::Relaxed
        );

        if res.is_err() {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, "connection still alive".to_string()));
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

    #[inline]
    pub(super) fn is_alive(&self) -> bool {
        self.state.load(Ordering::Relaxed) > ST_DEAD
    }

    #[inline]
    pub(super) fn is_dead_time(&self) -> bool {
        let dtime = self.dead_on.load(Ordering::Acquire);
        if dtime == 0 {
            return false;
        }

        let untime = now();
        return untime > dtime;
    }
}

impl SocketWriter for Client {
    async fn write_all(&mut self, buffer: &mut [u8]) -> tokio::io::Result<()> {
        let mut guard = self.socket.inner.lock().await;
        guard.write_all(buffer).await
    }
}

impl MqttConnectedResponse for Client {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()> {
        if self.state.load(Ordering::Relaxed) != ST_NEEDACK {
            panic!("response ack, but wrong time")
        }
        
        let mut packet = ack.encode().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if self.state.compare_exchange(
            ST_NEEDACK, 
            ST_READY,
            Ordering::Release, 
            Ordering::Relaxed
        ).is_err() {
            panic!("response ack, but wrong time")
        }

        let res = self.write_all(&mut packet).await;
        res
    }
}

#[inline]
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}