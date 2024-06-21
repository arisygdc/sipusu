use tokio::io;
use crate::protocol::v5::{connack::ConnackPacket, connect::ConnectPacket};
use super::{errors::ConnError, SocketReader, SocketWriter};

pub(super) trait MqttConnectRequest: SocketReader {
    async fn read_request<'a>(&'a mut self) -> Result<ConnectPacket, ConnError>;
}

pub trait MqttConnectedResponse: SocketWriter {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()>;
}