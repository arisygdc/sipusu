use std::{fmt::Display, time::Duration};
use tokio::{io, time};

pub mod line;
pub mod handler;
pub mod handshake;
mod errors;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct ConnectionID(u32);
impl Display for ConnectionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)  
    }
}

pub trait SocketReader {
    async fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize>;
    async fn read_timeout(&mut self, buffer: &mut [u8], dur: Duration) -> io::Result<usize> {
        match time::timeout(dur, self.read(buffer)).await {
            Ok(v) => return v,
            Err(err) => Err(io::Error::new(io::ErrorKind::TimedOut, err.to_string()))
        }
    }
}

pub trait SocketWriter {
    fn write_all(&mut self, buffer: &[u8]) -> impl std::future::Future<Output = io::Result<()>> + Send;
}