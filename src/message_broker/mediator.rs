use std::sync::Arc;
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
        let mut wr = self.clients.write().await;
        wr.push(client)
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