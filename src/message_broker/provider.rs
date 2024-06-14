use std::sync::Arc;
use crate::protocol::mqtt::{PublishPacket, SubscribePacket};

use super::{linked_list::List, trie::Trie, Event, Messanger};

#[derive(Clone)]
pub struct EventHandler {
    message_queue: Arc<List<PublishPacket>>,
    router: Arc<Trie<u32>>
}

impl EventHandler {
    pub fn from(message_queue: Arc<List<PublishPacket>>, router: Arc<Trie<u32>>) -> Self {
        Self { message_queue, router }
    }
}

impl Messanger for EventHandler {
    fn dequeue_message(&self) -> Option<PublishPacket> {
        self.message_queue.take_first()
    }

    async fn route(&self, topic: &str) -> Option<Vec<u32>> {
        self.router.get(topic).await
    }
}

impl Event for EventHandler {
    fn enqueue_message(&self, msg: PublishPacket) {
        self.message_queue.append(msg)
    }

    async fn subscribe_topic(&self, sub: SubscribePacket, con_id: u32) {
        self.router.insert(&sub.topic, con_id).await
    }
}