#![allow(dead_code)]
use bytes::{BufMut, BytesMut};

pub type SubAckResult = Result<ServiceLevel, SubAckInvalid>;
pub struct SubsAck {
    pub id: u16,
    pub subs_result: Vec<SubAckResult>
}

impl SubsAck {
    pub fn serialize(&self) -> BytesMut {
        let mut buf = BytesMut::from([0x90, 0x03].as_slice());
        buf.put_u16(self.id);
        buf.put_u8(0x00);
        for ack in &self.subs_result {
            let r = match ack {
                Ok(v) => v.code(),
                Err(e) => e.reason_code()
            };
            buf.put_u8(r);
        }
        buf
    }
}

pub enum ServiceLevel {
    QoS0, QoS1, QoS2
}

impl ServiceLevel {
    fn code(&self) -> u8 {
        match self {
            Self::QoS0 => 0x00,
            Self::QoS1 => 0x01,
            Self::QoS2 => 0x02
        }
    }

    pub fn max_qos() -> u8 {
        0x02
    }
}

impl TryFrom<u8> for ServiceLevel {
    type Error = SubAckInvalid;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::QoS0),
            0x01 => Ok(Self::QoS1),
            0x02 => Ok(Self::QoS2),
            _ => Err(SubAckInvalid::UnspecifiedError)
        }
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