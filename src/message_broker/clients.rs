use std::{pin::Pin, sync::{atomic::{AtomicPtr, Ordering}, Arc}, time::Duration};
use bytes::BytesMut;
use tokio::{io, sync::RwLock, task::yield_now};
use crate::protocol::{mqtt::{MqttClientPacket, PublishPacket}, subscribe::{SubAckResult, SubscribeAck}};
use super::{client::{Client, ClientID}, Consumer, Event, EventListener};

type MutexClients = RwLock<Vec<AtomicPtr<Client>>>;

pub struct Clients(Arc<MutexClients>);

impl<'lc> Clients {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(Vec::new())))
    }

    // FIXME: return error when equal
    /// insert sort by conn number
    pub async fn insert(&self, new_cl: Client) {
        let mut clients = self.0.write().await;

        let mut found = clients.len();
        for (i, cval) in clients.iter().enumerate() {
            let clid = unsafe {&(*cval.load(Ordering::Acquire)).clid};
            match new_cl.clid.cmp(clid) {
                std::cmp::Ordering::Equal => panic!("kok iso"),
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

    /// binary search by connection id
    /// in action lock all value on the vector
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
        
        for client in listeners.iter() {
            let cval = Box::pin(unsafe{&*client.load(Ordering::Relaxed)});
            if !cval.is_alive() {
                yield_now().await;
                if !cval.is_dead_time() {
                    continue;
                }
            }

            let mut buffer = BytesMut::zeroed(512);
            let read = tokio::time::timeout(
                Duration::from_millis(10), 
                cval.listen(&mut buffer)
            ).await;

            let read_result;
            if let Ok(res) = read {
                read_result = res;
            } else {
                continue;
            }

            let n = match read_result {
                Ok(n) => n, 
                Err(err) => { 
                    println!("err: {}", err.to_string());
                    continue;
                }
            };

            if n == 0 {
                yield_now().await;

                if cval.is_alive() {
                    cval.set_alive(false); 
                }
                continue;
            }

            let mut buffer = buffer.split_to(n);
            
            let packet = MqttClientPacket::deserialize(&mut buffer).unwrap();
            match packet {
                MqttClientPacket::Publish(p) 
                    => {event.enqueue_message(p); println!("enqueue message")},
                MqttClientPacket::Subscribe(subs) 
                    => {
                        let result: Vec<SubAckResult> = event.subscribe_topics(subs.list, cval.clid.clone()).await;
                        let response = SubscribeAck{ id: subs.id, subs_result: result };
                        
                        let pin = Pin::new(cval);
                        let id = response.id;
                        let result = pin.write(&mut response.serialize()).await;
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
            c.write(&mut buffer)
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
