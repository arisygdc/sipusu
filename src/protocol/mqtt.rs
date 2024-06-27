use bytes::{Buf, BufMut, BytesMut};

use super::{subscribe::SubscribePacket, v5::{publish::PublishPacket as PubV5, subscribe::SubscribePacket as SubV5}};

pub enum V5ClientPacket {
    Publish(PubV5),
    Subscribe(SubV5)
}

impl V5ClientPacket {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, String> {
        let ctrl_packet = buffer[0] >> 4;
        let pv = match ctrl_packet {
            0x08 => Self::Subscribe(SubV5::decode(buffer)?),
            0x03 => Self::Publish(PubV5::decode(buffer)?),
            _ => return Err(String::from("invalid header"))
        };
        Ok(pv)
    }
}

pub enum MqttClientPacket {
    Publish(PublishPacket),
    Subscribe(SubscribePacket)
}

#[derive(Default, Debug)]
pub struct PublishPacket {
    pub topic: String,
    pub payload: Vec<u8>,
}

impl MqttClientPacket {
    pub fn deserialize(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        let packet = match buffer.get_u8() {
            0x82 => Self::Subscribe(SubscribePacket::skip_header(buffer)?),
            0x30 => Self::Publish(PublishPacket::skip_header(buffer)?),
            _ => return Err("invalid header")
        };

        Ok(packet)
    }
}


impl PublishPacket {
    pub fn serialize(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u8(0x30); // PUBLISH packet type

        // Remaining length
        let remaining_length = 2 + self.topic.len() + self.payload.len();
        buf.put_u8(remaining_length as u8);

        // Topic
        buf.put_u8((self.topic.len() >> 8) as u8);
        buf.put_u8(self.topic.len() as u8);
        buf.put_slice(self.topic.as_bytes());

        // Payload
        buf.put_slice(&self.payload);

        buf
    }

    fn skip_header(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        let _remaining_length = buffer.get_u8() as usize;

        // Topic
        let topic_len = buffer.get_u16() as usize;
        let topic = String::from_utf8(buffer.split_to(topic_len).to_vec()).map_err(|_| "Invalid topic")?;

        // Payload
        let mut payload = buffer.split().to_vec();
        let mut payload_leng = payload.len();
        for ( i, p ) in payload.iter().enumerate() {
            if 0.eq(p) { 
                payload_leng = i;
                break;
            }
        }

        payload.drain(payload_leng..);
        Ok(Self {
            topic,
            payload,
        })  
    }
}