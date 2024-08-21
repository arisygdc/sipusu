use crate::protocol::v5::ServiceLevel;
use super::client::clobj::ClientID;

mod nonshared;
pub mod topicrouter;

#[derive(Clone)]
pub struct SubscriberInstance {
    pub clid: ClientID,
    pub max_qos: ServiceLevel
}

impl PartialEq for SubscriberInstance {
    fn eq(&self, other: &Self) -> bool {
        self.clid.eq(&other.clid)
    }
}