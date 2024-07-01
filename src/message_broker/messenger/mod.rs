use crate::protocol::v5::publish::PublishPacket;
use super::{client::client::ClientID, Forwarder};
mod msg_ack;
pub mod distributor;

pub trait DistMsgStrategy<S> 
where S: Forwarder + Send + Sync + 'static
{
    fn qos0(&self, sender: S, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String>;
    fn qos1(&self, sender: S, publisher: ClientID, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String>;
    fn qos2(&self, sender: S, publisher: ClientID, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String>;
}