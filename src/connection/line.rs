// #![allow(unused)]
use std::{io::{self, ErrorKind}, time::Duration};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time};
use crate::protocol::mqtt::ConnectPacket;

pub trait Streamer: 
AsyncReadExt
+ AsyncWriteExt
+ std::marker::Unpin {}


pub struct ConnectedLine<S>
    where S: Streamer + Send + Sync + 'static
{
    pub(super) conn_num: u32,
    pub(super) socket: S,
}

impl<S> ConnectedLine<S>
    where S: Streamer + Send + Sync + 'static 
{
    pub fn new(conn_num: u32, socket: S) -> Self {
        Self { conn_num, socket }
    }

    pub async fn read_timeout(&mut self, buffer: &mut BytesMut, timeout_sec: u8) -> io::Result<()> {
        let timeout_duration = Duration::from_secs(timeout_sec as u64);
    
        match time::timeout(timeout_duration, self.read(buffer)).await {
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

    #[inline(always)]
    pub async fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buffer).await
    }

    #[inline(always)]
    pub async fn write(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.socket.write(buffer).await
    }
}

pub trait MQTTHandshake {
    fn read_ack(&mut self) -> impl std::future::Future<Output = io::Result<ConnectPacket>> + Send;
    fn connack(&mut self, flag: SessionFlag, code: u8) -> impl std::future::Future<Output = io::Result<()>> + Send;
}

impl<S> MQTTHandshake for ConnectedLine<S> 
    where S: Streamer + Send + Sync + 'static 
{
    async fn read_ack(&mut self) -> io::Result<ConnectPacket> {
        let mut buffer = BytesMut::with_capacity(30);
        self.read_timeout(&mut buffer, 1).await?;
        let packet = ConnectPacket::deserialize(&mut buffer)
            .map_err(|e| io::Error::new(
                ErrorKind::InvalidData, 
                e.to_string()
            )
        )?;
        Ok(packet)
    }

    async fn connack(&mut self, flag: SessionFlag, code: u8) -> io::Result<()> {
        let session = match flag {
            SessionFlag::New => 0x0u8,
            SessionFlag::Preset => 0x1u8
        };

        let mut connack = [0x20u8, 0x02u8, session, code];
        self.write(connack.as_mut_slice()).await?;
        Ok(())
    }
}

pub enum SessionFlag {
    // Resume session
    Preset,
    // Create new session
    New
}