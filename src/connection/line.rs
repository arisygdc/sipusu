// #![allow(unused)]
use std::{io::{self, ErrorKind}, net::SocketAddr, time::Duration};
use bytes::BytesMut;
use tokio::{io::AsyncReadExt, net::TcpStream, time};
use crate::core::OnlineState;
use super::handler::SecuredStream;

pub enum Socket {
    Secure(SecuredStream),
    Default(TcpStream)
}

#[allow(dead_code)]
pub struct ConnectedLine
{
    id: u32,
    socket: Socket,
    addr: SocketAddr,
}

impl ConnectedLine {
    pub fn new(id: u32, socket: Socket, addr: SocketAddr) -> Self {
        Self { id, socket, addr }
    }

    pub async fn handshake(&mut self) -> Option<(String, OnlineState)> {
        let state = OnlineState::Publisher;
        Some(("topic".to_owned(), state))
    }

    // pub fn online(mut self, topic: String, state: OnlineState) {
    //     let fut = async move {
    //         loop {
    //             let mut buf = BytesMut::with_capacity(256);
    //             match self.socket.read(&mut buf).await {
    //                 Ok(0) => {
    //                     // Connection was closed
    //                     return ();
    //                 }
    //                 Ok(n) => {
    //                     // Handle received data
    //                     println!("Received: {:?}", &buf[..n]);
    //                 }
    //                 Err(e) => {
    //                     eprintln!("[listen] Error: {}", e);
    //                     return ();
    //                 }
    //             }
    //         }
    //     };
    //     tokio::task::spawn(fut);
    // }
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