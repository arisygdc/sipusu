pub mod client;
pub mod clients;
mod storage;

pub const DATA_STORE: &str = ".dbg_data/clients";

/// time base session control for `signaling`
/// 
/// compare internal variable with given time,
/// there is 3 main concept: alive, dead, expired.
/// 
/// alive and death indicates you may use this session or not.
/// when session is expire, you dont need to hold this connection.
pub trait SessionController {
    fn is_alive(&self, t: u64) -> bool;
    fn kill(&mut self);
    /// set `t` as checkpoint then add keep alive duration,
    /// generate error when old duration less than `t`
    fn keep_alive(&mut self, t: u64) -> Result<u64, String>;
    fn is_expired(&self, t: u64) -> bool;
}