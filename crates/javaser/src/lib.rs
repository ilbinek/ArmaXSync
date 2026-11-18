//! Java Object Serialization Stream Protocol (version 5) reader and writer.
//!
//! The reader parses an arbitrary stream into an arena [`Graph`] that keeps the
//! exact handle numbering Java uses, so back-references (cycles such as tree
//! `parent` pointers) resolve naturally. The [`Writer`] emits streams that the
//! JDK's `ObjectInputStream` accepts, driven by static [`ClassSpec`] descriptors
//! supplied by the caller.

mod graph;
mod mutf8;
mod reader;
mod writer;

#[cfg(feature = "json")]
mod json;

pub use graph::*;
pub use mutf8::{decode_modified_utf8, encode_modified_utf8};
pub use reader::{parse, parse_with_limits, Limits};
pub use writer::{ClassSpec, EnumSpec, FieldSpec, Writer};

#[cfg(feature = "json")]
pub use json::to_json;

/// Stream constants from `java.io.ObjectStreamConstants`.
pub mod consts {
    pub const STREAM_MAGIC: u16 = 0xACED;
    pub const STREAM_VERSION: u16 = 5;

    pub const TC_NULL: u8 = 0x70;
    pub const TC_REFERENCE: u8 = 0x71;
    pub const TC_CLASSDESC: u8 = 0x72;
    pub const TC_OBJECT: u8 = 0x73;
    pub const TC_STRING: u8 = 0x74;
    pub const TC_ARRAY: u8 = 0x75;
    pub const TC_CLASS: u8 = 0x76;
    pub const TC_BLOCKDATA: u8 = 0x77;
    pub const TC_ENDBLOCKDATA: u8 = 0x78;
    pub const TC_RESET: u8 = 0x79;
    pub const TC_BLOCKDATALONG: u8 = 0x7A;
    pub const TC_EXCEPTION: u8 = 0x7B;
    pub const TC_LONGSTRING: u8 = 0x7C;
    pub const TC_PROXYCLASSDESC: u8 = 0x7D;
    pub const TC_ENUM: u8 = 0x7E;

    pub const BASE_WIRE_HANDLE: u32 = 0x7E_0000;

    pub const SC_WRITE_METHOD: u8 = 0x01;
    pub const SC_SERIALIZABLE: u8 = 0x02;
    pub const SC_EXTERNALIZABLE: u8 = 0x04;
    pub const SC_BLOCK_DATA: u8 = 0x08;
    pub const SC_ENUM: u8 = 0x10;
}

/// Errors produced while reading or writing a serialization stream.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected end of stream")]
    Eof,
    #[error("bad stream header {0:#06x} {1}")]
    BadHeader(u16, u16),
    #[error("unexpected type code {code:#04x} at offset {offset} ({context})")]
    UnexpectedTag {
        code: u8,
        offset: usize,
        context: &'static str,
    },
    #[error("invalid handle {0:#x}")]
    BadHandle(u32),
    #[error("handle {0:#x} does not refer to a {1}")]
    WrongKind(u32, &'static str),
    #[error("unsupported stream feature: {0}")]
    Unsupported(&'static str),
    #[error("invalid field type code {0:#04x}")]
    BadFieldType(u8),
    #[error("limit exceeded: {0}")]
    Limit(&'static str),
    #[error("invalid modified UTF-8 data")]
    BadUtf8,
    #[error("trailing data after root object")]
    Trailing,
}

pub type Result<T> = std::result::Result<T, Error>;
