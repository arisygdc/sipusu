#![allow(dead_code)]
use std::{os::unix::net::SocketAddr, sync::Arc};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, select, sync::{mpsc, RwLock}};
use topic::Topic;
use crate::connection::{handler::SecuredStream, line::ConnectedLine};

pub trait Streamer: 
AsyncReadExt
+ AsyncWriteExt
+ std::marker::Unpin {}

impl Streamer for SecuredStream {}
impl Streamer for TcpStream {}

// PUBLISH SUBSCRIBE CORE
mod pubisher;
mod subscriber;
mod topic;

pub enum OnlineState {
    Publisher,
    Subscriber
}

pub struct Online {
    topic: String,
    state: OnlineState,
    line: ConnectedLine,
}

impl Online {
    #[inline]
    pub fn topic_eq(&self, other: &str) -> bool {
        self.topic.eq(other)
    }

    pub fn new(cline: ConnectedLine, topic: String, state: OnlineState) -> Self {
        Self { topic, state, line: cline }
    }
}

pub struct LineSettle<S> 
    where S: Streamer + Send + Sync + 'static
{
    id: u32,
    socket: S,
    addr: SocketAddr,
}

pub struct TopicMediator<S>
    where S: Streamer + Send + Sync + 'static
{
    topics: Arc<RwLock<Vec<Topic<S>>>>,
    incoming: mpsc::Receiver<Online>
}

impl<S> TopicMediator<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn new() -> (mpsc::Sender<Online>, Self) {
        let (tx, rx) = mpsc::channel(4);
        let topics = RwLock::new(Vec::new());
        let topics = Arc::new(topics);
        (tx, Self{ incoming: rx, topics})
    }

    // async fn write_incoming(&mut self, buffer: &mut Vec<Online>) {
    //     for client in buffer {
    //         println!("as {}", client.topic)
            // let reader = self.topics.read().await;
            // for topic in reader.iter() {
            //     if client.topic_eq(topic.literal_name.as_str()) {
            //         // client.
            //         // topic.add_publisher();
            //     }
            // }

            // if let OnlineState::Subscriber = client.state {
            //     continue;
            // }
    //     }
    // }

    // async fn read_incomings(&mut self, buffer: &mut Vec<Online>) -> usize {
    //     let len = buffer.len();
    //     self.incoming.recv_many(buffer, len).await
    // }
}

pub struct EventHandler;

impl EventHandler {
    pub async fn listen<S>(&self, mut media: TopicMediator<S>) 
        where S: Streamer + Send + Sync + 'static
    {
        tokio::spawn(async move {
            println!("----- Spanw Event Listener -----");
            // let mut buffer = Vec::with_capacity(4);
            'evlis: loop {
                let tt = media.incoming.recv().await;
                match tt {
                    None => break 'evlis,
                    Some(v) => println!("rcvd: {:?}", v.topic),
                };
            }
        });
    }
}