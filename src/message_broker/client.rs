use std::{io, mem, net::SocketAddr, pin::Pin, sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc}, time::{SystemTime, UNIX_EPOCH}};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::{Mutex, RwLock}, task::yield_now};
use crate::{connection::handler::SecuredStream, protocol::mqtt::{ConnectPacket, MqttClientPacket}};

use super::{Consumer, Event, EventListener};
extern crate tokio;


#[derive(Debug)]
pub enum SocketInner {
    Secure(SecuredStream), 
    Plain(TcpStream)
}

#[derive(Debug)]
pub struct Socket {
    inner: Arc<Mutex<SocketInner>>
}

impl Socket {
    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            SocketInner::Plain(ref mut v) => {
                let mut stream = unsafe { Pin::new_unchecked(v) };
                stream.read(buf).await
            }, SocketInner::Secure(ref mut v) => {
                let mut stream = unsafe { Pin::new_unchecked(v) };
                stream.read(buf).await
            }
        }
    }

    async fn write_all(&self, buf: &mut [u8]) -> io::Result<()> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            SocketInner::Plain(ref mut v) => {
                let mut stream = unsafe { Pin::new_unchecked(v) };
                stream.write_all(buf).await
            }, SocketInner::Secure(ref mut v) => {
                let mut stream = unsafe { Pin::new_unchecked(v) };
                stream.write_all(buf).await
            }
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Client {
    pub(super) conn_num: u32,
    pub(super) alive: AtomicBool,
    socket: Socket,
    pub(super) addr: SocketAddr,
    dead_on: AtomicU64,
    protocol_name: String,
    protocol_level: u8,
    pub(super) client_id: String,
    keep_alive: u16,
}

impl Client {
    pub fn new(
        conn_num: u32,
        socket: SocketInner,
        addr: SocketAddr, 
        conn_pkt: ConnectPacket
    ) -> Self {
        let mut pkt = conn_pkt;
        let socket = Socket { inner: Arc::new(Mutex::new(socket)) };
        Self {
            conn_num,
            addr,
            socket,
            dead_on: AtomicU64::new(0),
            client_id: mem::take(&mut pkt.client_id),
            alive: AtomicBool::new(true),
            keep_alive: pkt.keep_alive,
            protocol_level: pkt.protocol_level,
            protocol_name: pkt.protocol_name
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
    pub(super) fn set_alive(&self, state: bool) {
        let cpmx = self.alive.compare_exchange(
            !state, 
            state, 
            Ordering::Acquire, 
            Ordering::Relaxed
        );

        if cpmx.is_err() {
            return;
        }

        let mut untime = 0;
        if state == false {
            untime = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }

        self.dead_on.store(untime, Ordering::Release)
    }

    #[inline]
    pub(super) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    #[inline]
    pub(super) fn is_dead_time(&self) -> bool {
        let dtime = self.dead_on.load(Ordering::Acquire);
        if dtime == 0 {
            return false;
        }

        let untime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        return untime > dtime;
    }
}

type MutexClients = RwLock<Vec<Client>>;

pub struct Clients(Arc<MutexClients>);

impl Clients {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(Vec::new())))
    }
    
    /// insert sort by conn number
    pub async fn insert(&self, new_cl: Client) {
        let mut clients = self.0.write().await;

        let mut found = clients.len();
        for (i, cval) in clients.iter().enumerate() {
            match new_cl.conn_num.cmp(&cval.conn_num) {
                std::cmp::Ordering::Equal => panic!("kok iso"),
                std::cmp::Ordering::Greater => continue,
                std::cmp::Ordering::Less => ()
            }

            found = i;
            break;
        }
        clients.insert(found, new_cl);
    }

    pub async fn remove(&self, con_num: u32) -> Result<(), String> {
        let mut clients = self.0.write().await;
        let idx = clients.binary_search_by(|c| con_num.cmp(&c.conn_num))
            .map_err(|_| format!("cannot find conn num {}", con_num))?;
        clients.remove(idx);
        Ok(())
    }

    pub async fn find<R>(&self, clid: &str, f: impl FnOnce(&Client) -> R) -> Option<R> {
        let clients = self.0.read().await;
        for client in clients.iter() {
            if client.client_id.eq(clid) {
                return Some(f(client));
            }
        }
        None
    }
}

impl EventListener for Clients {
    async fn listen_all<E>(&self, event: &E) 
        where E: Event + Send + Sync 
    {
        let listeners = self.0.read().await;
        
        for cval in listeners.iter() {
            let mut buffer = BytesMut::zeroed(512);
            if !cval.is_alive() {
                yield_now().await;
                if !cval.is_dead_time() {
                    continue;
                }
                
                self.remove(cval.conn_num)
                    .await
                    .unwrap();
            }

            match cval.listen(&mut buffer).await {
                Ok(0) => {
                    yield_now().await;

                    if cval.is_alive() {
                        cval.set_alive(false); 
                    }
                }, 
                Ok(_) => (), 
                Err(err) => { println!("err: {}", err.to_string()) }
            };
            
            let packet = MqttClientPacket::deserialize(&mut buffer).unwrap();
            match packet {
                MqttClientPacket::Publish(p) 
                    => event.enqueue_message(p),
                MqttClientPacket::Subscribe(s) 
                    => event.subscribe_topic(s, cval.conn_num).await
            }
        }
    }

    async fn count_listener(&self) -> usize {
        self.0.read().await.len()
    }
}

impl Consumer for Clients {
    async fn pubish(&self, con_id: u32, packet: crate::protocol::mqtt::PublishPacket) -> io::Result<()> {
        let clients = self.0.read().await;
        let idx = clients.binary_search_by(|c| con_id.cmp(&c.conn_num)).unwrap();
        let mut buffer = packet.serialize();
        clients[idx].write(&mut buffer).await
    }
}

impl Clone for Clients {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}