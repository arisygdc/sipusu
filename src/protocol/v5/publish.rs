#![allow(dead_code)]
use bytes::{Buf, BufMut, BytesMut};

use super::{decode_binary_data, decode_string_pair, decode_utf8_string, encode_utf8_string, RemainingLength, ServiceLevel};

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Default)]
pub struct PublishPacket {
    pub dup: bool,
    pub qos: ServiceLevel,
    pub retain: bool,
    pub topic: String,
    pub packet_id: Option<u16>,
    pub payload: Vec<u8>,
    pub properties: Option<Properties>,
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Default)]
pub struct Properties {
    pub payload_format_indicator: Option<u8>,
    pub message_expiry_interval: Option<u32>,
    pub topic_alias: Option<u16>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Option<Vec<(String, String)>>,
    pub subscription_identifier: Option<Vec<u32>>,
    pub content_type: Option<String>,
}

impl Properties {
    fn decode(buffer: &mut BytesMut) -> Result<Option<Self>, String> {
        let properties_length = RemainingLength::decode(buffer).map_err(|_| "Failed to decode properties length")? as usize;
        if properties_length == 0 {
            return Ok(None);
        }
    
        if buffer.len() < properties_length {
            return Err("Buffer too short for properties".to_string());
        }
    
        let mut properties = Properties {
            payload_format_indicator: None,
            message_expiry_interval: None,
            topic_alias: None,
            response_topic: None,
            correlation_data: None,
            user_properties: None,
            subscription_identifier: None,
            content_type: None,
        };
    
        let mut buf_prop = buffer.split_to(properties_length);
        while buf_prop.len() != 0 {
            let identifier = buf_prop.get_u8();
    
            match identifier {
                0x01 => properties.payload_format_indicator = Some(buf_prop.get_u8()),
                0x02 => properties.message_expiry_interval = Some(buf_prop.get_u32()),
                0x23 => properties.topic_alias = Some(buf_prop.get_u16()),
                0x08 => properties.response_topic = Some(decode_utf8_string(&mut buf_prop)?),
                0x09 => properties.correlation_data = Some(decode_binary_data(&mut buf_prop)?),
                0x26 => {
                    let user_props = decode_string_pair(&mut buf_prop)?;
                    match &mut properties.user_properties {
                        None => properties.user_properties = Some(vec![user_props]),
                        Some(prop) => prop.push(user_props),
                    }
                }
                0x0B => properties.subscription_identifier = Some(vec![RemainingLength::decode(&mut buf_prop)?]),
                0x03 => properties.content_type = Some(decode_utf8_string(&mut buf_prop)?),
                _ => return Err("Unknown property identifier".to_string()),
            }
        }
    
        Ok(Some(properties))
    }

    fn est_len(&self) -> usize {
        let mut len = 0;
        if self.payload_format_indicator.is_some() {
            len += 2;
        }

        if self.message_expiry_interval.is_some() {
            len += 5;
        }

        if let Some(content_type) = &self.content_type {
            len += 3 + content_type.len();
        }

        if let Some(user_properties) = &self.user_properties {
            for (key, value) in user_properties {
                len += 5 + key.len() + value.len();
            }
        }

        len
    }

    fn encode(&self) -> Result<BytesMut, String> {
        let prop_len = self.est_len();
        let (rml, sz) = RemainingLength::encode(prop_len as u32)?;
        let (rml, _) = rml.split_at(sz);
        
        let mut props_buffer = BytesMut::with_capacity(prop_len + rml.len());
        props_buffer.put(rml);

        if let Some(indicator) = self.payload_format_indicator {
            props_buffer.put_u8(0x01);
            props_buffer.put_u8(indicator);
        }

        if let Some(interval) = self.message_expiry_interval {
            props_buffer.put_u8(0x02);
            props_buffer.put_u32(interval);
        }

        if let Some(content_type) = &self.content_type {
            props_buffer.put_u8(0x03);
            encode_utf8_string(&mut props_buffer, &content_type)?;
        }

        if let Some(user_properties) = &self.user_properties {
            for (key, value) in user_properties {
                props_buffer.put_u8(0x26);
                encode_utf8_string(&mut props_buffer, key)?;
                encode_utf8_string(&mut props_buffer, value)?;
            }
        }

        Ok(props_buffer)
    }
}

impl PublishPacket {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, String> {
        if buffer.len() < 2 {
            return Err("Buffer too short".to_string());
        }

        // Fixed header
        let header = buffer.get_u8();
        let packet_type = header >> 4;
        if packet_type != 3 {
            return Err("Invalid packet type for PUBLISH".to_string());
        }

        let dup = (header & 0x08) != 0;
        let qos = ServiceLevel::try_from((header & 0x06) >> 1)?;
        let retain = (header & 0x01) != 0;

        // Remaining length
        let remaining_length = RemainingLength::decode(buffer).map_err(|_| "Failed to decode remaining length")? as usize;
        if buffer.len() < remaining_length {
            return Err("Buffer too short for remaining length".to_string());
        }

        // Topic
        let topic = decode_utf8_string(buffer)?;

        // Packet Identifier
        let packet_id = match qos.code() > 0 {
            true => Some(buffer.get_u16()),
            false => None
        };

        // Properties
        let properties = Properties::decode(buffer)?;

        // Payload
        let payload = buffer.to_vec();

        Ok(PublishPacket {
            dup,
            qos,
            retain,
            topic,
            packet_id,
            payload,
            properties,
        })
    }

    pub fn encode(&self) -> Result<BytesMut, String> {
        let prop = match &self.properties {
            Some(prop) => prop.encode()?,
            None => {
                let mut buf = BytesMut::with_capacity(1);
                buf.put_u8(0x00);
                buf
            }
        };

        let mut len = self.topic.len() + prop.len() + self.payload.len();
        if self.packet_id.is_some() {
            len += 2;
        }

        let (rml, sz) = RemainingLength::encode(len as u32)?;
        let (rml, _) = rml.split_at(sz);

        let buf_cap = len + rml.len() + 2;
        let mut buffer = BytesMut::with_capacity(buf_cap);

        // Fixed header
        let mut fixed_header: u8 = 0x30; // Packet type PUBLISH
        fixed_header |= (self.dup as u8) << 4;
        fixed_header |= (self.qos.code()) << 1;
        fixed_header |= self.retain as u8;
        buffer.put_u8(fixed_header);
        buffer.put(rml);

        // Variable header
        encode_utf8_string(&mut buffer, &self.topic)?;
        if let Some(packet_id) = self.packet_id {
            buffer.put_u16(packet_id);
        }

        // Properties
        buffer.put(prop);

        // Payload
        buffer.put(self.payload.as_slice());

        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    #[test]
    fn test_decode_publish() {
        let mut buffer = BytesMut::new();
        buffer.put_u8(0x30); // Packet type and flags (QoS 0)
        buffer.put_u8(0x14); // Remaining length

        // Topic
        buffer.put_u16(5); // Topic length
        buffer.put_slice(b"topic");

        // Properties length
        buffer.put_u8(0x07);

        // Payload Format Indicator (0x01, 1 byte)
        buffer.put_u8(0x01);
        buffer.put_u8(1);

        // Message Expiry Interval (0x02, 4 bytes)
        buffer.put_u8(0x02);
        buffer.put_u32(300);

        // Payload
        buffer.put_slice(b"hello");

        let packet = PublishPacket::decode(&mut buffer).unwrap();
        let expected_properties = Properties {
            payload_format_indicator: Some(1),
            message_expiry_interval: Some(300),
            topic_alias: None,
            response_topic: None,
            correlation_data: None,
            user_properties: None,
            subscription_identifier: None,
            content_type: None,
        };

        let expected_packet = PublishPacket {
            dup: false,
            qos: ServiceLevel::QoS0,
            retain: false,
            topic: "topic".to_string(),
            packet_id: None,
            payload: b"hello".to_vec(),
            properties: Some(expected_properties),
        };

        assert_eq!(packet, expected_packet);
    }

    #[test]
    fn dif() {
        let prop = Properties {
            payload_format_indicator: Some(1),
            message_expiry_interval: Some(300),
            topic_alias: None,
            response_topic: None,
            correlation_data: None,
            user_properties: None,
            subscription_identifier: None,
            content_type: None,
        };

        let packet = PublishPacket {
            dup: false,
            qos: ServiceLevel::QoS1,
            retain: false,
            topic: "topic".to_string(),
            packet_id: Some(12345),
            payload: b"hello".to_vec(),
            properties: Some(prop),
        };

        let mut encoded = packet.encode().unwrap();
        let decoded = PublishPacket::decode(&mut encoded).unwrap();

        assert_eq!(packet, decoded)
        
    }
}