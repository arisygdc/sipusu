#![allow(dead_code)]
use bytes::{BufMut, BytesMut};

use super::{encode_utf8_string, RemainingLength, ServiceLevel};

pub type SubAckResult = Result<ServiceLevel, SubAckInvalid>;
// #[cfg_attr(test, derive(PartialEq))]
// #[derive(Debug, Default)]
pub struct SubsAck {
    pub id: u16,
    pub properties: Option<SubAckProperties>,
    pub return_codes: Vec<SubAckResult>
}

// #[cfg_attr(test, derive(PartialEq))]
// #[derive(Debug, Default)]
pub struct SubAckProperties {
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<(String, String)>>,
}

impl SubAckProperties {
    // estimate encode leng
    fn len(&self) -> usize {
        let mut leng = 0;
        if let Some(ref reason) = self.reason_string {
            leng += reason.len() + 3;
        }

        if let Some(ref uprop) = self.user_properties {
            for (k, v) in uprop.iter() {
                leng += k.len() + v.len() + 5;
            }
        }

        leng
    }

    fn encode(&self) -> Result<BytesMut, String> {
        let mut props_buffer = BytesMut::new();
    
        if let Some(reason_string) = &self.reason_string {
            props_buffer.put_u8(0x1F);
            encode_utf8_string(&mut props_buffer, reason_string)?;
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

impl SubsAck {
    pub fn encode(&self) -> Result<BytesMut, String> {
        let prop = match &self.properties {
            None => BytesMut::new(),
            Some(p) => p.encode()?
        };

        let (pl, plsz) = RemainingLength::encode(prop.len() as u32)?;
        let (prop_len, _) = pl.split_at(plsz);

        let rml_num = prop.len() + plsz + self.return_codes.len() + 2;
        let (rml, rlsz) = RemainingLength::encode(rml_num as u32)?;
        let (remaining_leng, _) = rml.split_at(rlsz);

        let mut buf = BytesMut::with_capacity(rml_num+rlsz+1);

        let header = 0x90u8;
        buf.put_u8(header);
        buf.put(remaining_leng);
        buf.put_u16(self.id);
        buf.put(prop_len);
        buf.put(prop);

        for ack in self.return_codes.iter() {
            let r = match ack {
                Ok(v) => v.code(),
                Err(e) => e.reason_code()
            };
            buf.put_u8(r);
        }
        Ok(buf)
    }
}

pub enum SubAckInvalid {
    UnspecifiedError,
    ImplSpecificError,
    NotAuthorized,
    TopicFilterInvalid,
    PacketIdentifierInUse,
    QuotaExceeded,
    SharedSubsUnsupported,
    SubsIdUnSupported,
    WildcardSubsUnSupported
}

impl SubAckInvalid {
    fn reason_code(&self) -> u8 {
        match self {
            Self::UnspecifiedError => 0x80,
            Self::ImplSpecificError => 0x83,
            Self::NotAuthorized => 0x87,
            Self::TopicFilterInvalid => 0x8F,
            Self::PacketIdentifierInUse => 0x91,
            Self::QuotaExceeded => 0x97,
            Self::SharedSubsUnsupported => 0x9E,
            Self::SubsIdUnSupported => 0xA1,
            Self::WildcardSubsUnSupported => 0xA2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_suback() {
        let properties = SubAckProperties {
            reason_string: Some("Success".to_string()),
            user_properties: Some(vec![
                ("key1".to_string(), "value1".to_string()),
                ("key2".to_string(), "value2".to_string()),
            ]),
        };

        let prop_len = properties.len();

        let packet = SubsAck {
            id: 1234,
            properties: Some(properties),
            return_codes: vec![
                Ok(ServiceLevel::QoS0), 
                Ok(ServiceLevel::QoS1), 
                Err(SubAckInvalid::UnspecifiedError)
            ], // Example return codes
        };

        let buffer = packet.encode().unwrap();

        let mut expected = BytesMut::new();
        expected.put_u8(0x90); // Packet type SUBACK
        expected.put_u8(0x2E); // Remaining length

        // Variable header
        expected.put_u16(1234u16); // Packet Identifier

        // Properties
        expected.put_u8(prop_len as u8); // Properties length
        expected.put_u8(0x1F); // Reason String
        expected.put_u16(7); // Length of "Success"
        expected.put_slice(b"Success");
        expected.put_u8(0x26); // User Property
        expected.put_u16(4); // Key length
        expected.put_slice(b"key1");
        expected.put_u16(6); // Value length
        expected.put_slice(b"value1");
        expected.put_u8(0x26); // User Property
        expected.put_u16(4); // Key length
        expected.put_slice(b"key2");
        expected.put_u16(6); // Value length
        expected.put_slice(b"value2");

        // Return Codes
        expected.put_u8(0x00);
        expected.put_u8(0x01);
        expected.put_u8(0x80);

        println!("len res: {}, len expect: {}", buffer.len(), expected.len());
        assert_eq!(&buffer[..], &expected[..]);
    }
}