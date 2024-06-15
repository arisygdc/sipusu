use std::time::Duration;
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time};
use crate::protocol::mqtt::ConnectPacket;

use super::errors::{ConnError, ErrorKind};

pub trait ConnHandshake: AsyncReadExt + AsyncWriteExt + Unpin + Send {
    async fn read_ack<'a>(&'a mut self) -> Result<ConnectPacket, ConnError> {
        let mut buffer = {
            let mut buffer = BytesMut::zeroed(256);
            let read_len = read_timeout(self, &mut buffer, 1).await?;
            buffer.split_to(read_len)
        };
        
        let packet = ConnectPacket::deserialize(&mut buffer)
            .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(String::from(e)))
        )?;
        Ok(packet)
    }

    async fn connack<'a>(&'a mut self, flag: SessionFlag, code: u8) -> Result<usize, ConnError> {
        let session = match flag {
            SessionFlag::New => 0x0u8,
            SessionFlag::Preset => 0x1u8
        };

        let mut connack = [0x20u8, 0x02u8, session, code];
        match self.write(connack.as_mut_slice()).await {
            Err(err) => Err(ConnError::new(ErrorKind::ConnectionAborted, Some(err.to_string()))),
            Ok(len) => Ok(len)
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

pub enum SessionFlag {
    // Resume session
    Preset,
    // Create new session
    New
}