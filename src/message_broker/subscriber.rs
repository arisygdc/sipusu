use super::{LineSettle, Streamer};

pub struct Subscriber<S>(Vec<LineSettle<S>>) where S: Streamer + Send + Sync + 'static;
impl<S> Subscriber<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub(super) fn new() -> Self {
        Self(vec![])
    }
    pub fn push(&mut self, online: LineSettle<S>) {
        self.0.push(online)
    }
}
