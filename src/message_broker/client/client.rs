use std::io;
use tokio::net::TcpStream;
use crate::{
    connection::{
        line::{SecuredStream, SocketConnection}, 
        ConnectionID
    }, 
    helper::time::sys_now
};
use super::{clobj::{ClientID, Limiter, Session, Socket}, storage::{ClientStore, MetaData}, SessionController};


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