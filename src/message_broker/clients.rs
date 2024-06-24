use std::{sync::{atomic::{AtomicPtr, Ordering}, Arc}, time::Duration};
use bytes::BytesMut;
use tokio::{io, sync::RwLock};
use crate::{connection::{SocketReader, SocketWriter}, helper::time::sys_now, protocol::{mqtt::{MqttClientPacket, PublishPacket}, subscribe::{SubAckResult, SubscribeAck}}};
use super::{client::{Client, ClientID}, Consumer, Event, EventListener};

type MutexClients = RwLock<Vec<AtomicPtr<Client>>>;

pub struct Clients(Arc<MutexClients>);

impl<'lc> Clients {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(Vec::new())))
    }

    // FIXME: return error when equal
    /// insert sort by conn number
    pub async fn insert(&self, new_cl: Client) -> Result<(), String> {
        let mut clients = self.0.write().await;

        let mut found = clients.len();
        for (i, cval) in clients.iter().enumerate() {
            let clid = unsafe {&(*cval.load(Ordering::Acquire)).clid};
            match new_cl.clid.cmp(clid) {
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
        Ok(())
    }

    // TODO: Create Garbage collector
    #[allow(dead_code)]
    pub async fn remove(&self, clid: &ClientID) -> Result<(), String> {
        let mut clients = self.0.write().await;
        let idx = clients.binary_search_by(|c| unsafe {
                (*c.load(Ordering::Relaxed)).clid.cmp(&clid)
            })
            .map_err(|_| format!("cannot find conn num {}", clid))?;
        clients.remove(idx);
        Ok(())
    }

    /// binary search by client id
    /// in action read guard from vector
    /// give you access to mutable reference on client
    pub async fn search_mut_client<R>(&self, clid: &ClientID, f: impl FnOnce(&'lc mut Client) -> R) -> Option<R> {
        let clients = self.0.read().await;

        let idx = clients.binary_search_by(|c| unsafe {
            (*c.load(Ordering::Relaxed)).clid.cmp(&clid)
        }).ok()?;
        
        let cl = unsafe {&mut (*clients[idx].load(Ordering::Relaxed))};
        Some(f(cl))
    }

    pub async fn session_exists(&self, clid: &ClientID) -> bool {
        let clients = self.0.read().await;
        clients.binary_search_by(|c| unsafe {
            let t = &(*c.load(Ordering::Acquire)).clid;
            t.cmp(clid)
        }).is_ok()
    }
}

impl EventListener for Clients {
    async fn listen_all<E>(&self, event: &E) 
        where E: Event + Send + Sync 
    {
        let listeners = self.0.read().await;
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

            let mut buffer = BytesMut::zeroed(512);
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

            let mut buffer = buffer.split_to(n);
            
            let packet = MqttClientPacket::deserialize(&mut buffer).unwrap();
            match packet {
                MqttClientPacket::Publish(p) 
                    => {event.enqueue_message(p); println!("enqueue message")},
                MqttClientPacket::Subscribe(subs) 
                    => {
                        let result: Vec<SubAckResult> = event.subscribe_topics(subs.list, cval.clid.clone()).await;
                        let response = SubscribeAck{ id: subs.id, subs_result: result };
                        
                        let id = response.id;
                        let result = cval.write_all(&mut response.serialize()).await;
                        if let Err(e) = result {
                            println!("[{}] {}", id, e.to_string());
                        }
                    }
            }
        }
    }

    async fn count_listener(&self) -> usize {
        let listener = self.0.read().await;
        listener.len()
    }
}

impl Consumer for Clients {
    async fn pubish(&self, clid: ClientID, packet: PublishPacket) -> io::Result<()> {
        let mut buffer = packet.serialize();
        let res = self.search_mut_client(&clid, |c| {
            println!("send to: {}", c.addr);
            c.write_all(&mut buffer)
        }).await;

        if let Some(writer) = res {
            return writer.await;
        }
        
        Err(io::Error::new(io::ErrorKind::NotFound, format!("client id {}", clid)))
    }
}

impl Clone for Clients {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}   

/// time base session control for `signaling`
/// 
/// compare internal variable with given time,
/// there is 3 main concept: alive, dead, expired.
/// 
/// alive and death indicates you may use this session or not.
/// when session is expire, you dont need to hold this connection.
pub trait SessionController {
    fn is_alive(&self, t: u64) -> bool;
    fn kill(&mut self);
    /// set `t` as checkpoint then add keep alive duration,
    /// generate error when old duration less than `t`
    fn keep_alive(&mut self, t: u64) -> Result<u64, String>;
    fn is_expired(&self, t: u64) -> bool;
}