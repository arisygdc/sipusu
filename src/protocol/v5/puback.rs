use bytes::{BytesMut, BufMut};
use super::{encode_utf8_string, RemainingLength};

#[derive(Debug)]
pub struct PubackPacket {
    packet_id: u16,
    reason_code: u8,
    properties: Option<Properties>,
}

#[derive(Debug)]
pub struct Properties {
    reason_string: Option<String>,
    user_properties: Option<Vec<(String, String)>>,
}

impl PubackPacket {
    pub fn encode(&self) -> Result<BytesMut, String> {
        let mut buffer = BytesMut::new();
        // Properties
        let mut buf_prop = BytesMut::new();
        if let Some(properties) = &self.properties {
            buf_prop = encode_properties(properties)?;
        }

        let (prop_leng, plen_size) = RemainingLength::encode(buf_prop.len() as u32)?;
        let (prop_leng, _) = prop_leng.split_at(plen_size);

        let (remaining_length, rmlen_size) = RemainingLength::encode((plen_size + buf_prop.len() + 3) as u32)?;
        let (remaining_length, _) = remaining_length.split_at(rmlen_size);

        // Fixed header
        buffer.put_u8(0x40); // Packet type and flags

        // Remaining length
        buffer.put(remaining_length);

        // Variable header
        buffer.put_u16(self.packet_id);
        buffer.put_u8(self.reason_code);

        buffer.put(prop_leng);
        buffer.put(buf_prop);
        
        Ok(buffer)
    }
}

fn encode_properties(properties: &Properties) -> Result<BytesMut, String> {
    let mut props_buffer = BytesMut::new();

    if let Some(reason_string) = &properties.reason_string {
        props_buffer.put_u8(0x1F);
        encode_utf8_string(&mut props_buffer, reason_string)?;
    }

    if let Some(user_properties) = &properties.user_properties {
        for (key, value) in user_properties {
            props_buffer.put_u8(0x26);
            encode_utf8_string(&mut props_buffer, key)?;
            encode_utf8_string(&mut props_buffer, value)?;
        }
    }

    Ok(props_buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_puback() {
        let properties = Properties {
            reason_string: Some("Success".to_string()),
            user_properties: Some(vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]),
        };

        let packet = PubackPacket {
            packet_id: 1234,
            reason_code: 0,
            properties: Some(properties),
        };

        let buffer = packet.encode().unwrap();

        let mut expected = BytesMut::new();
        expected.put_u8(0x40); // Packet type and flags
        expected.put_u8(0x2C); // Remaining length

        expected.put_u16(1234); // Packet Identifier
        expected.put_u8(0); // Reason Code

        // Properties
        expected.put_u8(0x28); // Properties length
        expected.put_u8(0x1F); // Reason String identifier
        expected.put_u16(7); // Reason String length
        expected.put_slice("Success".as_bytes()); // 7 bytes

        expected.put_u8(0x26); // User Property identifier
        expected.put_u16(4); // Key length
        expected.put_slice("key1".as_bytes()); // 4 bytes
        expected.put_u16(6); // Value length
        expected.put_slice("value1".as_bytes()); // 6 bytes

        expected.put_u8(0x26); // User Property identifier
        expected.put_u16(4); // Key length
        expected.put_slice("key2".as_bytes()); // 4 bytes
        expected.put_u16(6); // Value length
        expected.put_slice("value2".as_bytes()); // 6 bytes
        println!("buffer len: {}", buffer.len());
        println!("expect len: {}", expected.len());
        assert_eq!(&buffer[..], &expected[..]);
    }
}