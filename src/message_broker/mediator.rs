use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use crate::protocol::mqtt::PublishPacket;
use super::{client::Client, linked_list::List};

type Clients = Vec<Client>;

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
        println!("{:?}", self.clients);
        let mut wr = self.clients.write().await;
        wr.push(client)
    }

    pub async fn check_session(&self, clid: &str, addr: &SocketAddr) -> Option<u32> {
        let read = self.clients.read().await;
        for c in read.iter() {
            if !c.client_id.eq(clid) {
                continue;
            }

            if c.addr.eq(addr) {
                return Some(c.conn_num);
            }
        }
        
        None
    }
}

impl MessageProducer for BrokerMediator {
    type T = PublishPacket;
    fn send(&self, val: Self::T) {
        self.message_queue.append(val)
    }
}

impl MessageConsumer for BrokerMediator {
    type T = Option<PublishPacket>;
    fn get(&self) -> Self::T {
        self.message_queue.take_first()
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