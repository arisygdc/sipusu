use std::{collections::HashMap, sync::Arc};
use super::subscriber::Subscriber;
use tokio::sync::RwLock;

pub struct Publisher<S> where S: Subscriber {
    events: HashMap<String, Arc<RwLock<S>>>
}