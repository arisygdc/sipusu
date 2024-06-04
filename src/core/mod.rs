#![allow(dead_code)]
use std::{mem::take, sync::Arc};
use pubisher::Publisher;
use tokio::{select, sync::{mpsc, RwLock}};
use topic::Topic;
use crate::connection::line::{ConnectedLine, Streamer};

// PUBLISH SUBSCRIBE CORE
mod pubisher;
mod subscriber;
mod topic;

pub enum OnlineState {
    Publisher,
    Subscriber
}

pub struct Online<S> 
    where S: Streamer + Send + Sync + 'static 
{
    topic: String,
    state: OnlineState,
    line: ConnectedLine<S>,
}

impl<S: Streamer + Send + Sync + 'static > Online<S> {
    #[inline]
    pub fn topic_eq(&self, other: &str) -> bool {
        self.topic.eq(other)
    }

    pub fn new(cline: ConnectedLine<S>, topic: String, state: OnlineState) -> Self {
        Self { topic, state, line: cline }
    }
}

pub struct TopicMediator<S> 
    where S: Streamer + Send + Sync + 'static
{
    topics: Arc<RwLock<Vec<Topic<S>>>>,
    incoming: mpsc::Receiver<Online<S>>
}

impl<S> TopicMediator<S> 
    where S: Streamer + Send + Sync + 'static
{
    async fn write_incoming(&mut self, buffer: &mut Vec<Online<S>>) {
        for client in buffer {
            let reader = self.topics.read().await;
            for topic in reader.iter() {
                if client.topic_eq(topic.literal_name.as_str()) {
                    // client.
                    // topic.add_publisher();
                }
            }

            if let OnlineState::Subscriber = client.state {
                continue;
            }

        }
    }

    async fn read_incomings(&mut self, buffer: &mut Vec<Online<S>>) {
        let len = buffer.len();
        self.incoming.recv_many(buffer, len).await;
    }
}

pub struct EventHandler;

impl EventHandler {
    async fn read_signal<S>(mut media: TopicMediator<S>) 
        where S: Streamer + Send + Sync + 'static
    {
        let mut buffer = Vec::with_capacity(4);
        loop {
            select! {
                _ = media.read_incomings(&mut buffer) => {
                    media.write_incoming(&mut buffer).await
                }
            }
        }
    }
}


// pub struct IncomingCLient<S: Streamer + Send + Sync + 'static>(Arc<Mutex<Vec<Online<S>>>>);

// impl<S: Streamer + Send + Sync + 'static> IncomingCLient<S> {
//     pub async fn push(&self, online: Online<S>) {
//         let mut hold_conn = self.0.lock().await;
        
//         hold_conn.push(online)
//     }

//     async fn consume(&self) {
//         let mut hold_conn = self.0.lock().await;
//         let mut consumeer = Vec::with_capacity(hold_conn.len());
//         hold_conn.swap_with_slice(consumeer.as_mut());
//         hold_conn.shrink_to(4);
//     }
// }