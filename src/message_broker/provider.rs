use std::sync::Arc;
use crate::{ds::{linked_list::List, trie::Trie}, protocol::{mqtt::PublishPacket, subscribe::{SubAckResult, SubWarranty, Subscribe}}};

use super::{client::client::ClientID, Event, Messanger};

#[derive(Clone)]
pub struct EventHandler {
    message_queue: Arc<List<PublishPacket>>,
    router: Arc<Trie<ClientID>>
}

impl EventHandler {
    pub fn from(message_queue: Arc<List<PublishPacket>>, router: Arc<Trie<ClientID>>) -> Self {
        Self { message_queue, router }
    }
}

impl Messanger for EventHandler {
    fn dequeue_message(&self) -> Option<PublishPacket> {
        self.message_queue.take_first()
    }

    async fn route(&self, topic: &str) -> Option<Vec<ClientID>> {
        self.router.get(topic).await
    }
}

impl Event for EventHandler {
    fn enqueue_message(&self, msg: PublishPacket) {
        self.message_queue.append(msg)
    }

    async fn subscribe_topics(&self, subs: &[Subscribe], con_id: ClientID) -> Vec<SubAckResult> {
        let mut res: Vec<SubAckResult> = Vec::with_capacity(subs.len());
        for sub in subs {
            self.router.insert(&sub.topic, con_id.clone()).await;
            res.push(SubWarranty::try_from(sub.qos))
        }

        res
    }
}