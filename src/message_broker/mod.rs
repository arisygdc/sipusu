use std::{future::Future, io};

use client::clobj::ClientID;

mod msg_state;
pub mod client;
pub mod mediator;
pub mod cleanup;
mod message;
mod router;

pub const MAX_QOS: u8 = 2;
pub const WILDCARD_SUPPORT: bool = false;
pub const SUBS_ID_SUPPORT: bool = false;
pub const SHARED_SUBS_SUPPORT: bool = false;

pub trait SendStrategy: Forwarder + Send + Sync
{
    fn qos0(&self, subscriber: &ClientID, buffer: &[u8]) -> impl Future<Output = ()> + Send;
    fn qos1(
        &self, 
        publisher: &ClientID, 
        subscriber: &ClientID, 
        packet_id: u16, 
        buffer: &[u8]
    ) -> impl Future<Output = io::Result<()>> + Send;

    fn qos2(
        &self, 
        publisher: &ClientID, 
        subscriber: &ClientID, 
        packet_id: u16, 
        buffer: &[u8]
    ) -> impl Future<Output = io::Result<()>> + Send;
}

pub trait Forwarder {
    fn pubish(&self, clid: &ClientID, packet: &[u8]) -> impl Future<Output = io::Result<()>> + Send;
}
