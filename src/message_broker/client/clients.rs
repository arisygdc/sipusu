use std::{sync::{atomic::{AtomicPtr, Ordering}, Arc}, time::Duration};
use bytes::BytesMut;
use tokio::{io, sync::RwLock};
use crate::{
    connection::{SocketReader, SocketWriter}, 
    helper::time::sys_now, 
    message_broker::{ClientReqHandle, EventListener, Forwarder, Message}, 
    protocol::{
        mqtt::{ClientPacketV5, PING_RES}, 
        v5::subsack::SubsAck
    }
};
use super::{client::{Client, ClientID}, storage::ClientStore, SessionController};

type MutexClients = RwLock<Vec<AtomicPtr<Client>>>;

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

    // FIXME: return error when equal
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
}

impl EventListener for Clients {
    async fn listen_all<E>(&self, handle: &E) 
        where E: ClientReqHandle + Send + Sync 
    {
        let listeners = self.list.read().await;
        let sys_time = sys_now();
        for client in listeners.iter() {
            let cval = unsafe{&mut *client.load(Ordering::Relaxed)};
            if !cval.is_alive(sys_time) {
                if cval.is_expired(sys_time) {
                    // TODO: Remove expired
                    continue;
                }
                continue;
            }

            let mut buffer = BytesMut::zeroed(128);
            let duration = Duration::from_millis(10);
            let read_result = cval.read_timeout(&mut buffer, duration).await;

            let n = match read_result {
                Ok(n) => n, 
                Err(_) => continue
            };

            if n == 0 {
                cval.kill();
                continue;
            }

            cval.keep_alive(sys_time+1)
                .unwrap();

            println!();
            let mut buffer = buffer.split_to(n);
            let packet = ClientPacketV5::decode(&mut buffer).unwrap();
            match packet {
                ClientPacketV5::Publish(pubs) => {
                    let mut msg = Message {
                        packet: pubs,
                        publisher: None
                    };

                    if msg.packet.qos.code() > 0 {
                        msg.publisher = Some(cval.clid.clone());
                    }

                    handle.enqueue_message(msg)
                },
                ClientPacketV5::Subscribe(subs) => {
                    let res = handle.subscribe_topics(&subs.list, cval.clid.clone()).await;
                    
                    let response = SubsAck{
                        id: subs.id,
                        properties: None,
                        return_codes: res
                    };

                    let buffer = response.encode().unwrap();
                    let clid = cval.clid.clone();
                    let net = cval.write_all(&buffer);
                    let log = self.storage.subscribe(&clid, &subs.list);
                    let _ = tokio::join!(net, log);
                },
                ClientPacketV5::PingReq => {
                    cval.write_all(&PING_RES).await.unwrap();
                }
            }
        }
    }

    async fn count_listener(&self) -> usize {
        let listener = self.list.read().await;
        listener.len()
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