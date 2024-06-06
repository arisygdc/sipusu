use bytes::{Buf, BufMut, BytesMut};

// #[derive(Debug, Serialize, Deserialize)]
pub enum MqttPacket {
    Connect(ConnectPacket),
    Publish(PublishPacket),
}

#[derive(Debug)]
pub struct ConnectPacket {
    pub protocol_name: String,
    pub protocol_level: u8,
    pub client_id: String,
    pub keep_alive: u16,
}

#[derive(Default)]
pub struct PublishPacket {
    pub topic: String,
    pub payload: Vec<u8>,
}

impl MqttPacket {
    pub fn serialize(&self) -> BytesMut {
        match self {
            MqttPacket::Connect(packet) => packet.serialize(),
            MqttPacket::Publish(packet) => packet.serialize(),
        }
    }

    pub fn deserialize(bytes: &mut BytesMut) -> Result<Self, &'static str> {
        if bytes.is_empty() {
            return Err("No data to deserialize");
        }

        let packet_type = bytes[0] >> 4;

        match packet_type {
            1 => Ok(MqttPacket::Connect(ConnectPacket::deserialize(bytes)?)),
            3 => Ok(MqttPacket::Publish(PublishPacket::deserialize(bytes)?)),
            _ => Err("Unknown packet type"),
        }
    }
}

impl ConnectPacket {
    pub fn serialize(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u8(0x10); // CONNECT packet type

        // Remaining length (for simplicity, we assume it fits in one byte)
        let remaining_length = 10 + self.protocol_name.len() + self.client_id.len();
        buf.put_u8(remaining_length as u8);

        // Protocol name
        buf.put_u8((self.protocol_name.len() >> 8) as u8);
        buf.put_u8(self.protocol_name.len() as u8);
        buf.put_slice(self.protocol_name.as_bytes());

        // Protocol level
        buf.put_u8(self.protocol_level);

        // Connect flags (we use default flags here)
        buf.put_u8(0x02);

        // Keep alive
        buf.put_u8((self.keep_alive >> 8) as u8);
        buf.put_u8(self.keep_alive as u8);

        // Client ID
        buf.put_u8((self.client_id.len() >> 8) as u8);
        buf.put_u8(self.client_id.len() as u8);
        buf.put_slice(self.client_id.as_bytes());

        buf
    }

    pub fn deserialize(bytes: &mut BytesMut) -> Result<Self, &'static str> {
        if bytes[0] != 0x10 {
            return Err("Invalid CONNECT packet type");
        }

        // Skip the packet type byte
        bytes.advance(1);

        // Remaining length
        let _remaining_length = bytes.get_u8() as usize;

        // Protocol name
        let protocol_name_len = bytes.get_u16() as usize;
        let protocol_name = String::from_utf8(bytes.split_to(protocol_name_len).to_vec()).map_err(|_| "Invalid protocol name")?;

        // Protocol level
        let protocol_level = bytes.get_u8();

        // Connect flags (skip)
        bytes.advance(1);

        // Keep alive
        let keep_alive = bytes.get_u16();

        // Client ID
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

    pub fn deserialize(bytes: &mut BytesMut) -> Result<Self, &'static str> {
        if bytes[0] != 0x30 {
            return Err("Invalid PUBLISH packet type");
        }

        // Skip the packet type byte
        bytes.advance(1);

        // Remaining length
        let _remaining_length = bytes.get_u8() as usize;

        // Topic
        let topic_len = bytes.get_u16() as usize;
        let topic = String::from_utf8(bytes.split_to(topic_len).to_vec()).map_err(|_| "Invalid topic")?;

        // Payload
        let payload = bytes.split().to_vec();

        Ok(Self {
            topic,
            payload,
        })
    }
}