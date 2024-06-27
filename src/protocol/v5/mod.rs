pub mod connect;
pub mod connack;
pub mod subscribe;
pub mod subsack;
pub mod publish;
pub mod puback;
use bytes::{Buf, BufMut, BytesMut};


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
    
    pub fn encode(mut target: u32) -> Result<([u8; 4], usize), &'static str> {
        const MAX_ENCODABLE: u32 = 268435455;
        const MOD: u32 = 128;
        if target > MAX_ENCODABLE {
            return Err("error encode length");
        }
    
        let mut res: [u8; 4] = [0; 4];
        let mut encoded_byte: u8;
        let mut size: usize = 0;
    
        while target != 0 {
            encoded_byte = (target % MOD) as u8;
            target /= 128;
            if target > 0 {
                encoded_byte |= 128;
            }
        
            res[size] = encoded_byte;
            size += 1;   
        }
        Ok((res, size))
    }
}

fn decode_utf8_string(buffer: &mut BytesMut) -> Result<String, String> {
    let len = buffer.get_u16() as usize;
    if len > buffer.remaining() {
        return Err("buffer out of capacity".to_string());
    }

    let s = std::str::from_utf8(buffer.split_to(len).chunk())
        .map_err(|_| "Invalid UTF-8 string".to_string())?
        .to_string();
    Ok(s)
}

fn decode_binary_data(buffer: &mut BytesMut) -> Result<Vec<u8>, String> {
    let len = buffer.get_u16() as usize;
    if len > buffer.remaining() {
        return Err("buffer out of capacity".to_string());
    }
    let data = buffer.split_to(len);
    Ok(data.to_vec())
}

fn encode_utf8_string(buffer: &mut BytesMut, value: &str) -> Result<(), String> {
    if value.len() > u16::MAX as usize {
        return Err("String too long".to_string());
    }

    buffer.put_u16(value.len() as u16);
    buffer.put_slice(value.as_bytes());

    Ok(())
}

fn encode_binary_data(buffer: &mut BytesMut, data: &[u8]) -> Result<(), String> {
    if data.len() > u16::MAX as usize {
        return Err("Binary data too long".to_string());
    }

    buffer.put_u16(data.len() as u16);
    buffer.put_slice(data);

    Ok(())
}

pub(super) fn decode_string_pair(buffer: &mut BytesMut) -> Result<(String, String), String> {
    let key = decode_utf8_string(buffer)?;
    let value = decode_utf8_string(buffer)?;
    Ok((key, value))
}