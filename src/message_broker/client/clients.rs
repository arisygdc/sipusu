use std::sync::{atomic::{AtomicPtr, Ordering}, Arc};
use tokio::{io, sync::RwLock};
use crate::{connection::SocketWriter, helper::time::sys_now, message_broker::{cleanup::Cleanup, client::storage::{EventType, WALL}, Forwarder, SendStrategy}};
use crate::protocol::v5::puback::{PubACKType, PubackPacket};
use super::{client::Client, clobj::ClientID, SessionController};

pub type AtomicClient = Arc<AtomicPtr<Client>>;
type MutexClients = RwLock<Vec<AtomicClient>>;

pub struct Clients{
    list: Arc<MutexClients>,
}

impl<'lc, 'st> Clients {
    pub async fn new() -> Self {
        Self{
            list: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// insert sort by conn number
    pub async fn insert(&self, new_cl: Client) -> Result<(), String> {
        let new_clid = new_cl.clid.clone();
        let new_cl = Box::new(new_cl);
        
        let mut clients = self.list.write().await;
        let mut found = clients.len();
        let mut swap = false;
        let mut i = 0;
        'insert: while i < clients.len() {
            let clid = unsafe {&(*clients[i].load(Ordering::Relaxed)).clid};
            match new_clid.cmp(clid) {
                std::cmp::Ordering::Equal => {
                    let is_available = unsafe {(*clients[i].load(Ordering::Relaxed)).is_alive(sys_now())};
                    if is_available {
                        return Err("duplicate client id".to_string())
                    }
                    swap = true;
                },
                std::cmp::Ordering::Greater => {
                    i += 1;
                    continue 'insert
                },
                std::cmp::Ordering::Less => ()
            }
            
            found = i;
            break 'insert;
        }

        let p = Box::into_raw(new_cl);
        if swap {
            let odl_cl = clients[i].swap(p, Ordering::AcqRel);
            drop(clients);
            unsafe{drop(Box::from_raw(odl_cl))}
        } else {
            let new_cl = AtomicPtr::new(p);
            let new_cl = Arc::new(new_cl);
            clients.insert(found, new_cl);
        }

        Ok(())
    }

    /// binary search by client id
    /// in action read guard from vector
    /// give you access to mutable reference on client
    pub async fn search_mut_client<R>(&self, clid: &ClientID, f: impl FnOnce(&'lc mut Client) -> R) -> Option<R> {
        let clients = self.list.read().await;

        let idx = clients.binary_search_by(|c| unsafe {
            (*c.load(Ordering::Relaxed)).clid.cmp(&clid)
        }).ok()?;
        
        let cl = unsafe {&mut (*clients[idx].load(Ordering::Relaxed))};
        Some(f(cl))
    }

    pub async unsafe fn get_client(&self, clid: &ClientID) -> Option<AtomicClient> {
        let clients = self.list.read().await;
        let idx = clients.binary_search_by(|c| {
            (*c.load(Ordering::Relaxed)).clid.cmp(&clid)
        }).ok()?;

        Some(clients[idx].clone())
    }
}

impl SendStrategy for Clients
{
    async fn qos0(&self, subscriber: &ClientID, buffer: &[u8]) {
        let _ = self.pubish(subscriber, buffer).await;
    }

    async fn qos1(
        &self,
        publisher: &ClientID,
        subscriber: &ClientID,
        packet_id: u16,
        buffer: &[u8]
    ) -> std::io::Result<()> {
        self.pubish(subscriber, buffer).await?;
        let puback = PubackPacket {
            packet_id,
            packet_type: PubACKType::PubAck,
            properties: None,
            reason_code: 0
        };

        let buffer = puback.encode().unwrap();
        self.pubish(publisher, &buffer).await
    }

    async fn qos2(
        &self, 
        publisher: &ClientID,
        subscriber: &ClientID,
        packet_id: u16,
        buffer: &[u8]
    ) -> std::io::Result<()> {
        let msg_buffer = buffer;

        // Pub Rec
        let mut ack = PubackPacket {
            packet_id,
            packet_type: PubACKType::PubRec,
            properties: None,
            reason_code: 0x00
        };
        let buffer = ack.encode().unwrap();
        self.pubish(&publisher, &buffer).await?;

        // Wait Pub Rel
        // ...
        println!("TODO: wait pubrel");

        // Publish Message
        self.pubish(&subscriber, &msg_buffer).await?;
        
        // Pub Comp
        ack.packet_type = PubACKType::PubRel;
        let buffer = ack.encode().unwrap();
        self.pubish(&publisher, &buffer).await?;
        Ok(())
    }
}

impl Forwarder for Clients {
    async fn pubish(&self, con_id: &ClientID, packet: &[u8]) -> io::Result<()> {
        let found = self.search_mut_client(&con_id, |client| {
            client.socket.write_all(packet)
        }).await;
    
        let res = match found {
            None => return Err(
                io::Error::new(
                    io::ErrorKind::NotFound, 
                    format!("client {} not found", con_id)
                )),
            Some(fut) => fut.await
        };
    
        res
    }
}

impl Clone for Clients {
    fn clone(&self) -> Self {
        Self { list: Arc::clone(&self.list) }
    }
}

impl Cleanup for Clients {
    async fn clear(self) {
        let mut clients = self.list.write().await;
        if clients.is_empty() {
            return ;
        }

        for _ in 0..clients.len() {
            let opc = clients.pop();
            let _cl = match opc {
                Some(cl) => cl,
                None => continue
            };
            let _take_cl = unsafe {Box::from_raw(_cl.load(Ordering::Acquire))};
            let expired_at = _take_cl.expiration_time();
            let _res = 
            _take_cl.storage.log_session(&[WALL{
                    time: sys_now(), 
                    value: EventType::DisconnectByServer(expired_at)}
            ]).await;
            println!("[Cleanup] {}", _take_cl.clid);
        }
    }
}