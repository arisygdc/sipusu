#![allow(dead_code)]
use bytes::{Buf, BytesMut};

use super::RemainingLength;

#[derive(Debug)]
pub struct SubscribePacket {
    pub id: u16,
    pub list: Vec<Subscribe>,
}

#[derive(Debug)]
pub struct Subscribe {
    pub topic: String,
    pub max_qos: u8 
}

impl SubscribePacket {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        let header = buffer.get_u8();
        if  (header >> 0x04) != 0x08 {
            return Err("invalid header");
        }
        Self::skip_header(buffer)
    }

    pub(super) fn skip_header(buffer: &mut BytesMut) -> Result<Self, &'static str> {
        let remaining_length = RemainingLength::decode(buffer)?;
        *buffer = buffer.split_to(remaining_length as usize);

        let packet_identifier = buffer.get_u16();
        let _prop_leng = buffer.get_u8();

        // Payload
        let mut subscriptions = Vec::new();

        while buffer.len() != 0 {
            let topic_filter_len = buffer.get_u16() as usize;
            let topic_filter_bytes = buffer.split_to(topic_filter_len);

            let topic = String::from_utf8(topic_filter_bytes.into())
                .map_err(|_| "Invalid UTF-8 in topic filter")?;

            let max_qos = buffer.get_u8() & 0x3;
            subscriptions.push(Subscribe { topic, max_qos });
        }
        
        Ok(SubscribePacket {
            id: packet_identifier,
            list: subscriptions,
        })
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;

    use crate::protocol::v5::subscribe::{Subscribe, SubscribePacket};

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
                    0x2A,   
                    0x00, 0x01,
                    0x00,
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
                            max_qos: 1,
                        },
                        Subscribe {
                            topic: "sensor/humidity".to_string(),
                            max_qos: 2,
                        },
                    ],
                },
            }, TestCase {
                raw: BytesMut::from([
                    0x82, // SUBSCRIBE packet type
                    0x10, // Remaining length
                    0x00, 0x01, // Packet ID
                    0x00, // Property length
                    0x00, 0x0A, // Topic name length
                    b't', b'e', b's', b't', b'/', b't', b'o', b'p', b'i', b'c', // Topic name
                    0x00, // QoS    
                ].as_slice()),
                exp: SubscribePacket { 
                    id: 1, 
                    list: vec![Subscribe {
                        topic: String::from("test/topic"),
                        max_qos: 0
                    }]
                }
            }
        ];

        for test in test_table {
            let mut buf = test.raw;
            let deserialized = SubscribePacket::decode(&mut buf).expect("Deserialization failed");
            println!("deserialize {:?}, expected {:?}", deserialized, test.exp);

            assert_eq!(deserialized.id, test.exp.id);
            assert_eq!(deserialized.list.len(), test.exp.list.len());
            for (deserialized_sub, expected_sub) in deserialized.list.iter().zip(test.exp.list.iter()) {
                assert_eq!(deserialized_sub.topic, expected_sub.topic);
                assert_eq!(deserialized_sub.max_qos, expected_sub.max_qos);
            }
            
        }
    }
}