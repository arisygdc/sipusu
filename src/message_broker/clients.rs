use std::{pin::Pin, sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{io, sync::RwLock, task::yield_now};
use crate::{connection::ConnectionID, protocol::{mqtt::{MqttClientPacket, PublishPacket}, subscribe::{SubAckResult, SubscribeAck}}};
use super::{client::{Client, ClientID}, Consumer, Event, EventListener};

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
            match new_cl.clid.cmp(&cval.clid) {
                std::cmp::Ordering::Equal => panic!("kok iso"),
                std::cmp::Ordering::Greater => continue,
                std::cmp::Ordering::Less => ()
            }

            found = i;
            break;
        }
        clients.insert(found, new_cl);
    }

    // TODO: Create Garbage collector
    #[allow(dead_code)]
    pub async fn remove(&self, conid: ConnectionID) -> Result<(), String> {
        let mut clients = self.0.write().await;
        let idx = clients.binary_search_by(|c| c.conid.cmp(&conid))
            .map_err(|_| format!("cannot find conn num {}", conid))?;
        clients.remove(idx);
        Ok(())
    }

    /// binary search by connection id
    /// in action lock all value on the vector
    /// give you access to mutable reference on client
    pub async fn search_mut_client<R>(&self, clid: &ClientID, f: impl FnOnce(&mut Client) -> R) -> Option<R> {
        let mut clients = self.0.write().await;

        let idx = clients.binary_search_by(|c| c.clid.cmp(clid)).ok()?;
        let r = f(&mut clients[idx]);
        Some(r)
    }

    pub async fn search_clid_mut<R>(&self, raw_clid: &str, f: impl FnOnce(&mut Client) -> R) -> Option<R> {
        let clid = ClientID::new(raw_clid.to_owned());
        self.search_mut_client(&clid, f).await
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
            }

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
                        let result: Vec<SubAckResult> = event.subscribe_topics(subs.list, cval.conid.clone()).await;
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
    async fn pubish(&self, con_id: ConnectionID, packet: PublishPacket) -> io::Result<()> {
        let clients = self.0.read().await;
        let idx = clients.binary_search_by(|c| c.conid.cmp(&con_id)).unwrap();
        let mut buffer = packet.serialize();
        let client = &clients[idx];
        println!("send to: {}", client.addr);
        client.write(&mut buffer).await
    }
}

impl Clone for Clients {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}   
