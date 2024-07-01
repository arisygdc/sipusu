use std::sync::Arc;
use crate::{
    ds::{linked_list::List, trie::Trie}, 
    protocol::v5::{
            subsack::SubAckResult, 
            subscribe::Subscribe, 
            ServiceLevel
        }
};
use super::{client::client::ClientID, ClientReqHandle, Message, Messanger};

#[derive(Clone)]
pub struct EventHandler {
    message_queue: Arc<List<Message>>,
    router: Arc<Trie<ClientID>>
}

impl EventHandler {
    pub fn from(message_queue: Arc<List<Message>>, router: Arc<Trie<ClientID>>) -> Self {
        Self { message_queue, router }
    }
}

impl Messanger for EventHandler {
    fn dequeue_message(&self) -> Option<Message> {
        self.message_queue.take_first()
    }

    async fn route(&self, topic: &str) -> Option<Vec<ClientID>> {
        self.router.get(topic).await
    }
}

// TODO: shared and non shared subscription
impl ClientReqHandle for EventHandler {
    fn enqueue_message(&self, msg: Message) {
        self.message_queue.append(msg)
    }

    async fn subscribe_topics(&self, subs: &[Subscribe], con_id: ClientID) -> Vec<SubAckResult> {
        let mut res = Vec::with_capacity(subs.len());
        for sub in subs {
            self.router.insert(&sub.topic, con_id.clone()).await;
            let qos = ServiceLevel::try_from(sub.max_qos).unwrap();
            res.push(Ok(qos));
        }
        res
    }
}