use std::{net::SocketAddr, sync::Arc, time::Duration};
use bytes::BytesMut;
use tokio::{sync::{Mutex, RwLock}, task::JoinHandle, time};
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

    pub async fn check_session(&self, clid: &str, addr: &SocketAddr) -> Option<u32> {
        let read = self.clients.read().await;
        for c in read.iter() {
            let c = c.lock().await;
            if !c.client_id.eq(clid) {
                continue;
            }

            if c.addr.eq(addr) {
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

        let msg = self.message_queue.clone();

        let future = async move {
            println!("[entering] broker");
            let producer = producer;
            loop { 
                time::sleep(Duration::from_millis(10)).await;
                unsafe{ producer.listen_many().await } 
            }
        };
        let t1 = tokio::spawn(future);
        let t2 = tokio::spawn(async move {
            loop {
                if let Some(v) = msg.take_first() {
                    println!("[msg thread] {}", String::from_utf8(v.payload).unwrap());
                    continue;
                }
                time::sleep(Duration::from_millis(5)).await;
            }
        });
        (t1, t2)
    }
}

struct Producer {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>
}

impl Producer {
    async unsafe fn listen_many(&self) {
        let listen = self.clients.read().await;
        if listen.len() == 0 {
            return ;
        }
        
        for c in listen.iter() {
            let mut buffer = BytesMut::zeroed(512);
            let mut cval = c.lock().await;
            // println!("{:?}", cval);
            match cval.listen(&mut buffer).await {
                Ok(0) => {
                    // println!("[closed] {}", cval.client_id);
                    cval.set_alive(false);
                    continue;
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