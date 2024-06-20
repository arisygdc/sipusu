#![allow(dead_code)]
use bytes::{Buf, BufMut, BytesMut};

use super::{
    decode_binary_data, 
    decode_string_pair, 
    decode_utf8_string,
    encode_binary_data,
    encode_utf8_string,
    RemainingLength
};

#[derive(Default)]
pub struct ConnackPacket {
    pub session_present: bool,  
    pub return_code: u8,
    pub properties: Option<Properties>,
}

// TODO: use map with rules
#[derive(Debug, PartialEq, Default)]
pub struct Properties {
    pub session_expiry_interval: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub maximum_qos: Option<u8>,
    pub retain_available: Option<u8>,
    pub maximum_packet_size: Option<u32>,
    pub assigned_client_identifier: Option<String>,
    pub topic_alias_maximum: Option<u16>,
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<(String, String)>>,
    pub wildcard_subscription_available: Option<u8>,
    pub subscription_identifier_available: Option<u8>,
    pub shared_subscription_available: Option<u8>,
    pub server_keep_alive: Option<u16>,
    pub response_information: Option<String>,
    pub server_reference: Option<String>,
    pub authentication_method: Option<String>,
    pub authentication_data: Option<Vec<u8>>,
}


impl ConnackPacket {
    #[cfg(test)]
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, String> {
        {
            if buffer.len() < 4 {
                return Err("Buffer too short".to_string());
            }
    
            // Fixed header
            let header = buffer.get_u8();
            let packet_type = header >> 4;
            if packet_type != 2 {
                return Err("Invalid packet type for CONNACK".to_string());
            }
        }
        
        {
            // Remaining length
            let remaining_length = RemainingLength::decode(buffer).map_err(|_| "Failed to decode remaining length")? as usize;
            
            if buffer.remaining() < remaining_length {
                return Err("Buffer too short for remaining length".to_string());
            }
        }

        // Variable header
        let session_present = (buffer.get_u8() & 0x01) != 0;
        let return_code = buffer.get_u8();

        // Properties
        let properties = decode_properties(buffer)?;

        Ok(ConnackPacket {
            session_present,
            return_code,
            properties,
        })
    }

    pub fn encode(&self) -> Result<BytesMut, String> {
        // Variable header
        let mut buf_vrheader = [0; 2];
        let session_present_flag = self.session_present as u8;
        buf_vrheader[0] = session_present_flag;
        buf_vrheader[1] = self.return_code;

        // Properties
        let mut buf_prop = BytesMut::new();

        if let Some(properties) = &self.properties {

            if let Some(session_expiry_interval) = properties.session_expiry_interval {
                buf_prop.put_u8(0x11);
                buf_prop.put_u32(session_expiry_interval);
            }
            if let Some(receive_maximum) = properties.receive_maximum {
                buf_prop.put_u8(0x21);
                buf_prop.put_u16(receive_maximum);
            }
            if let Some(maximum_qos) = properties.maximum_qos {
                buf_prop.put_u8(0x24);
                buf_prop.put_u8(maximum_qos);
            }
            if let Some(retain_available) = properties.retain_available {
                buf_prop.put_u8(0x25);
                buf_prop.put_u8(retain_available);
            }
            if let Some(maximum_packet_size) = properties.maximum_packet_size {
                buf_prop.put_u8(0x27);
                buf_prop.put_u32(maximum_packet_size);
            }
            if let Some(assigned_client_identifier) = &properties.assigned_client_identifier {
                buf_prop.put_u8(0x12);
                encode_utf8_string(&mut buf_prop, assigned_client_identifier)?;
            }
            if let Some(topic_alias_maximum) = properties.topic_alias_maximum {
                buf_prop.put_u8(0x22);
                buf_prop.put_u16(topic_alias_maximum);
            }
            if let Some(reason_string) = &properties.reason_string {
                buf_prop.put_u8(0x1F);
                encode_utf8_string(&mut buf_prop, reason_string)?;
            }
            if let Some(user_properties) = &properties.user_properties {
                for (key, value) in user_properties {
                    buf_prop.put_u8(0x26);
                    encode_utf8_string(&mut buf_prop, key)?;
                    encode_utf8_string(&mut buf_prop, value)?;
                }
            }
            if let Some(wildcard_subscription_available) = properties.wildcard_subscription_available {
                buf_prop.put_u8(0x28);
                buf_prop.put_u8(wildcard_subscription_available);
            }
            if let Some(subscription_identifier_available) = properties.subscription_identifier_available {
                buf_prop.put_u8(0x29);
                buf_prop.put_u8(subscription_identifier_available);
            }
            if let Some(shared_subscription_available) = properties.shared_subscription_available {
                buf_prop.put_u8(0x2A);
                buf_prop.put_u8(shared_subscription_available);
            }
            if let Some(server_keep_alive) = properties.server_keep_alive {
                buf_prop.put_u8(0x13);
                buf_prop.put_u16(server_keep_alive);
            }
            if let Some(response_information) = &properties.response_information {
                buf_prop.put_u8(0x1A);
                encode_utf8_string(&mut buf_prop, response_information)?;
            }
            if let Some(server_reference) = &properties.server_reference {
                buf_prop.put_u8(0x1C);
                encode_utf8_string(&mut buf_prop, server_reference)?;
            }
            if let Some(authentication_method) = &properties.authentication_method {
                buf_prop.put_u8(0x15);
                encode_utf8_string(&mut buf_prop, authentication_method)?;
            }
            if let Some(authentication_data) = &properties.authentication_data {
                buf_prop.put_u8(0x16);
                encode_binary_data(&mut buf_prop, authentication_data)?;
            }
        } else {
            // No properties
            buf_prop.put_u8(0);
        }
        
        // Properties remaining length
        let (prop_remleng, plsize) = RemainingLength::encode(buf_prop.len() as u32)?;
        let (prop_remleng, _) = prop_remleng.split_at(plsize);
        
        
        // Packet remaining length
        let remaining_leng = buf_prop.len() + buf_vrheader.len() + prop_remleng.len();
        let (packet_remleng, rsize) = RemainingLength::encode(remaining_leng as u32)?;
        let (packet_remleng, _) = packet_remleng.split_at(rsize);
        
        let mut buffer = BytesMut::with_capacity(remaining_leng + 10);
        // Fixed header
        buffer.put_u8(0x20); // Packet type and flags

        // Remaining length
        buffer.put(packet_remleng);
        buffer.put(buf_vrheader.as_slice());
        buffer.put(prop_remleng);
        buffer.put(buf_prop);


        Ok(buffer)
    }
}

fn decode_properties(buffer: &mut BytesMut) -> Result<Option<Properties>, String> {
    if buffer.is_empty() {
        return Ok(None);
    }

    let property_length = RemainingLength::decode(buffer).map_err(|_| "Failed to decode property length")? as usize;
    if property_length == 0 {
        return Ok(None);
    }

    let mut properties = Properties::default();


    while buffer.remaining() > 0 {
        let identifier = buffer.get_u8();

        match identifier {
            0x11 => properties.session_expiry_interval = Some(buffer.get_u32()),
            0x21 => properties.receive_maximum = Some(buffer.get_u16()),
            0x24 => properties.maximum_qos = Some(buffer.get_u8()),
            0x25 => properties.retain_available = Some(buffer.get_u8()),
            0x27 =>  properties.maximum_packet_size = Some(buffer.get_u32()),
            0x12 => {
                let value = decode_utf8_string(buffer)?;
                properties.assigned_client_identifier = Some(value);
            }
            0x22 => properties.topic_alias_maximum = Some(buffer.get_u16()),
            0x1F => {
                let value = decode_utf8_string(buffer)?;
                properties.reason_string = Some(value);
            }
            0x26 => {
                let user_props= decode_string_pair(buffer)?;
                match &mut properties.user_properties {
                    None => properties.user_properties = Some(vec![user_props]),
                    Some(v) => v.push(user_props)
                }
            }   
            0x28 => properties.wildcard_subscription_available = Some(buffer.get_u8()),
            0x29 => properties.subscription_identifier_available = Some(buffer.get_u8()),
            0x2A => properties.shared_subscription_available = Some(buffer.get_u8()),
            0x13 => properties.server_keep_alive = Some(buffer.get_u16()),
            0x1A => {
                let value = decode_utf8_string(buffer)?;
                properties.response_information = Some(value);
            }
            0x1C => {
                let value = decode_utf8_string(buffer)?;
                properties.server_reference = Some(value);
            }
            0x15 => {
                let value = decode_utf8_string(buffer)?;
                properties.authentication_method = Some(value);
            }
            0x16 => {
                let value= decode_binary_data(buffer)?;
                properties.authentication_data = Some(value);
            }
            _ => {
                return Err("Unknown property identifier".to_string());
            }
        }
    }

    Ok(Some(properties))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_connack() {
        let properties = Properties {
            session_expiry_interval: Some(120),
            receive_maximum: Some(10),
            maximum_qos: Some(1),
            retain_available: Some(1),
            maximum_packet_size: Some(256),
            assigned_client_identifier: Some("client-123".to_string()),
            topic_alias_maximum: Some(5),
            reason_string: Some("Success".to_string()),
            user_properties: Some(vec![("key1".to_string(), "value1".to_string()), ("key2".to_string(), "value2".to_string())]),
            wildcard_subscription_available: Some(1),
            subscription_identifier_available: Some(1),
            shared_subscription_available: Some(1),
            server_keep_alive: Some(30),
            response_information: Some("Info".to_string()),
            server_reference: Some("ServerRef".to_string()),
            authentication_method: Some("AuthMethod".to_string()),
            authentication_data: Some(vec![1, 2, 3, 4, 5]),
        };

        let packet = ConnackPacket {
            session_present: true,
            return_code: 0,
            properties: Some(properties),
        };

        let mut buffer = packet.encode().unwrap();
        let mut cbuf = buffer.clone();

        assert_eq!(cbuf.get_u8(), 0x20); // Packet type and flags
        let remaining_leng = RemainingLength::decode(&mut cbuf).unwrap();
        assert_eq!(remaining_leng, cbuf.len() as u32);

        assert_eq!(cbuf.get_u8(), 0x01); // Session present
        assert_eq!(cbuf.get_u8(), 0x00); // Return code

        let properties_len = RemainingLength::decode(&mut cbuf).unwrap();
        assert_eq!(properties_len, 122); // Properties length

        let mut expected = BytesMut::new();
        expected.put([
            0x20, 125, 0x01, 0x00, 122, 0x11,
        ].as_slice());
        expected.put_u32(120);
        expected.put_u8(0x21);
        expected.put_u16(10);
        expected.put([
            0x24, 1, 0x25, 1, 0x27
        ].as_slice());
        expected.put_u32(256);
        expected.put_u8(0x12);
        expected.put_u16(10);
        expected.put_slice("client-123".as_bytes());
        expected.put_u8(0x22);
        expected.put_u16(5);
        expected.put_u8(0x1F);
        expected.put_u16(7);
        expected.put_slice("Success".as_bytes());
        expected.put_u8(0x26);
        expected.put_u16(4);
        expected.put_slice("key1".as_bytes());
        expected.put_u16(6);
        expected.put_slice("value1".as_bytes());
        expected.put_u8(0x26);
        expected.put_u16(4);
        expected.put_slice("key2".as_bytes());
        expected.put_u16(6);
        let mut val = String::from("value2").into_bytes();
        val.append(&mut vec![0x28u8, 1, 0x29, 1, 0x2A, 1, 0x13]);
        expected.put_slice(&val);
        expected.put_u16(30);
        expected.put_u8(0x1A);
        expected.put_u16(4);
        expected.put_slice("Info".as_bytes());
        expected.put_u8(0x1C);
        expected.put_u16(9);
        expected.put_slice("ServerRef".as_bytes());
        expected.put_u8(0x15);
        expected.put_u16(10);
        expected.put_slice("AuthMethod".as_bytes());
        expected.put_u8(0x16);
        expected.put_u16(5);
        expected.put_slice(&[1, 2, 3, 4, 5]);

        assert_eq!(&buffer[..], &expected[..]);

        let decoded = ConnackPacket::decode(&mut buffer).unwrap();
        assert_eq!(decoded.session_present, packet.session_present);
        assert_eq!(decoded.properties, packet.properties)
    }
}

