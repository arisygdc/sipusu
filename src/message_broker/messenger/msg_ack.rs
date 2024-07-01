use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{helper::time::sys_now, message_broker::client::client::ClientID};

#[derive(Clone)]
pub struct MessageCoordinator {
    data: Arc<RwLock<Vec<Map>>>
}

impl MessageCoordinator {
    pub fn new() -> Self {
        Self { 
            data: Arc::new(RwLock::new(Vec::new())) 
        }
    }
}


pub enum MsgState {
    PubRec,
    PubRel,
    Publish,
}

impl MsgState {
    fn step(&self) -> u8 {
        match &self {
            Self::PubRec => 0,
            Self::PubRel => 1,
            Self::Publish => 2
        }
    }

    fn last_step() -> u8 {
        MsgState::Publish.step()
    }
}

pub struct Map {
    clid: ClientID,
    val: RwLock<Vec<SaveState>>
}

pub struct SaveState {
    packet_id: u16,
    ack: MsgState,
    expired_at: u64
}

pub enum MsgAckErrors {
    NotFound,
    AlreadyExists,
    StateExpired,
    InvalidResolveState
}

impl MessageCoordinator {
    pub async fn create(&self, clid: ClientID, packet_id: u16, expr_intrval_sec: u64) -> Result<(), MsgAckErrors> {
        let state = SaveState {
            packet_id,
            ack: MsgState::PubRec,
            expired_at: sys_now() + expr_intrval_sec
        };
    
        let mut data = self.data.write().await;
        for v in data.iter() {
            if !v.clid.eq(&clid) {
                continue;
            }
            let mut mval = v.val.write().await;
            for smv in mval.iter() {
                if smv.packet_id == packet_id {
                    return Err(MsgAckErrors::AlreadyExists);
                }
            }
            mval.push(state);
            return Ok(());
        }
        
        data.push(Map {
            clid,
            val: RwLock::new(vec![state])
        });
        Ok(())
    }

    pub async fn resolve(&self, clid: ClientID, packet_id: u16, ack: MsgState) -> Result<(), MsgAckErrors> {
        let data = self.data.read().await;
        for v in data.iter() {
            if v.clid.eq(&clid) {
                continue;
            }

            let now = sys_now();
            let mut mval = v.val.write().await;
            
            for smv in mval.iter_mut() {
                if smv.packet_id == packet_id {
                    let next_step = smv.ack.step() + 1;
                    if next_step == ack.step() {
                        if MsgState::last_step() == next_step {
                            // TODO: Remove
                        }

                        if now > smv.expired_at {
                            // Remove
                            return Err(MsgAckErrors::StateExpired);
                        }
                        smv.ack = ack;
                        return Ok(());
                    } else {
                        return Err(MsgAckErrors::InvalidResolveState);
                    }
                }
            }
        }
        Err(MsgAckErrors::NotFound)
    }
}