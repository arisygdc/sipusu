use std::time::Duration;

use crate::message_broker::{client::client::ClientID, Forwarder};
use crate::protocol::v5::{puback::{PubACKType, PubackPacket}, publish::PublishPacket};
use super::{msg_ack::MessageCoordinator, DistMsgStrategy};

pub struct MessageDistributor {
    // QoS2 only
    coordinator: MessageCoordinator
}

impl MessageDistributor {
    pub fn new() -> Self {
        Self {
            coordinator: MessageCoordinator::new()
        }
    }
}

impl<S> DistMsgStrategy<S> for MessageDistributor 
where S: Forwarder + Send + Sync + 'static
{
    fn qos0(&self, sender: S, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String> {
        if cfg!(debug_assertion) {
            if msg.qos.code() != 0 {
                panic!("Invalid qos")
            }
        }
        
        let buffer = msg.encode()?;
        tokio::spawn(async move {
            for clid in subscribers {
                let _ = sender.pubish(&clid, &buffer).await;
            }
        });
        Ok(())
    }

    fn qos1(&self, sender: S, publisher: ClientID, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String> {
        if cfg!(debug_assertion) {
            if msg.qos.code() != 1 {
                panic!("Invalid qos")
            }
        }
        
        let buffer = msg.encode()?;
        tokio::spawn(async move {
            let mut sent = 0;
            for clid in subscribers {
                let sending = sender.pubish(&clid, &buffer).await;
                if sending.is_ok() {
                    sent += 1
                }
            }

            if sent > 0 {
                let puback = PubackPacket {
                    packet_id: msg.packet_id.unwrap(),
                    packet_type: PubACKType::PubAck,
                    properties: None,
                    reason_code: 0
                };

                let buffer = puback.encode().unwrap();

                for i in 0..10 {
                    let callback = sender.pubish(&publisher, &buffer).await;
                    let err = match callback {
                        Ok(_) => break,
                        Err(err) => err
                    };

                    println!("{}", err.to_string());
                    tokio::time::sleep(Duration::from_secs((i + 1) * 2)).await;
                }
            }
        });
        Ok(())
    }

    fn qos2(&self, sender: S, publisher: ClientID, subscribers: Vec<ClientID>, msg: PublishPacket) -> Result<(), String> {
        if cfg!(debug_assertion) {
            if msg.qos.code() != 2 {
                panic!("Invalid qos")
            }
        }

        // let coordinator = self.coordinator.clone();
        let packet_id = msg.packet_id
            .ok_or(String::from("packet id is required on QoS2"))?;

        let msg_buffer = msg.encode()?;

        tokio::spawn(async move {
            let mut ack = PubackPacket {
                packet_id,
                packet_type: PubACKType::PubRec,
                properties: None,
                reason_code: 0x00
            };

            let buffer = ack.encode().unwrap();
            let send = sender.pubish(&publisher, &buffer).await;
            match send {
                Ok(_) => {},
                Err(err) => {
                    println!("{}", err.to_string());
                    return ;
                }
            };

            tokio::time::sleep(Duration::from_secs(1)).await;
            ack.packet_type = PubACKType::PubRel;

            let buffer = ack.encode().unwrap();
            let send = sender.pubish(&publisher, &buffer).await;
            match send {
                Ok(_) => {},
                Err(err) => {
                    println!("{}", err.to_string());
                    return ;
                }
            };

            let mut sent = false;
            for cl in subscribers.iter() {
                let send = sender.pubish(&cl, &msg_buffer).await;
                match send {
                    Ok(_) => {
                        sent = true;
                        break;
                    }, Err(err) => {
                        println!("{}", err.to_string());
                        continue;
                    }
                };
            }

            if sent {
                
            }
        });
        Ok(())
    }
}