use std::{sync::{atomic::{AtomicPtr, Ordering}, Arc}, time::Duration};
use bytes::BytesMut;
use tokio::{io, sync::RwLock};
use crate::{connection::{SocketReader, SocketWriter}, helper::time::sys_now, message_broker::{Consumer, Event, EventListener}, protocol::{mqtt::{MqttClientPacket, PublishPacket}, subscribe::{SubAckResult, SubscribeAck}}};
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
        clients.remove(idx);
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
    async fn listen_all<E>(&self, event: &E) 
        where E: Event + Send + Sync 
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
                    => event.enqueue_message(p),
                MqttClientPacket::Subscribe(subs) 
                    => {
                        let clid = cval.clid.clone();
                        let result: Vec<SubAckResult> = event.subscribe_topics(&subs.list, clid.clone()).await;
                        let response = SubscribeAck{ id: subs.id, subs_result: result };
                        
                        let id = response.id;
                        let mut srz = response.serialize();

                        let (response, store) = tokio::join!(
                            cval.write_all(&mut srz),
                            self.storage.subscribe(&clid, &subs.list)
                        );
                        if let Err(e) = response {
                            println!("[{}] {}", id, e.to_string());
                        }

                        if let Err(e) = store {
                            println!("[{}] {}", id, e.to_string());
                        }
                        
                    }
            }
        }
    }

    async fn count_listener(&self) -> usize {
        let listener = self.list.read().await;
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
        Self { list: self.list.clone(), storage: self.storage.clone() }
    }
}