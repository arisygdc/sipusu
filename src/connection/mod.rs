use std::fmt::Display;

pub mod line;
pub mod handler;
mod errors;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct ConnectionID(u32);
impl Display for ConnectionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)  
    }
}