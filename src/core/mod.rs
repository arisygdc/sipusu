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

    async fn write_incoming(&mut self, buffer: &mut Vec<Online>) {
        for client in buffer {
            println!("as {}", client.topic)
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
        }
    }

    async fn read_incomings(&mut self, buffer: &mut Vec<Online>) {
        let len = buffer.len();
        // self.incoming.recv();
        self.incoming.recv_many(buffer, len).await;
    }
}

pub struct EventHandler;

impl EventHandler {
    pub async fn read_signal<S>(&self, mut media: TopicMediator<S>) 
        where S: Streamer + Send + Sync + 'static
    {
        tokio::spawn(async move {
            println!("----- Spanw Event Listener -----");
            // let mut buffer = Vec::with_capacity(4);
            loop {
                select! {
                    tt = media.incoming.recv() => {
                        match tt {
                            None => println!("no data through channel"),
                            Some(v) => println!("rcvd: {:?}", v.topic),
                        };
                    }
                }
            }
        });
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

// main function
// let mediator = TopicMediator::new();
// // mediator got error
// // type annotations needed
// // cannot satisfy `_: Streamer`
// // the following types implement trait `Streamer`:
// //  tokio_rustls::server::TlsStream<tokio::net::TcpStream>
// //  tokio::net::TcpStreamrustcClick for full compiler diagnostic

// pub struct TopicMediator<S>
//     where S: Streamer + Send + Sync + 'static
// {
//     topics: Arc<RwLock<Vec<Topic<S>>>>,
//     incoming: mpsc::Receiver<Online>
// }

// impl<S> TopicMediator<S> 
//     where S: Streamer + Send + Sync + 'static
// {
//     pub fn new() -> (mpsc::Sender<Online>, Self) {
//         let (tx, rx) = mpsc::channel(4);
//         let topics = RwLock::new(Vec::new());
//         let topics = Arc::new(topics);
//         (tx, Self{ incoming: rx, topics})
//     }
// }

// pub trait Streamer: 
// AsyncReadExt
// + AsyncWriteExt
// + std::marker::Unpin {}

// impl Streamer for SecuredStream {}
// impl Streamer for TcpStream {}


// bantu saya untuk solve problem variable mediator