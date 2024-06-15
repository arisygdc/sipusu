#![allow(dead_code)]
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

impl ToString for ConnError {
    fn to_string(&self) -> String {
        match self {
            ConnError::Custom(c) => c.msg.clone(),
            ConnError::Kind(k) => k.to_string()
        }
    }
}

#[derive(Clone, Copy)]
pub enum ErrorKind {
    TimedOut,
    InvalidData,
    BrokenPipe,
    ConnectionAborted
}

impl ToString for ErrorKind {
    fn to_string(&self) -> String {
        let bstr = match self {
            Self::BrokenPipe => "broken pipe",
            Self::ConnectionAborted => "connection aborted",
            Self::InvalidData => "invalid data",
            Self::TimedOut => "timeout"
        };
        String::from(bstr)
    }
}

pub struct WithMessage {
    msg: String,
    kind: ErrorKind
}
