// The Reason Codes used for Malformed Packet and Protocol Errors are:

// ·         0x81           Malformed Packet
// ·         0x82           Protocol Error
// ·         0x93           Receive Maximum exceeded
// ·         0x95           Packet too large
// ·         0x9A           Retain not supported
// ·         0x9B           QoS not supported
// ·         0x9E           Shared Subscriptions not supported
// ·         0xA1           Subscription Identifiers not supported
// ·         0xA2           Wildcard Subscriptions not supported

pub enum MalformedReq {
    MalformedPacket,
    ProtocolError,
    ReceiveMax,
    PacketTooLarge,
    RetainNotSupported,
    QoSNotSupported,
    SharedSubsUnsuppported,
    SubsIdUnSupported,
    WildcardSubsUnSupported
}

impl MalformedReq {
    pub fn code(&self) -> u8 {
        match self {
            MalformedReq::MalformedPacket => 0x81,
            MalformedReq::ProtocolError => 0x82,
            MalformedReq::ReceiveMax => 0x93,
            MalformedReq::PacketTooLarge => 0x95,
            MalformedReq::RetainNotSupported => 0x9A,
            MalformedReq::QoSNotSupported => 0x9B,
            MalformedReq::SharedSubsUnsuppported => 0x9E,
            MalformedReq::SubsIdUnSupported => 0xA1,
            MalformedReq::WildcardSubsUnSupported => 0xA2,
        }
    }
}