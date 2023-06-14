//#![allow(deprecated)]

pub type Result<T, E = Error> = std::result::Result<T, E>;

use std::io::Error as IOError;
use std::string::FromUtf8Error;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[deprecated]
    #[error("invalid union case: {case} (0x{case:X})")]
    InvalidCase{case: i32},
    #[deprecated]
    #[error("invalid enum value: {value} (0x{value:X})")]
    InvalidEnum{value: i32},
    #[error("invalid array len: {len} (0x{len:X})")]
    InvalidLen{len: usize},
    #[error("union '{name}' - invalid case: {value} (0x{value:X})")]
    InvalidNamedCase{name: &'static str, value: i32},
    #[error("enum '{name}' - invalid value: {value} (0x{value:X})")]
    InvalidNamedEnum{name: &'static str, value: i32},
    #[error("IO Error: {0}")]
    IOError(IOError),
    #[error("Invalid utf8: {0}")]
    InvalidUtf8(FromUtf8Error),
}

impl From<IOError> for Error {
    fn from(err: IOError) -> Self {
        Self::IOError(err)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Self {
        Self::InvalidUtf8(err)
    }
}

impl Error {
    #[allow(deprecated)]
    pub fn invalid_case(case: i32) -> Error {
        Error::InvalidCase{case}
    }

    #[allow(deprecated)]
    pub fn invalid_enum(value: i32) -> Error {
        Error::InvalidEnum{value}
    }

    pub fn invalid_len(len: usize) -> Error {
        Error::InvalidLen{len}
    }

    pub fn invalid_named_case(name: &'static str, value: i32) -> Error {
        Error::InvalidNamedCase{name, value}
    }

    pub fn invalid_named_enum(name: &'static str, value: i32) -> Error {
        Error::InvalidNamedEnum{name, value}
    }

    #[cfg(test)]
    #[allow(deprecated)]
    pub(crate) fn is_invalid_enum(&self) -> bool {
        matches!(self, Error::InvalidEnum{..} | Error::InvalidNamedEnum{..})
    }
}
