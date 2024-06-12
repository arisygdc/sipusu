use std::{io, net::SocketAddr, sync::Arc, time::Duration};
use dashmap::DashMap;
use tokio::{sync::{Mutex, RwLock}, task::JoinHandle, time};
use crate::protocol::mqtt::PublishPacket;
use super::{client::Client, consumer::Consumer, linked_list::List, producer::Producer};

pub(super) type Clients = Vec<Mutex<Client>>;

pub struct BrokerMediator {
    clients: Arc<RwLock<Clients>>,
    message_queue: Arc<List<PublishPacket>>,
    subscriber: Arc<DashMap<String, Clients>>
}

impl BrokerMediator {
    pub fn new() -> Self {
        let clients = Arc::new(RwLock::new(Vec::new()));
        let message_queue = Arc::new(List::new());
        let subscriber: Arc<DashMap<String, Clients>> = Arc::new(DashMap::new());
        Self{ clients, message_queue, subscriber }
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

    pub fn join_handle(&mut self) -> (JoinHandle<()>, JoinHandle<()>) {
        let producer = Producer::new(
            self.clients.clone(),
            self.message_queue.clone()
        );
    
        let consumer = Consumer::new(
            self.message_queue.clone(),
            self.subscriber.clone(),
        );
    
        let t1 = tokio::spawn(event_listener(producer));
        let t2 = tokio::spawn(observer_message(consumer));
        (t1, t2)
    }
}

async fn event_listener<P>(producer: P) 
    where P: MessageProducer + Send + Sync
{
    loop { producer.listen_clients().await; }
}

async fn observer_message<C>(consumer: C)
    where C: MessageConsumer + Send + Sync
{
    loop {
        if let Some(packet) = consumer.take() {
            consumer.broadcast(packet).await.unwrap();
        }
        time::sleep(Duration::from_millis(10)).await;
    }
}

pub trait MessageProducer {
    fn listen_clients(&self) -> impl std::future::Future<Output = ()> + Send;
}

pub trait MessageConsumer {
    type T;
    fn take(&self) -> Option<Self::T>;
    // TODO: maybe list unsent message with client identier
    fn broadcast(&self, message: Self::T) -> impl std::future::Future<Output = io::Result<()>> + Send;
}