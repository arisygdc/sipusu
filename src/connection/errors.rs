#![allow(dead_code)]

use std::fmt::Display;
pub enum ConnError {
    Kind(ErrorKind),
    Custom(WithMessage)
}

impl ConnError {
    pub fn new(kind: ErrorKind, msg: Option<String>) -> Self {
        match msg {
            Some(msg) => Self::Custom(WithMessage { msg, kind }),
            None => Self::Kind(kind),
        }
    }

    pub fn get_kind(&self) -> ErrorKind {
        match self {
            ConnError::Custom(c) => c.kind,
            ConnError::Kind(k) => *k
        }
    }
}

impl Display for ConnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let w = match self {
            ConnError::Custom(c) => c.msg.clone(),
            ConnError::Kind(k) => k.to_string()
        };
        write!(f, "{}", w)
    }
}

#[derive(Clone, Copy)]
pub enum ErrorKind {
    TimedOut,
    InvalidData,
    BrokenPipe,
    ConnectionAborted
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bstr = match self {
            Self::BrokenPipe => "broken pipe",
            Self::ConnectionAborted => "connection aborted",
            Self::InvalidData => "invalid data",
            Self::TimedOut => "timeout"
        };
        write!(f, "{}", bstr)
    }
}

pub struct WithMessage {
    msg: String,
    kind: ErrorKind
}
