use std::io;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum SourceLang {
    English = 0,
    Korean = 1,
    French = 2,
    German = 3,
    Spanish = 4,
    Chinese = 5,
}

pub type ReadResult<T> = Result<T, ReadError>;
pub type WriteResult<T> = Result<T, WriteError>;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("unsupported protocol, expected {0} got {1}")]
    UnsupportedProtocol(u32, u32),

    #[error("cache key too long")]
    CacheKeyTooLong,

    #[error("invalid utf8 string")]
    InvalidUtf8String,

    #[error("invalid message length")]
    InvalidMessageLength,

    #[error("unknown message")]
    UnknownMessage,

    #[error("{0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("cache key too long")]
    CacheKeyTooLong,

    #[error("string too long")]
    StringTooLong,

    #[error("message too long")]
    MessageTooLong,

    #[error("{0}")]
    Io(#[from] io::Error),
}

mod logger;
mod module;
mod msg;
mod util;

pub use {logger::*, module::*, msg::*, util::*};
