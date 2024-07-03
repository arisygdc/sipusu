use std::sync::{atomic::{AtomicPtr, Ordering}, Arc};
use tokio::{io, sync::RwLock};
use crate::{
    connection::SocketWriter, 
    message_broker::{Forwarder, SendStrategy}, protocol::v5::puback::{PubACKType, PubackPacket}
};
use super::{client::{Client, ClientID}, storage::ClientStore, SessionController};

pub type AtomicClient = Arc<AtomicPtr<Client>>;
type MutexClients = RwLock<Vec<AtomicClient>>;

/// When clients drop trigger [`SessionController`] kill for all client
pub struct Clients{
    list: Arc<MutexClients>,
    storage: ClientStore,
}

impl<'lc, 'st> Clients {
    pub async fn new() -> Self {
        Self{
            list: Arc::new(RwLock::new(Vec::new())),
            storage: ClientStore::new()
                .await
                .expect("cannot prepare client")
        }
    }

    /// insert sort by conn number
    pub async fn insert(&self, new_cl: Client) -> Result<(), String> {
        let new_clid = new_cl.clid.clone();
        let mut clients = self.list.write().await;

        let mut found = clients.len();
        for (i, cval) in clients.iter().enumerate() {
            let clid = unsafe {&(*cval.load(Ordering::Acquire)).clid};
            match new_clid.cmp(clid) {
                std::cmp::Ordering::Equal => return Err("duplicate client id".to_string()),
                std::cmp::Ordering::Greater => continue,
                std::cmp::Ordering::Less => ()
            }

            found = i;
            break;
        }

        let new_cl = Box::new(new_cl);
        let p = Box::into_raw(new_cl);
        let new_cl = AtomicPtr::new(p);
        let new_cl = Arc::new(new_cl);
        clients.insert(found, new_cl);
        drop(clients);

        self.storage.prepare(&new_clid).await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    // TODO: Create Garbage collector
    #[allow(dead_code)]
    pub async fn remove(&self, clid: &ClientID) -> Result<(), String> {
        let mut clients = self.list.write().await;
        let idx = clients.binary_search_by(|c| unsafe {
                (*c.load(Ordering::Relaxed)).clid.cmp(&clid)
            })
            .map_err(|_| format!("cannot find conn num {}", clid))?;
        let rm_ptr = clients[idx].load(Ordering::Relaxed);
        clients.remove(idx);

        drop(clients);
        unsafe { drop(Box::from_raw(rm_ptr)) }
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

    pub async fn session_exists(&self, clid: &ClientID) -> bool {
        let clients = self.list.read().await;
        clients.binary_search_by(|c| unsafe {
            let t = &(*c.load(Ordering::Acquire)).clid;
            t.cmp(clid)
        }).is_ok()
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
            client.write_all(packet)
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
        Self { list: self.list.clone(), storage: self.storage.clone() }
    }
}

impl Drop for Clients {
    fn drop(&mut self) {
        let a = self.list.clone();
        let b = Arc::into_inner(a);
        let val = match b {
            None => return,
            Some(val) => val
        };

        let val = val.into_inner();
        for v in val {
            unsafe{(*v.load(Ordering::Acquire)).kill()}
        }
    }
}