use std::{io, sync::Arc};
use dashmap::DashMap;
use crate::protocol::mqtt::PublishPacket;
use super::{linked_list::List, mediator::{Clients, MessageConsumer}};

pub struct Consumer {
    message_queue: Arc<List<PublishPacket>>,
    forward: Arc<DashMap<String, Clients>>
}

impl Consumer {
    pub fn new(
        message_queue: Arc<List<PublishPacket>>, 
        forward: Arc<DashMap<String, Clients>>
    ) -> Self {
        Self { message_queue, forward }
    }
}

impl MessageConsumer for Consumer {
    type T = PublishPacket;
    fn take(&self) -> Option<Self::T> {
        self.message_queue.take_first()
    }

    // TODO: write into disk when message is nod sent
    async fn broadcast(&self, message: Self::T) -> io::Result<()> {
        let clients = self.forward.get_mut(&message.topic);
        let clients = match clients {
            Some(c) => c,
            None => return Ok(()),
        };

        // TODO: format send to subscriber
        for c in clients.iter() {
            let mut c = c.lock().await;
            let mut buffer = message.serialize();
            c.write(&mut buffer).await?;
        }
        Ok(())
    }
}

