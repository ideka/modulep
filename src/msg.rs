use std::{
    io::{Read, Write},
    sync::Arc,
};

use crate::{ReadError, ReadResult, WriteError, WriteResult};

#[derive(Debug)]
pub struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take_const<const N: usize>(&mut self) -> ReadResult<&'a [u8; N]> {
        let Some(chunk) = self.0.first_chunk::<N>() else {
            return Err(ReadError::InvalidMessageLength);
        };
        self.0 = &self.0[N..];
        Ok(chunk)
    }

    fn take(&mut self, n: usize) -> ReadResult<&'a [u8]> {
        let Some((head, tail)) = self.0.split_at_checked(n) else {
            return Err(ReadError::InvalidMessageLength);
        };
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> ReadResult<u8> {
        Ok(u8::from_le_bytes(*self.take_const()?))
    }

    fn u16(&mut self) -> ReadResult<u16> {
        Ok(u16::from_le_bytes(*self.take_const()?))
    }

    fn u32(&mut self) -> ReadResult<u32> {
        Ok(u32::from_le_bytes(*self.take_const()?))
    }

    fn u64(&mut self) -> ReadResult<u64> {
        Ok(u64::from_le_bytes(*self.take_const()?))
    }

    fn string(&mut self) -> ReadResult<String> {
        let len = self.u16()? as usize;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| ReadError::InvalidUtf8String)
    }
}

fn put_string(buf: &mut Vec<u8>, s: &str) -> WriteResult<()> {
    let len = u16::try_from(s.len()).map_err(|_| WriteError::StringTooLong)?;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
    Ok(())
}

const MAX_MESSAGE: u32 = 1 << 20; // sanity cap so garbage can't allocate 4 GB

pub trait Message: Sized {
    const SKIP_BAD: bool = true;

    /// Append kind byte + payload.
    fn encode(&self, buf: &mut Vec<u8>) -> WriteResult<()>;

    fn decode(c: &mut Cursor<'_>) -> ReadResult<Self>;

    fn write(&self, w: &mut impl Write) -> WriteResult<()> {
        let mut buf = vec![0u8; 4];
        self.encode(&mut buf)?;
        let len = u32::try_from(buf.len() - 4).map_err(|_| WriteError::MessageTooLong)?;
        if len > MAX_MESSAGE {
            return Err(WriteError::MessageTooLong);
        }
        buf[..4].copy_from_slice(&len.to_le_bytes());
        w.write_all(&buf)?;
        Ok(())
    }

    fn read(r: &mut impl Read) -> ReadResult<Self> {
        loop {
            let mut len = [0u8; 4];
            r.read_exact(&mut len)?;
            let len = u32::from_le_bytes(len);

            if len > MAX_MESSAGE {
                return Err(ReadError::InvalidMessageLength);
            }

            let mut payload = vec![0u8; len as usize];
            r.read_exact(&mut payload)?;

            let mut c = Cursor(&payload);
            match Self::decode(&mut c) {
                Ok(x) => return Ok(x),

                Err(ReadError::UnknownMessage) => {
                    log::warn!("skipping unknown message");
                }

                // Recoverable errors
                Err(ReadError::InvalidMessageLength | ReadError::InvalidUtf8String)
                    if Self::SKIP_BAD =>
                {
                    log::warn!("skipping malformed message");
                    continue;
                }

                // Irrecoverable errors
                Err(e) => return Err(e),
            };
        }
    }
}

fn put_text(buf: &mut Vec<u8>, id: u32, hash: u64, text: &str) -> WriteResult<()> {
    buf.extend_from_slice(&id.to_le_bytes());
    buf.extend_from_slice(&hash.to_le_bytes());
    put_string(buf, text)
}

fn get_text(c: &mut Cursor<'_>) -> ReadResult<(u32, u64, String)> {
    Ok((c.u32()?, c.u64()?, c.string()?))
}

#[derive(Debug)]
pub enum AddonMessage {
    Text((u32, u64, String)),
}

impl Message for AddonMessage {
    fn encode(&self, buf: &mut Vec<u8>) -> WriteResult<()> {
        match self {
            Self::Text((id, hash, t)) => {
                buf.push(0);
                put_text(buf, *id, *hash, t)
            }
        }
    }

    fn decode(c: &mut Cursor<'_>) -> ReadResult<Self> {
        Ok(match c.u8()? {
            0 => Self::Text(get_text(c)?),
            _ => return Err(ReadError::UnknownMessage),
        })
    }
}

#[derive(Debug)]
pub enum ModuleMessage {
    Text((u32, u64, String)),
    TextCancel(u32),
}

impl Message for ModuleMessage {
    fn encode(&self, buf: &mut Vec<u8>) -> WriteResult<()> {
        match self {
            Self::Text((id, hash, t)) => {
                buf.push(0);
                put_text(buf, *id, *hash, t)
            }
            Self::TextCancel(id) => {
                buf.push(1);
                buf.extend_from_slice(&id.to_le_bytes());
                Ok(())
            }
        }
    }

    fn decode(c: &mut Cursor<'_>) -> ReadResult<Self> {
        Ok(match c.u8()? {
            0 => Self::Text(get_text(c)?),
            1 => Self::TextCancel(c.u32()?),
            _ => return Err(ReadError::UnknownMessage),
        })
    }
}

#[derive(Debug)]
pub struct Handshake {
    pub protocol: u32,
    pub source_lang: i32,
    pub result_version: u32,
    pub cache_key: Arc<str>,
}

impl Handshake {
    const PROTOCOL: u32 = 2;
    const CACHE_KEY_MAX_LEN: usize = 32;

    pub fn new(source_lang: i32, result_version: u32, cache_key: Arc<str>) -> WriteResult<Self> {
        if cache_key.len() > Self::CACHE_KEY_MAX_LEN {
            return Err(WriteError::CacheKeyTooLong);
        }
        Ok(Self {
            protocol: Self::PROTOCOL,
            source_lang,
            result_version,
            cache_key,
        })
    }
}

impl Message for Handshake {
    const SKIP_BAD: bool = false;

    fn encode(&self, buf: &mut Vec<u8>) -> WriteResult<()> {
        buf.extend_from_slice(&Self::PROTOCOL.to_le_bytes());
        buf.extend_from_slice(&self.source_lang.to_le_bytes());
        buf.extend_from_slice(&self.result_version.to_le_bytes());
        put_string(buf, &self.cache_key)?;
        Ok(())
    }

    fn decode(c: &mut Cursor<'_>) -> ReadResult<Self> {
        let protocol = c.u32()?;
        if protocol != Self::PROTOCOL {
            return Err(ReadError::UnsupportedProtocol(Self::PROTOCOL, protocol));
        }
        let source_lang = c.u32()?.cast_signed();
        let result_version = c.u32()?;
        let cache_key = c.string()?;
        if cache_key.len() > Self::CACHE_KEY_MAX_LEN {
            return Err(ReadError::CacheKeyTooLong);
        }

        Ok(Self {
            protocol,
            source_lang,
            result_version,
            cache_key: Arc::from(cache_key),
        })
    }
}
