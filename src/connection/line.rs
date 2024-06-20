use std::time::Duration;
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time};
use tokio_rustls::server::TlsStream;
use crate::protocol::v5::{connack::ConnackPacket, connect::ConnectPacket};  
use super::errors::{ConnError, ErrorKind};

pub type SecuredStream = TlsStream<TcpStream>;

pub(super) trait MqttHandshake {
    async fn read_ack<'a>(&'a mut self) -> Result<ConnectPacket, ConnError>;
    // async fn validate_request(&self, packet: ConnectPacket) -> Result<(), String>;
    async fn connack<'a>(&'a mut self, ack: ConnackPacket) -> Result<(), ConnError>;
}

#[derive(Debug)]
pub enum SocketConnection {
    Secure(SecuredStream),
    Plain(TcpStream)
}

impl SocketConnection {
    async fn read<'a>(&mut self, buffer: &'a mut [u8]) -> tokio::io::Result<usize>
    {
        match self {
            Self::Plain(p) => p.read(buffer).await,
            Self::Secure(s) => s.read(buffer).await,
        }
    }

    async fn write_all<'a>(&mut self, buffer: &'a mut [u8]) -> tokio::io::Result<()>
    {
        match self {
            Self::Plain(p) => p.write_all(&buffer).await,
            Self::Secure(s) => s.write_all(&buffer).await
        }
    }
}

impl MqttHandshake for SocketConnection {
    async fn read_ack<'a>(&'a mut self) -> Result<ConnectPacket, ConnError> {
        let mut buffer = {
            let mut buffer = BytesMut::zeroed(256);
            let read_len = match self {
                Self::Plain(p) => read_timeout(p, &mut buffer, 1).await?,
                Self::Secure(s) => read_timeout(s, &mut buffer, 1).await?
            };

            buffer.split_to(read_len)
        };

        let packet = ConnectPacket::decode(&mut buffer)
            .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(String::from(e)))
        )?;
        Ok(packet)
    }

    async fn connack<'a>(&'a mut self, ack: ConnackPacket) -> Result<(), ConnError> {
        let mut packet = ack.encode()
            .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(e)))?;

        match self.write_all(&mut packet).await {
            Err(err) => Err(ConnError::new(ErrorKind::ConnectionAborted, Some(err.to_string()))),
            Ok(empty) => Ok(empty)
        }
    }
}

async fn read_timeout<'a, Read>(reader: &'a mut Read, buffer: &'a mut BytesMut, timeout_sec: u8) -> Result<usize, ConnError> 
    where Read: AsyncReadExt + Unpin + ?Sized
{
    let timeout_duration = Duration::from_secs(timeout_sec as u64);

    let read_result = match time::timeout(timeout_duration, reader.read(buffer)).await {
        Ok(r) => r,
        Err(_) => return Err(ConnError::new(ErrorKind::TimedOut, Some(String::from("operation timeout")))),
    };

    match read_result {
        Ok(0) => Err(ConnError::new(ErrorKind::ConnectionAborted, None)),
        Ok(len) => Ok(len),
        Err(err) => Err(ConnError::new(
            ErrorKind::ConnectionAborted, 
            Some(String::from(err.to_string()))
        ))
    }
}