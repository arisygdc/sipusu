use std::sync::Arc;
use crate::{ds::trie::Trie, protocol::v5::{malform::Malformed, subsack::SubAckResult, subscribe::Subscribe, ServiceLevel}};
use super::client::clobj::ClientID;

#[derive(Clone)]
pub struct SubscriberInstance {
    pub clid: ClientID,
    pub max_qos: ServiceLevel
}

impl PartialEq for SubscriberInstance {
    fn eq(&self, other: &Self) -> bool {
        self.clid.eq(&other.clid)
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

pub trait TopicRouter {
    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed>;
    fn route(&self, topic: &str) -> Option<SubscriberInstance>;
}

impl TopicRouter for Arc<Trie<SubscriberInstance>> {
    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed> {
        let mut res = Vec::with_capacity(subs.len());
        for sub in subs {
            let instance = SubscriberInstance {
                clid: clid.clone(),
                max_qos: sub.max_qos.clone()
            };

            self.insert(&sub.topic, instance);
            res.push(Ok(sub.max_qos.clone()));
        }

        Ok(res)
    }
    
    fn route(&self, topic: &str) -> Option<SubscriberInstance> {
        self.get_val(topic)
    }
}