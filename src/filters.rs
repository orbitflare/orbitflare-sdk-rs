#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memcmp {
    pub offset: u64,
    pub data: MemcmpData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemcmpData {
    Bytes(Vec<u8>),
    Base58(String),
    Base64(String),
}

impl Memcmp {
    pub fn bytes(offset: u64, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            offset,
            data: MemcmpData::Bytes(bytes.into()),
        }
    }

    pub fn base58(offset: u64, data: impl Into<String>) -> Self {
        Self {
            offset,
            data: MemcmpData::Base58(data.into()),
        }
    }

    pub fn base64(offset: u64, data: impl Into<String>) -> Self {
        Self {
            offset,
            data: MemcmpData::Base64(data.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lamports {
    Eq(u64),
    Ne(u64),
    Lt(u64),
    Gt(u64),
}
