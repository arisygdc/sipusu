use bytes::BytesMut;

use super::v5::{publish::PublishPacket, subscribe::SubscribePacket};

pub const PING_RES: [u8; 1] = [0xD0];


pub enum ClientPacketV5 {
    Publish(PublishPacket),
    Subscribe(SubscribePacket),
    PingReq
}

impl ClientPacketV5 {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, String> {
        let ctrl_packet = buffer[0] >> 4;
        let pv = match ctrl_packet {
            0x08 => Self::Subscribe(SubscribePacket::decode(buffer)?),
            0x03 => Self::Publish(PublishPacket::decode(buffer)?),
            0x0C => Self::PingReq,
            _ => return Err(String::from("invalid header"))
        };
        Ok(pv)
    }
}