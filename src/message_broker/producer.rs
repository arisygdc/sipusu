use std::{sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{sync::{RwLock, RwLockReadGuard}, task::yield_now, time};
use crate::protocol::mqtt::PublishPacket;
use super::{linked_list::List, mediator::{Clients, MessageProducer}};

pub struct Producer {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>
}

impl Producer {
    pub fn new(clients: Arc<RwLock<Clients>>, message_queue: Arc<List<PublishPacket>>) -> Self {
        Self { clients, message_queue }
    }

    async fn iterate_listeners(&self, listeners: RwLockReadGuard<'_, Clients>) {
        for c in listeners.iter() {
            let mut buffer = BytesMut::zeroed(512);
            let mut cval = c.lock().await;
            if !cval.is_alive() {
                continue;
            }

            match cval.listen(&mut buffer).await {
                Ok(0) => {
                    if !cval.is_alive() {
                        yield_now().await;
                        if cval.is_dead_time() {
                            // TODO: remove the client when is dead
                        }
                        continue;
                    }
                    cval.set_alive(false);
                    yield_now().await;
                }, Ok(_) => {
                    let packet = PublishPacket::deserialize(&mut buffer).unwrap();
                    println!("[packet] topic: {}, payload {}", packet.topic, String::from_utf8(packet.payload.clone()).unwrap());
                    self.message_queue.append(packet);
                }, Err(err) => { println!("err: {}", err.to_string()) }
            };
        }
    }
}

impl MessageProducer for Producer {
    async fn listen_clients(&self) {
        let listen = self.clients.read().await;
        if listen.len() == 0 {
            time::sleep(Duration::from_millis(5)).await;
            return ;
        }
        
        self.iterate_listeners(listen).await;
    }
}