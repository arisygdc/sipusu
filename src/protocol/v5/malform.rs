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

pub enum Malformed {
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

impl Malformed {
    pub fn code(&self) -> u8 {
        match self {
            Malformed::MalformedPacket => 0x81,
            Malformed::ProtocolError => 0x82,
            Malformed::ReceiveMax => 0x93,
            Malformed::PacketTooLarge => 0x95,
            Malformed::RetainNotSupported => 0x9A,
            Malformed::QoSNotSupported => 0x9B,
            Malformed::SharedSubsUnsuppported => 0x9E,
            Malformed::SubsIdUnSupported => 0xA1,
            Malformed::WildcardSubsUnSupported => 0xA2,
        }
    }
}