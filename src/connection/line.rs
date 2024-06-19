use std::time::Duration;
use bytes::BytesMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time};
use tokio_rustls::server::TlsStream;
use crate::{message_broker, protocol::v5::{connack, connect::ConnectPacket}};
use super::errors::{ConnError, ErrorKind};

pub type SecuredStream = TlsStream<TcpStream>;
pub(super) trait MqttHandshake {
    async fn read_ack<'a>(&'a mut self) -> Result<ConnectPacket, ConnError>;
    // async fn validate_request(&self, packet: ConnectPacket) -> Result<(), String>;
    async fn connack<'a>(&'a mut self, flag: SessionFlag, code: u8) -> Result<(), ConnError>;
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

    async fn connack<'a>(&'a mut self, flag: SessionFlag, code: u8) -> Result<(), ConnError> {
        let session = match flag {
            SessionFlag::New => 0x0u8,
            SessionFlag::Preset => 0x1u8
        };

        let mut connack = [0x20u8, 0x02u8, session, code];
        match self.write_all(connack.as_mut_slice()).await {
            Err(err) => Err(ConnError::new(ErrorKind::ConnectionAborted, Some(err.to_string()))),
            Ok(empty) => Ok(empty)
        }
    }
}

pub trait ConnHandshake: AsyncReadExt + AsyncWriteExt + Unpin + Send {
    async fn read_ack<'a>(&'a mut self) -> Result<ConnectPacket, ConnError> {
        let mut buffer = {
            let mut buffer = BytesMut::zeroed(256);
            let read_len = read_timeout(self, &mut buffer, 1).await?;
            buffer.split_to(read_len)
        };
        let packet = ConnectPacket::decode(&mut buffer)
            .map_err(|e| ConnError::new(ErrorKind::InvalidData, Some(String::from(e)))
        )?;
        Ok(packet)
    }

    async fn validate_request(&self, packet: ConnectPacket) -> Result<(), String> {
        let prop = match packet.properties {
            None => None,
            Some(value) => {Some(
                connack::Properties {
                    session_expiry_interval: value.session_expiry_interval,
                    receive_maximum: value.receive_maximum,
                    maximum_qos: Some(message_broker::MAX_QOS),
                    retain_available: None,
                    maximum_packet_size: value.maximum_packet_size,
                    assigned_client_identifier: Some(packet.client_id),
                    topic_alias_maximum: value.topic_alias_maximum,
                    // TODO
                    reason_string: Some(String::from("value")),
                    user_properties: value.user_properties,
                    wildcard_subscription_available: Some(message_broker::WILDCARD_SUPPORT as u8),
                    subscription_identifier_available: Some(message_broker::SUBS_ID_SUPPORT as u8),
                    shared_subscription_available: Some(message_broker::SHARED_SUBS_SUPPORT as u8),
                    server_keep_alive: Some(0),
                    response_information: None,
                    server_reference: None,
                    authentication_data: None,
                    authentication_method: None
                }
            )}
        };
        Ok(())
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

pub trait IntoConnection {
    fn into_conn(self) -> SocketConnection;
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

