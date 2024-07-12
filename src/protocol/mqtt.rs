use bytes::BytesMut;

use super::v5::{malform::Malformed, publish::PublishPacket, subscribe::SubscribePacket};

pub const PING_RES: [u8; 1] = [0x0D];


pub enum ClientPacketV5 {
    Publish(PublishPacket),
    Subscribe(SubscribePacket),
    PingReq
}

impl ClientPacketV5 {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, Malformed> {
        let ctrl_packet = buffer[0] >> 4;
        let pv = match ctrl_packet {
            0x08 => Self::Subscribe(SubscribePacket::decode(buffer)?),
            0x03 => Self::Publish(PublishPacket::decode(buffer).map_err(|_| Malformed::MalformedPacket)?),
            0x0C => Self::PingReq,
            _ => return Err(Malformed::ProtocolError)
        };
        Ok(pv)
    }
}