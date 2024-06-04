use std::cell::RefCell;

use crate::connection::line::Streamer;

use super::{pubisher::Publisher, Online};

pub struct Topic<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub(super) literal_name: String,
    pub(super) hash_name: u64,
    publishers: RefCell<Publisher<S>>
}

impl<S> Topic<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn add_publisher(&self, online: Online<S>) {
        let mut publish = self.publishers.borrow_mut();
        publish.push(online);
    }
}
