use std::io;
use client::client::ClientID;

use crate::protocol::v5::{publish::PublishPacket, subsack::SubAckResult, subscribe::Subscribe};

mod messenger;
pub mod client;
pub mod mediator;
mod provider;

pub const MAX_QOS: u8 = 2;
pub const WILDCARD_SUPPORT: bool = false;
pub const SUBS_ID_SUPPORT: bool = false;
pub const SHARED_SUBS_SUPPORT: bool = false;

#[derive(Default)]
pub struct Message {
    pub publisher: Option<ClientID>,
    pub packet: PublishPacket
}

pub trait ClientReqHandle {
    fn enqueue_message(&self, msg: Message);
    fn subscribe_topics(&self, sub: &[Subscribe], clid: ClientID) -> impl std::future::Future<Output = Vec<SubAckResult>> + Send;
}

pub trait EventListener {
    fn listen_all<E>(&self, event: &E) -> impl std::future::Future<Output = ()> + Send
        where E: ClientReqHandle + Send + Sync;

    fn count_listener(&self) -> impl std::future::Future<Output = usize> + Send;
}

pub trait Forwarder {
    fn pubish(&self, clid: &ClientID, packet: &[u8]) -> impl std::future::Future<Output = io::Result<()>> + Send;
}

pub trait Messanger {
    fn dequeue_message(&self) -> Option<Message>;
    fn route(&self, topic: &str) -> impl std::future::Future<Output =  Option<Vec<ClientID>>> + Send;
}