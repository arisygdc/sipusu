#![allow(dead_code)]
use std::{sync::Arc, time::Duration};

use bytes::BytesMut;
use client::Client;
use tokio::{sync::RwLock, task::yield_now, time};

use crate::protocol::mqtt::{MqttClientPacket, PublishPacket, SubscribePacket};

pub mod client;
pub mod mediator;
mod linked_list;
mod producer;
mod consumer;

// producer
// - listener(event)

// consumer
// - take
// - lookup(producer) 

pub struct SubscribeRequest {
    req: SubscribePacket,
    con_id: u32
}

impl SubscribeRequest {
    pub fn new(req: SubscribePacket, con_id: u32) -> Self {
        Self { req, con_id }
    }
}

pub trait Event {
    fn enqueue_message(&self, msg: PublishPacket);
    fn subscribe_topic(&self, sub: SubscribeRequest);
}

pub trait EventListener {
    fn listen<E>(&self, event: E) -> impl std::future::Future<Output = ()> + Send
        where E: Event + Send + Sync;
}

type MutexClients = RwLock<Vec<Client>>;

pub struct Clients(Arc<MutexClients>);

impl Clients {
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
}

impl EventListener for Clients {
    async fn listen<E>(&self, event: E) 
        where E: Event + Send + Sync 
    {
        let listeners = self.0.read().await;
        if listeners.len() == 0 {
            time::sleep(Duration::from_millis(5)).await;
            if listeners.len() == 0 { return ; }
        }
        
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
                    => event.subscribe_topic(SubscribeRequest::new(s, cval.conn_num)) 
            }
        }
    }
}

