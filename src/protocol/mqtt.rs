use bytes::{Buf, BufMut, BytesMut};

pub enum MqttClientPacket {
    Publish(PublishPacket),
    Subscribe(SubscribePacket)
}

#[derive(Debug)]
pub struct SubscribePacket {
    pub id: u16,
    pub list: Vec<Subscribe>
}

#[derive(Debug)]
pub struct Subscribe {
    pub topic: String,
    pub qos: u8
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

impl SubscribePacket {
    #[cfg(test)]
    fn deserialize(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        if buffer.get_u8() != 0x82 {
            return Err("invalid header");
        }
        Self::skip_header(buffer)
    }

    fn skip_header(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        let remaining_length = Self::read_remaining_length(buffer)?;
        *buffer = buffer.split_to(remaining_length);

        let packet_identifier = buffer.get_u16();

        // Payload
        let mut subscriptions = Vec::new();

        while buffer.len() != 0 {
            let topic_filter_len = buffer.get_u16() as usize;
            let topic_filter_bytes = buffer.split_to(topic_filter_len);

            let topic = String::from_utf8(topic_filter_bytes.into())
                .map_err(|_| "Invalid UTF-8 in topic filter")?;

            let qos = buffer.get_u8();
            subscriptions.push(Subscribe { topic, qos });
        }
        
        Ok(SubscribePacket {
            id: packet_identifier,
            list: subscriptions,
        })
    }

    fn read_remaining_length(buffer: &mut BytesMut) -> Result<usize, &'static str> {
        let mut multiplier = 1;
        let mut value = 0;
        for _ in 0..2 {
            let encoded_byte = buffer.get_u8();
            value += ((encoded_byte & 127) as usize) * multiplier;
            if (encoded_byte & 128) == 0 {
                break;
            }

            multiplier *= 128;
            if multiplier > 128 * 128 * 128 {
                return Err("Malformed Remaining Length");
            }
        }
        Ok(value)
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

#[cfg(test)]
mod tests {
    use bytes::BytesMut;

    use crate::protocol::mqtt::{Subscribe, SubscribePacket};

    #[test]
    fn test_subscribe_packet_deserialization() {
        struct TestCase {
            raw: BytesMut,
            exp: SubscribePacket
        }
        let test_table = vec![
            TestCase{
                raw: BytesMut::from([
                    0x82, 
                    0x29,   
                    0x00, 0x01, 
                    0x00, 0x12,
                    0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x65, 0x72, 0x61, 0x74, 0x75, 0x72, 0x65, 
                    0x01, 
                    0x00, 0x0F, 
                    0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x2F, 0x68, 0x75, 0x6D, 0x69, 0x64, 0x69, 0x74, 0x79,
                    0x02,
                ].as_slice()),
                exp: SubscribePacket {
                    id: 1,
                    list: vec![
                        Subscribe {
                            topic: "sensor/temperature".to_string(),
                            qos: 1,
                        },
                        Subscribe {
                            topic: "sensor/humidity".to_string(),
                            qos: 2,
                        },
                    ],
                },
            }, TestCase {
                raw: BytesMut::from([
                    0x82, // SUBSCRIBE packet type
                    0x0F, // Remaining length
                    0x00, 0x01, // Packet ID
                    0x00, 0x0A, // Topic name length
                    b't', b'e', b's', b't', b'/', b't', b'o', b'p', b'i', b'c', // Topic name
                    0x00, // QoS    
                ].as_slice()),
                exp: SubscribePacket { 
                    id: 1, 
                    list: vec![Subscribe {
                        topic: String::from("test/topic"),
                        qos: 0
                    }]
                }
            }
        ];

        for test in test_table {
            let mut buf = test.raw;
            let deserialized = SubscribePacket::deserialize(&mut buf).expect("Deserialization failed");
            println!("deserialize {:?}, expected {:?}", deserialized, test.exp);

            assert_eq!(deserialized.id, test.exp.id);
            assert_eq!(deserialized.list.len(), test.exp.list.len());
            for (deserialized_sub, expected_sub) in deserialized.list.iter().zip(test.exp.list.iter()) {
                assert_eq!(deserialized_sub.topic, expected_sub.topic);
                assert_eq!(deserialized_sub.qos, expected_sub.qos);
            }
            
        }
    }
}