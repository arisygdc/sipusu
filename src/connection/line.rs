use std::time::Duration;
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use tokio_rustls::server::TlsStream;
use crate::protocol::v5::connect::ConnectPacket;
use super::{errors::{ConnError, ErrorKind}, SocketReader, SocketWriter};

pub type SecuredStream = TlsStream<TcpStream>;

pub(super) trait MqttConnectRequest: SocketReader {
    async fn read_request<'a>(&'a mut self) -> Result<ConnectPacket, ConnError>;
}

#[derive(Debug)]
pub enum SocketConnection {
    Secure(SecuredStream),
    Plain(TcpStream)
}

impl SocketReader for SocketConnection {
    async fn read(&mut self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        match self {
            Self::Plain(p) => p.read(buffer).await,
            Self::Secure(s) => s.read(buffer).await,
        }
    }
}

impl SocketWriter for SocketConnection {
    async fn write_all(&mut self, buffer: &mut [u8]) -> tokio::io::Result<()> {
        match self {
            Self::Plain(p) => p.write_all(&buffer).await,
            Self::Secure(s) => s.write_all(&buffer).await
        }
    }
}

impl MqttConnectRequest for SocketConnection {
    async fn read_request<'a>(&'a mut self) -> Result<ConnectPacket, ConnError> {
        let mut buffer = {
            let mut buffer = BytesMut::zeroed(256);
            let dur = Duration::from_secs(1);
            let read_len = self.read_timeout(&mut buffer, dur).await.unwrap();

            buffer.split_to(read_len)
        };

        let packet = ConnectPacket::decode(&mut buffer)
            .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(String::from(e)))
        )?;
        Ok(packet)
    }
}

// impl MqttConnectResponse for SocketConnection {
//     async fn connack<'a>(&'a mut self, ack: ConnackPacket) -> Result<(), ConnError> {
//         let mut packet = ack.encode()
//             .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(e)))?;

//         match self.write_all(&mut packet).await {
//             Err(err) => Err(ConnError::new(ErrorKind::ConnectionAborted, Some(err.to_string()))),
//             Ok(empty) => Ok(empty)
//         }
//     }
// }