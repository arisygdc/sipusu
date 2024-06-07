use std::{net::SocketAddr, sync::{atomic::{AtomicPtr, Ordering}, Arc}, time::Duration};
use bytes::BytesMut;
use tokio::{sync::RwLock, task::{yield_now, JoinHandle}, time};
use crate::protocol::mqtt::PublishPacket;
use super::{client::Client, linked_list::List};

type Clients = Vec<AtomicPtr<Client>>;

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
        let mut client = client;
        println!("{:?}", self.clients);
        let value = AtomicPtr::new(&mut client);
        let mut wr = self.clients.write().await;
        wr.push(value)
    }

    pub async fn check_session(&self, clid: &str, addr: &SocketAddr) -> Option<u32> {
        let read = self.clients.read().await;
        for c in read.iter() {
            let c = c.load(Ordering::Relaxed);
            unsafe {
                if !(*c).client_id.eq(clid) {
                    continue;
                }
    
                if (*c).addr.eq(addr) {
                    return Some((*c).conn_num);
                }
            }
        }
        
        None
    }

    pub fn run(&self) -> JoinHandle<()> {
        let producer = Producer {
            clients: self.clients.clone(),
            message_queue: self.message_queue.clone()
        };

        let future = async move {
            loop { unsafe{ producer.listen_many().await } }
        };
        tokio::spawn(future)
    }
}

impl MessageConsumer for BrokerMediator {
    type T = Option<PublishPacket>;
    fn get(&self) -> Self::T {
        self.message_queue.take_first()
    }
}

struct Producer {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>
}

impl Producer {
    async unsafe fn listen_many(&self) {
        let listen = &self.clients.read().await;
        while listen.len() == 0 {
            yield_now().await;
            time::sleep(Duration::from_millis(5)).await;
        }
    
        for c in listen.iter() {
            let mut buffer = BytesMut::new();
            let cval = c.load(Ordering::Relaxed);
            match (*cval).listen(&mut buffer).await {
                Ok(0) => {
                    let cval = c.load(Ordering::Acquire);
                    (*cval).set_alive(false);
                }, Ok(_) => {
                    let packet = PublishPacket::deserialize(&mut buffer).unwrap();
                    println!("topic: {}, payload {}", packet.topic, String::from_utf8(packet.payload).unwrap())
                }, Err(err) => { println!("err: {}", err.to_string()) }
            };
        }
    }
}

impl MessageProducer for Producer {
    type T = PublishPacket;
    fn send(&self, val: Self::T) {
        self.message_queue.append(val)
    }
}

pub trait MessageProducer {
    type T;
    fn send(&self, val: Self::T);
}

pub trait MessageConsumer {
    type T;
    fn get(&self) -> Self::T;
}