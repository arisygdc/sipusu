use std::{io::{self, ErrorKind}, time::Duration};
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, time};
use crate::protocol::mqtt::ConnectPacket;

pub trait ConnHandshake: AsyncReadExt + AsyncWriteExt + Unpin + Send {
    async fn read_ack<'a>(&'a mut self) -> io::Result<ConnectPacket> {
        let mut buffer = BytesMut::zeroed(128);
        read_timeout(self, &mut buffer, 1).await?;
        let packet = ConnectPacket::deserialize(&mut buffer)
            .map_err(|e| io::Error::new(
                ErrorKind::InvalidData, 
                e.to_string()
            )
        )?;
        Ok(packet)
    }

    async fn connack<'a>(&'a mut self, flag: SessionFlag, code: u8) -> io::Result<()> {
        let session = match flag {
            SessionFlag::New => 0x0u8,
            SessionFlag::Preset => 0x1u8
        };

        let mut connack = [0x20u8, 0x02u8, session, code];
        self.write(connack.as_mut_slice()).await?;
        Ok(())
    }
}

async fn read_timeout<'a, Read>(reader: &'a mut Read, buffer: &'a mut BytesMut, timeout_sec: u8) -> io::Result<()> 
    where Read: AsyncReadExt + Unpin + ?Sized
{
    let timeout_duration = Duration::from_secs(timeout_sec as u64);

    match time::timeout(timeout_duration, reader.read(buffer)).await {
        Ok(Ok(read_len)) => {
            if read_len == 0 {
                println!("Client closed connection");
                return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "Client closed connection"));
            }
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "Reading stream timeout")),
    }
    
}

pub enum SessionFlag {
    // Resume session
    Preset,
    // Create new session
    New
}