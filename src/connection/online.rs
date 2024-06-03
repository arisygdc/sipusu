// #![allow(unused)]
use std::{io::{self, ErrorKind}, net::SocketAddr, time::Duration};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time};

use super::handler::SecuredStream;

pub trait Streamer: 
AsyncReadExt 
+ AsyncWriteExt
+ std::marker::Unpin {}

#[allow(dead_code)]
pub struct ConnectedLine<S> 
    where S: Streamer + Send + Sync + 'static
{
    id: u32,
    socket: S,
    addr: SocketAddr,
}

impl<S> ConnectedLine<S> 
    where S: Streamer + Send + Sync + 'static
{
    pub fn new(id: u32, socket: S, addr: SocketAddr) -> Self {
        Self { id, socket, addr }
    }

    pub async fn handshake(&mut self) -> Option<OnlineIdentity> {
        let state = OnlineState::Publisher;
        let identity = OnlineIdentity {
            topic: "fwef".to_owned(),
            state,
        };
        Some(identity)
    }

    // pub async fn authenticate<A>(&mut self, actr: Arc<A>) -> io::Result<()>
    //     where A: AuthenticationStore
    // {
    //     // self.socket.write("".as_slice()).await?;
    //     // let buffer = BytesMut::with_capacity(capacity)
    //     // actr.authenticate(auth);
    //     Ok(())
    // }

    pub fn online(mut self, identity: OnlineIdentity) {
        let fut = async move {
            loop {
                let mut buf = BytesMut::with_capacity(256);
                match self.socket.read(&mut buf).await {
                    Ok(0) => {
                        // Connection was closed
                        return ();
                    }
                    Ok(n) => {
                        // Handle received data
                        println!("Received: {:?}", &buf[..n]);
                    }
                    Err(e) => {
                        eprintln!("[listen] Error: {}", e);
                        return ();
                    }
                }
            }
        };
        tokio::task::spawn(fut);
    }
}

async fn read_timeout(stream: &mut SecuredStream, buffer: &mut BytesMut, timeout_sec: u8) -> io::Result<()>{
    let timeout_duration = Duration::from_secs(timeout_sec as u64);

    match time::timeout(timeout_duration, stream.read(buffer)).await {
        Ok(read_result) => {
            let read_leng = read_result?;
            if read_leng == 0 {
                println!("Client closed connection");
                return Err(io::Error::new(ErrorKind::ConnectionAborted, "Client closed connection"));
            }
            return Ok(());
        }
        Err(_) => {
            return Err(io::Error::new(ErrorKind::TimedOut, "Reading stream timeout"));
        }
    }
}

pub struct OnlineIdentity {
    topic: String,
    state: OnlineState
}

enum OnlineState {
    Publisher,
    Subscriber
}