use tokio::sync::RwLock;

use super::{pubisher::Publisher, LineSettle, Streamer};

pub struct Topic<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub(super) literal_name: String,
    pub(super) hash_name: u64,
    publishers: RwLock<Publisher<S>>
}

impl<S> Topic<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub async fn add_publisher(&self, online: LineSettle<S>) {
        let mut publish = self.publishers.write().await;
        publish.push(online);
    }
}
