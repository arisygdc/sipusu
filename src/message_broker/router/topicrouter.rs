use std::sync::Arc;

use super::nonshared::NonShared;
use crate::{ds::trie::Trie, message_broker::client::clobj::ClientID, protocol::v5::{malform::Malformed, subsack::SubAckResult, subscribe::Subscribe}};

use super::SubscriberInstance;

pub trait TopicRouter {
    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed>;
    fn route(&self, topic: &str) -> Option<SubscriberInstance>;
}

// pub type RouterTree = Arc<TRouter<SubscriberInstance>>;

#[derive(Clone)]
pub struct RouterTree {
    inner: Arc<TRouter<SubscriberInstance>>,
}

impl RouterTree {
    pub fn new() -> Self {
        Self{ inner: Arc::new(TRouter::new()) }
    }
}

impl TopicRouter for RouterTree {
    fn route(&self, topic: &str) -> Option<SubscriberInstance> {
        self.inner.route(topic)
    }

    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed> {
        self.inner.subscribe(clid, subs)
    }
}

struct TRouter<S> 
where
    S: Clone + PartialEq + Sync
{
    nshared: Trie<S, NonShared<S>>,
    // shared: Trie<S, Shared<S>>
}

impl TRouter<SubscriberInstance> {
    pub fn new() -> Self {
        Self{
            nshared: Trie::new_single()
        }
    }
}

impl TopicRouter for TRouter<SubscriberInstance> {
    fn subscribe(&self, clid: &ClientID, subs: &[Subscribe]) -> Result<Vec<SubAckResult>, Malformed> {
        let mut res = Vec::with_capacity(subs.len());
        for sub in subs {
            let instance = SubscriberInstance {
                clid: clid.clone(),
                max_qos: sub.max_qos.clone()
            };
            
            let to_shard = shared_route(&sub.topic);
            if let Some(_shared) = to_shard {
                unimplemented!()
            }
            
            self.nshared.store(&sub.topic, instance);
            res.push(Ok(sub.max_qos.clone()));
        }

        Ok(res)
    }
    
    fn route(&self, topic: &str) -> Option<SubscriberInstance> {
        let to_shard = shared_route(topic);
        if let Some(_shared) = to_shard {
            unimplemented!()
        }

        self.nshared.get(topic)
    }
}

fn shared_route(topic: &str) -> Option<&str> {
    if topic.starts_with('$') {
        let split_path_opt = topic.split_once('/');
        let (group, path) = split_path_opt?;
        if "$shared".eq(group) {
            return Some(path);
        }
    }
    None
}