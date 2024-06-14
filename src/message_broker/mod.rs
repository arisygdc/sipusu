use std::io;

use crate::{connection::ConnectionID, protocol::mqtt::{PublishPacket, Subscribe}};

pub mod client;
pub mod mediator;
mod trie;
mod linked_list;
mod provider;
// producer
// - listener(event)

// consumer
// - take
// - lookup(producer) 

pub trait Event {
    fn enqueue_message(&self, msg: PublishPacket);
    fn subscribe_topic(&self, sub: Subscribe, con_id: ConnectionID) -> impl std::future::Future<Output = ()> + Send;
}

pub trait EventListener {
    fn listen_all<E>(&self, event: &E) -> impl std::future::Future<Output = ()> + Send
        where E: Event + Send + Sync;

    fn count_listener(&self) -> impl std::future::Future<Output = usize> + Send;
}

pub trait Consumer {
    fn pubish(&self, con_id: ConnectionID, packet: PublishPacket) -> impl std::future::Future<Output = io::Result<()>> + Send;
}

pub trait Messanger {
    fn dequeue_message(&self) -> Option<PublishPacket>;
    fn route(&self, topic: &str) -> impl std::future::Future<Output =  Option<Vec<ConnectionID>>> + Send;
}