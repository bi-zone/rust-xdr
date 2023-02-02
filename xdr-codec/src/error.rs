#![allow(deprecated)]
error_chain! {
    foreign_links {
        IOError(::std::io::Error);
        InvalidUtf8(::std::string::FromUtf8Error);
    }

    errors {
        #[deprecated]
        InvalidCase(v: i32) {
            description("invalid union case")
            display("invalid union case: {} (0x{:X})", v, v)
        }
        #[deprecated]
        InvalidEnum(v: i32) {
            description("invalid enum value")
            display("invalid enum value: {} (0x{:X})", v, v)
        }
        InvalidLen(v: usize) {
            description("invalid array len")
            display("invalid array len: {} (0x{:X})", v, v)
        }
        InvalidNamedCase(name: &'static str, v: i32) {
            description("invalid named union case")
            display("union '{}' - invalid case: {} (0x{:X})", name, v, v)
        }
        InvalidNamedEnum(name: &'static str, v: i32) {
            description("invalid named enum value")
            display("enum '{}' - invalid value: {} (0x{:X})", name, v, v)
        }
    }
}

unsafe impl Sync for Error {}

impl Error {
    pub fn invalidcase(v: i32) -> Error {
        ErrorKind::InvalidCase(v).into()
    }

    pub fn invalidenum(v: i32) -> Error {
        ErrorKind::InvalidEnum(v).into()
    }

    pub fn invalidlen(v: usize) -> Error {
        ErrorKind::InvalidLen(v).into()
    }

    pub fn invalid_named_case(name: &'static str, v: i32) -> Error {
        ErrorKind::InvalidNamedCase(name, v).into()
    }

    pub fn invalid_named_enum(name: &'static str, v: i32) -> Error {
        ErrorKind::InvalidNamedEnum(name, v).into()
    }

    #[cfg(test)]
    pub(crate) fn is_invalid_enum(&self) -> bool {
        matches!(self.kind(), ErrorKind::InvalidEnum(..) | ErrorKind::InvalidNamedEnum(..))
    }
}
