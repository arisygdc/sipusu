use std::{collections::HashMap, sync::Arc};
use crate::connection::line::Streamer;

use super::{subscriber::Subscriber, Online};
use tokio::sync::RwLock;

pub struct Publisher<S>(Vec<Online<S>>) where S: Streamer + Send + Sync + 'static;
impl<S> Publisher<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn push(&mut self, online: Online<S>) {
        self.0.push(online)
    }
}