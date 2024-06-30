#![allow(dead_code)]
use bytes::{Buf, BytesMut};

use super::{decode_binary_data, decode_string_pair, decode_utf8_string, RemainingLength, ServiceLevel};

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

        // Properties
        let properties = decode_properties(buffer)?;

        // Packet Identifier
        let packet_id = match qos.code() > 0 {
            true => Some(buffer.get_u16()),
            false => None
        };

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
}

fn decode_properties(buffer: &mut BytesMut) -> Result<Option<Properties>, String> {
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

pub struct FwdPublish {
    pub dup: bool,
    pub qos: ServiceLevel,
    pub retain: bool,
    pub topic: String,
    pub packet_id: Option<u16>,
    pub payload: Vec<u8>,
    pub properties: Option<Properties>,     
}

pub struct FwdProperties {
    pub payload_format_indicator: Option<u8>,
    pub message_expiry_interval: Option<u32>,
    pub user_properties: Option<Vec<(String, String)>>,
    pub content_type: Option<String>,
}

impl FwdPublish {
    pub fn encode(&self) -> Result<BytesMut, String> {
        let buffer = BytesMut::with_capacity(512);

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
}