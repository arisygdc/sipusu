use std::io;
use client::client::ClientID;

use crate::protocol::v5::{publish::PublishPacket, subsack::SubAckResult, subscribe::Subscribe};

mod msg_state;
pub mod client;
pub mod mediator;

pub const MAX_QOS: u8 = 2;
pub const WILDCARD_SUPPORT: bool = false;
pub const SUBS_ID_SUPPORT: bool = false;
pub const SHARED_SUBS_SUPPORT: bool = false;

#[derive(Default)]
pub struct Message {
    pub publisher: Option<ClientID>,
    pub packet: PublishPacket
}

pub trait SendStrategy: Forwarder + Send + Sync
{
    async fn qos0(&self, subscriber: &ClientID, buffer: &[u8]);
    async fn qos1(
        &self, 
        publisher: &ClientID, 
        subscriber: &ClientID, 
        packet_id: u16, 
        buffer: &[u8]
    ) -> std::io::Result<()>;

    async fn qos2(
        &self, 
        publisher: &ClientID, 
        subscriber: &ClientID, 
        packet_id: u16, 
        buffer: &[u8]
    ) -> std::io::Result<()>;
}

pub trait Forwarder {
    fn pubish(&self, clid: &ClientID, packet: &[u8]) -> impl std::future::Future<Output = io::Result<()>> + Send;
}
