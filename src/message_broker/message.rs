use std::sync::Arc;

use crate::{ds::{linked_list::List, GetFromQueue, InsertQueue}, protocol::v5::publish::PublishPacket};
use super::{cleanup::Cleanup, client::clobj::ClientID};

#[derive(Default)]
pub struct Message {
    pub publisher: Option<ClientID>,
    pub packet: PublishPacket
}

pub type MessageQueue = Arc<List<Message>>;

impl<Message: Default> InsertQueue<Message> for Arc<List<Message>>
{
    fn enqueue(&self, val: Message) {
        self.append(val)
    }
}

impl<Message: Default> GetFromQueue<Message> for Arc<List<Message>>{
    fn dequeue(&self) -> Option<Message> {
        self.take_first()
    }
}

impl Cleanup for MessageQueue {
    async fn clear(self) {
        println!("clean message")
    }
}