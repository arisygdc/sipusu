use super::{LineSettle, Streamer};

pub struct Publisher<S>(Vec<LineSettle<S>>) where S: Streamer + Send + Sync + 'static;
impl<S> Publisher<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn push(&mut self, online: LineSettle<S>) {
        self.0.push(online)
    }
}
