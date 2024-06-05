use super::{topic::Topic, LineSettle, Streamer};

pub struct Event<S>(Vec<Publisher<S>>) where S: Streamer + Send + Sync + 'static;
impl<S> Event<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub(super) fn new() -> Self {
        Self(vec![])
    }

    pub fn insert(&mut self, online: Publisher<S>) {
        self.0.push(online)
    }
}

pub struct Publisher<S>
    where S: Streamer + Send + Sync + 'static
{
    topic: Topic,
    line: LineSettle<S>
}

impl<S> Publisher<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn new(topic: Topic, line: LineSettle<S>) -> Self {
        Self { topic, line }
    }
}