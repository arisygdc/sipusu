use bytes::{Buf, BytesMut};

pub mod mqtt;
pub mod subscribe;
pub mod v5;

pub struct RemainingLength;

impl RemainingLength {
    pub fn decode(buffer: &mut BytesMut) -> Result<u32, &'static str> {
        let mut multiplier = 1;
        let mut value = 0;
        
        loop {
            let encoded_byte = buffer.get_u8();
            value += ((encoded_byte & 127) as u32) * multiplier;
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