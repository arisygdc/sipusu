use bytes::{Buf, BufMut, BytesMut};

use super::subscribe::SubscribePacket;

pub enum MqttClientPacket {
    Publish(PublishPacket),
    Subscribe(SubscribePacket)
}

#[derive(Debug)]
pub struct ConnectPacket {
    pub protocol_name: String,
    pub protocol_level: u8,
    pub client_id: String,
    pub keep_alive: u16,
}

#[derive(Default, Debug)]
pub struct PublishPacket {
    pub topic: String,
    pub payload: Vec<u8>,
}

impl ConnectPacket {
    pub fn deserialize(bytes: &mut BytesMut) -> Result<Self, &'static str> {
        if bytes[0] != 0x10 {
            return Err("Invalid CONNECT packet type");
        }

        bytes.advance(1);

        let _remaining_length = bytes.get_u8() as usize;

        let protocol_name_len = bytes.get_u16() as usize;
        let protocol_name = String::from_utf8(bytes.split_to(protocol_name_len).to_vec()).map_err(|_| "Invalid protocol name")?;

        let protocol_level = bytes.get_u8();

        // Connect flags (skip)
        bytes.advance(1);

        let keep_alive = bytes.get_u16();

        let client_id_len = bytes.get_u16() as usize;
        let client_id = String::from_utf8(bytes.split_to(client_id_len).to_vec()).map_err(|_| "Invalid client ID")?;

        Ok(Self {
            protocol_name,
            protocol_level,
            client_id,
            keep_alive,
        })
    }
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