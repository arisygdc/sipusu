use std::{net::SocketAddr, sync::Arc, time::Duration};
use bytes::BytesMut;
use dashmap::DashMap;
use tokio::{sync::{Mutex, RwLock}, task::{yield_now, JoinHandle}, time};
use crate::protocol::mqtt::PublishPacket;
use super::{client::Client, linked_list::List};

type Clients = Vec<Mutex<Client>>;

pub struct BrokerMediator {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>
}

impl BrokerMediator {
    pub fn new() -> Self {
        let clients = Arc::new(RwLock::new(Vec::new()));
        let message_queue = Arc::new(List::new());
        Self{ clients, message_queue }
    }
}

impl BrokerMediator {
    pub async fn register(&self, client: Client) {
        let client = client;
        println!("[register] client {:?}", client);
        let value = Mutex::new(client);
        let mut wr = self.clients.write().await;
        wr.push(value);
    }

    pub async fn wakeup_exists(&self, clid: &str, addr: &SocketAddr) -> Option<u32> {
        let read = self.clients.read().await;
        for c in read.iter() {
            let mut c = c.lock().await;
            if !c.client_id.eq(clid) {
                continue;
            }

            if c.addr.eq(addr) {
                c.set_alive(true);
                return Some(c.conn_num);
            }
        }
        
        None
    }

    pub fn run(&self) -> (JoinHandle<()>, JoinHandle<()>) {
        let producer = Producer {
            clients: self.clients.clone(),
            message_queue: self.message_queue.clone()
        };

        let consumer = Consumer {
            forward: Arc::new(DashMap::new()),
            message_queue: self.message_queue.clone()
        };

        let future = async move {
            println!("[entering] broker");
            let producer = producer;
            loop { 
                unsafe{ producer.listen_many().await } 
            }
        };
        let t1 = tokio::spawn(future);
        let t2 = tokio::spawn(async move {
            loop {
                if let Some(mut v) = consumer.message_queue.take_first() {
                    println!("[msg thread] {}", String::from_utf8(v.payload.clone()).unwrap());
                    // TODO: sequential write into disk
                    let mut clients = match consumer.forward.get_mut(&v.topic) {
                        Some(c) => c,
                        None => {
                            println!("find a way to save unsent message");
                            continue;
                        }
                    };

                    if clients.len() == 0 {
                        println!("find a way to save unsent message");
                        continue;
                    }

                    for c in clients.iter_mut() {
                        let mut c = c.lock().await;
                        c.write(&mut v.payload).await.unwrap();
                    }

                    
                    continue;
                }
                time::sleep(Duration::from_millis(10)).await;
            }
        });
        (t1, t2)
    }
}

struct Consumer {
    message_queue: Arc<List<PublishPacket>>,
    forward: Arc<DashMap<String, Clients>>
}

struct Producer {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>
}

impl Producer {
    async unsafe fn listen_many(&self) {
        let listen = self.clients.read().await;
        if listen.len() == 0 {
            time::sleep(Duration::from_millis(5)).await;
            return ;
        }
        
        for c in listen.iter() {
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
        // println!("[looooooppp]")
    }
}

// impl MessageConsumer for BrokerMediator {
//     type T = Option<PublishPacket>;
//     fn get(&self) -> Self::T {
//         self.message_queue.take_first()
//     }
// }

// struct MessageObserver {
//     message_queue: Arc<List<PublishPacket>>
// }

// impl MessageProducer for Producer {
//     type T = PublishPacket;
//     fn send(&self, val: Self::T) {
//         self.message_queue.append(val)
//     }
// }

// pub trait MessageProducer {
//     type T;
//     fn send(&self, val: Self::T);
// }

// pub trait MessageConsumer {
//     type T;
//     fn get(&self) -> Self::T;
// }