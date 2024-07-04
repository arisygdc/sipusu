use std::io;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
use crate::{
    connection::{
        handshake::MqttConnectedResponse, 
        line::{SecuredStream, SocketConnection}, 
        ConnectionID, SocketReader, 
        SocketWriter
    }, 
    helper::time::sys_now, 
    protocol::v5::connack::ConnackPacket
};
use super::{clobj::{ClientID, Socket, SocketInner}, storage::{ClientStore, MetaData}, SessionController};


#[allow(dead_code)]
pub struct Client {
    pub(super) conid: ConnectionID,
    pub clid: ClientID,
    pub socket: Socket<SecuredStream, TcpStream>,
    protocol_level: u8,
    pub session: Session,
    pub limit: Limiter,
    pub storage: ClientStore
}

pub struct Session {
    ttl: u64,
    keep_alive: u16,
    expr_interval: u32,
}

#[derive(Default)]
pub struct Limiter {
    receive_maximum: Option<u16>,
    maximum_packet_size: Option<u16>,
    topic_alias_maximum: Option<u16>,
}

impl Limiter {
    pub fn new(
        receive_maximum: Option<u16>, 
        maximum_packet_size: Option<u16>, 
        topic_alias_maximum: Option<u16>
    ) -> Self {
        Self { maximum_packet_size, receive_maximum, topic_alias_maximum }
    }
}

pub struct UpdateClient {
    pub conid: Option<ConnectionID>,
    pub socket: Option<SocketConnection>,
    pub protocol_level: Option<u8>,
    pub keep_alive: Option<u16>,
}

// keepalive min value: 60
impl Client {
    pub async fn new(
        conid: ConnectionID,
        socket: SocketConnection,
        clid: ClientID,
        keep_alive: u16,
        expr_interval: u32,
        protocol_level: u8,
        limit: Limiter
    ) -> Self {
        let socket = Socket::new(socket);
        let ttl = sys_now() + keep_alive as u64;
        let keep_alive = keep_alive.max(60);
        let mdata = MetaData {
            expr_interval,
            keep_alive_interval: keep_alive,
            protocol_level,
            maximum_packet_size: limit.maximum_packet_size.unwrap_or_default(),
            receive_maximum: limit.receive_maximum.unwrap_or_default(),
            topic_alias_maximum: limit.topic_alias_maximum.unwrap_or_default(),
            user_properties: Vec::new()
        };

        let session =  Session {
            expr_interval,
            keep_alive,
            ttl
        };

        let storage = ClientStore::new(&clid, &mdata, None).await.unwrap();
        Self {
            conid,
            socket,
            clid,
            session,
            limit,
            protocol_level,
            storage
        }
    }

    // TODO: Error type
    pub fn restore_connection(&mut self, bucket: &mut UpdateClient) -> io::Result<()> {
        let now = sys_now();
        if !self.session.is_alive(now) && self.session.is_expired(now) {
           return Err(io::Error::new(io::ErrorKind::Other, String::from("connection is expired"))); 
        }

        match bucket.socket.take() {
            Some(s) => self.socket = Socket::new(s),
            None => panic!("empty connection")
        };

        if let Some(keep_alive) = bucket.keep_alive.take() {
            self.session.keep_alive = keep_alive;
        }

        if let Some(connid) = bucket.conid.take() {
            self.conid = connid;
        }
        if let Some(pr_lvl) = bucket.protocol_level.take() {
            self.protocol_level = pr_lvl;
        }

        Ok(())
    }
}

impl SessionController for Session {
    fn is_alive(&self, t: u64) -> bool {
        self.ttl >= t
    }

    fn is_expired(&self, t: u64) -> bool {
        (self.expr_interval as u64 + self.ttl) <= t
    }

    fn keep_alive(&mut self, t: u64) -> Result<u64, String> {
        if self.ttl <= t {
            return Err(String::from("already expired"));
        }
        self.ttl = t + self.keep_alive as u64;
        Ok(self.ttl)
    }

    fn kill(&mut self) {
        self.ttl = 0;
    }
}

impl SocketWriter for Client {
    async fn write_all(&mut self, buffer: &[u8]) -> tokio::io::Result<()> {
        match &mut self.socket.w {
            SocketInner::Plain(p) => p.write_all(buffer).await,
            SocketInner::Secured(s) => s.write_all(buffer).await
        }
    }
}

impl SocketReader for Client {
    async fn read(&mut self, buffer: &mut [u8]) -> tokio::io::Result<usize> {
        match &mut self.socket.r {
            SocketInner::Plain(p) => p.read(buffer).await,
            SocketInner::Secured(s) => s.read(buffer).await
        }
    }
}

impl MqttConnectedResponse for Client {
    async fn connack<'a>(&'a mut self, ack: &'a ConnackPacket) -> io::Result<()> {
        let mut packet = ack.encode().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let res = self.write_all(&mut packet).await;
        res
    }
}