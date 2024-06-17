
#![allow(dead_code)]
use bytes::{Buf, BytesMut};
use super::{decode_binary_data, decode_string_pair, decode_utf8_string, RemainingLength};

#[derive(Debug)]
pub struct ConnectPacket {
    protocol_name: String,
    protocol_level: u8,
    connect_flags: u8,
    keep_alive: u16,
    properties: Option<Properties>,
    client_id: String,
    will_properties: Option<Properties>,
    will_topic: Option<String>,
    will_payload: Option<Vec<u8>>,
    username: Option<String>,
    password: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct Properties {
    session_expiry_interval: Option<u32>,
    receive_maximum: Option<u16>,
    maximum_packet_size: Option<u32>,
    topic_alias_maximum: Option<u16>,
    request_response_information: Option<u8>,
    request_problem_information: Option<u8>,
    user_properties: Option<Vec<(String, String)>>,
    authentication_method: Option<String>,
    authentication_data: Option<Vec<u8>>,
}

impl ConnectPacket {
    pub fn decode(buffer: &mut BytesMut) -> Result<Self, String> {
        {
            // Fixed header
            let header = buffer.get_u8();
            let packet_type = header >> 4;
            let _flags = header & 0x0F;
    
            if packet_type != 1 {
                return Err("Invalid packet type for CONNECT".to_string());
            }
        }

        {
            // Remaining Length
            let remaining_length = RemainingLength::decode(buffer)?;
            if buffer.len() < remaining_length as usize {
                return Err("Invalid buffer supplied".to_string());
            }
        }

        let (protocol_name, protocol_level) = {
            // Protocol Name
            let protocol_name = decode_utf8_string(buffer)?;
            if protocol_name != "MQTT" {
                return Err("Invalid protocol name".to_string());
            }

            // Protocol Level
            let protocol_level = buffer.get_u8();

            if protocol_level != 5 {
                return Err("Unsupported protocol level".to_string());
            }
            (protocol_name, protocol_level)
        };


        // Connect Flags
        let connect_flags = buffer.get_u8();

        // Keep Alive
        let keep_alive = buffer.get_u16();

        // Properties
        let properties = decode_properties(buffer)?;

        // Client ID
        let client_id = decode_utf8_string(buffer)?;

        let mut will_properties = None;
        let mut will_topic = None;
        let mut will_payload = None;
        let mut username = None;
        let mut password = None;

        if connect_flags & 0x04 > 0 {
            // Will Properties
            let props = decode_properties(buffer)?;
            will_properties = Some(props);

            // Will Topic
            let topic = decode_utf8_string(buffer)?;
            will_topic = Some(topic);

            // Will Payload
            let payload = decode_binary_data(buffer)?;
            will_payload = Some(payload);
        }

        if connect_flags & 0x80 > 0 {
            // Username
            let user = decode_utf8_string(buffer)?;
            username = Some(user);
        }

        if connect_flags & 0x40 > 0 {
            // Password
            let pass = decode_binary_data(buffer)?;
            password = Some(pass);
        }

        Ok(ConnectPacket {
            protocol_name,
            protocol_level,
            connect_flags,
            keep_alive,
            properties: Some(properties),
            client_id,
            will_properties,
            will_topic,
            will_payload,
            username,
            password,
        })
    }
}

// TODO: when double?
fn  decode_properties(buffer: &mut BytesMut) -> Result<Properties, String> {
    let property_length = RemainingLength::decode(buffer)?;
    let mut properties = Properties {
        session_expiry_interval: None,
        receive_maximum: None,
        maximum_packet_size: None,
        topic_alias_maximum: None,
        request_response_information: None,
        request_problem_information: None,
        user_properties: None,
        authentication_method: None,
        authentication_data: None,
    };

    for _ in 0..property_length {
        let identifier = buffer.get_u8();

        match identifier {
            0x11 => properties.session_expiry_interval = Some(buffer.get_u32()),
            0x21 => properties.receive_maximum = Some(buffer.get_u16()),
            0x27 => properties.maximum_packet_size = Some(buffer.get_u32()),
            0x22 => properties.topic_alias_maximum = Some(buffer.get_u16()),
            0x19 => properties.request_response_information = Some(buffer.get_u8()),
            0x17 => properties.request_problem_information = Some(buffer.get_u8()),
            0x26 => {
                let user_props = decode_string_pair(buffer)?;
                match &mut properties.user_properties {
                    None => properties.user_properties = Some(vec![user_props]),
                    Some(v) => v.push(user_props)
                }
            }
            0x15 => {
                let auth_method = decode_utf8_string(buffer)?;
                properties.authentication_method = Some(auth_method);
            }
            0x16 => {
                let auth_data = decode_binary_data(buffer)?;
                properties.authentication_data = Some(auth_data);
            }
            _ => {
                return Err("Unknown property identifier".to_string());
            }
        }
    }

    Ok(properties)
}


#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn test_decode_connect_packet() {
        let mut buffer = BytesMut::from(&[
            0x10, 0x13, // Fixed header (packet type and remaining length)
            0x00, 0x04, // Protocol name length
            0x4D, 0x51, 0x54, 0x54, // Protocol name ("MQTT")
            0x05, // Protocol level (5)
            0x02, // Connect flags
            0x00, 0x3C, // Keep alive (60 seconds)
            0x00, // Properties length (no properties)
            0x00, 0x08, // Client ID length
            0x63, 0x6C, 0x69, 0x65, 0x6E, 0x74, 0x49, 0x44, // Client ID ("clientID")
        ][..]);

        let result = ConnectPacket::decode(&mut buffer);
        assert!(result.is_ok());

        let packet = result.unwrap();
        assert_eq!(packet.protocol_name, "MQTT");
        assert_eq!(packet.protocol_level, 5);
        assert_eq!(packet.connect_flags, 0x02);
        assert_eq!(packet.keep_alive, 60);
        assert!(packet.properties.is_some());
        assert_eq!(packet.client_id, "clientID");
        assert!(packet.will_properties.is_none());
        assert!(packet.will_topic.is_none());
        assert!(packet.will_payload.is_none());
        assert!(packet.username.is_none());
        assert!(packet.password.is_none());
    }

    #[test]
    fn test_decode_connect_packet_with_username_password() {
        let mut buffer = BytesMut::from(&[
            0x10, 0x23, // Fixed header (packet type and remaining length)
            0x00, 0x04, // Protocol name length
            0x4D, 0x51, 0x54, 0x54, // Protocol name ("MQTT")
            0x05, // Protocol level (5)
            0xC0, // Connect flags (username and password)
            0x00, 0x3C, // Keep alive (60 seconds)
            0x00, // Properties length (no properties)
            0x00, 0x08, // Client ID length
            0x63, 0x6C, 0x69, 0x65, 0x6E, 0x74, 0x49, 0x44, // Client ID ("clientID")
            0x00, 0x08, // Username length
            0x75, 0x73, 0x65, 0x72, 0x6E, 0x61, 0x6D, 0x65, // Username ("username")
            0x00, 0x08, // Password length
            0x70, 0x61, 0x73, 0x73, 0x77, 0x6F, 0x72, 0x64, // Password ("password")
        ][..]);

        let result = ConnectPacket::decode(&mut buffer);
        assert!(result.is_ok());

        let packet = result.unwrap();
        assert_eq!(packet.protocol_name, "MQTT");
        assert_eq!(packet.protocol_level, 5);
        assert_eq!(packet.connect_flags, 0xC0);
        assert_eq!(packet.keep_alive, 60);
        assert!(packet.properties.is_some());
        assert_eq!(packet.client_id, "clientID");
        assert!(packet.will_properties.is_none());
        assert!(packet.will_topic.is_none());
        assert!(packet.will_payload.is_none());
        assert_eq!(packet.username, Some("username".to_string()));
        assert_eq!(packet.password, Some("password".into()));
    }

    #[test]
    fn test_decode_connect_packet_with_invalid_protocol() {
        let mut buffer = BytesMut::from(&[
            0x10, 0x0C, // Fixed header (packet type and remaining length)
            0x00, 0x04, // Protocol name length
            0x4D, 0x51, 0x54, 0x58, // Protocol name ("MQTX")
            0x05, // Protocol level (5)
            0x02, // Connect flags
            0x00, 0x3C, // Keep alive (60 seconds)
            0x00, // Properties length (no properties)
            0x00, 0x08, // Client ID length
            0x63, 0x6C, 0x69, 0x65, 0x6E, 0x74, 0x49, 0x44, // Client ID ("clientID")
        ][..]);

        let result = ConnectPacket::decode(&mut buffer);
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error, "Invalid protocol name");
    }

    #[test]
    fn test_decode_connect_packet_with_invalid_protocol_level() {
        let mut buffer = BytesMut::from(&[
            0x10, 0x0C, // Fixed header (packet type and remaining length)
            0x00, 0x04, // Protocol name length
            0x4D, 0x51, 0x54, 0x54, // Protocol name ("MQTT")
            0x04, // Protocol level (4)
            0x02, // Connect flags
            0x00, 0x3C, // Keep alive (60 seconds)
            0x00, // Properties length (no properties)
            0x00, 0x07, // Client ID length
            0x63, 0x6C, 0x69, 0x65, 0x6E, 0x74, 0x49, 0x44, // Client ID ("clientID")
        ][..]);

        let result = ConnectPacket::decode(&mut buffer);
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error, "Unsupported protocol level");
    }

    #[test]
    fn test_decode_connect_packet_with_will() {
        let mut buffer = BytesMut::from(&[
            0x10, 0x1D, // Fixed header (packet type and remaining length)
            0x00, 0x04, // Protocol name length
            0x4D, 0x51, 0x54, 0x54, // Protocol name ("MQTT")
            0x05, // Protocol level (5)
            0x0E, // Connect flags (Will flag, QoS 1)
            0x00, 0x3C, // Keep alive (60 seconds)
            0x00, // Properties length (no properties)
            0x00, 0x08, // Client ID length
            0x63, 0x6C, 0x69, 0x65, 0x6E, 0x74, 0x49, 0x44, // Client ID ("clientID")
            0x00, // Will Properties length (no properties)
            0x00, 0x04, // Will Topic length
            0x77, 0x69, 0x6C, 0x6C, // Will Topic ("will")
            0x00, 0x07, // Will Payload length
            0x6D, 0x65, 0x73, 0x73, 0x61, 0x67, 0x65, // Will Payload ("message")
        ][..]);

        let result = ConnectPacket::decode(&mut buffer);
        assert!(result.is_ok());

        let packet = result.unwrap();
        assert_eq!(packet.protocol_name, "MQTT");
        assert_eq!(packet.protocol_level, 5);
        assert_eq!(packet.connect_flags, 0x0E);
        assert_eq!(packet.keep_alive, 60);
        assert!(packet.properties.is_some());
        assert_eq!(packet.client_id, "clientID");
        assert!(packet.will_properties.is_some());
        assert_eq!(packet.will_topic, Some("will".to_string()));
        assert_eq!(packet.will_payload, Some("message".into()));
        assert!(packet.username.is_none());
        assert!(packet.password.is_none());
    }
}