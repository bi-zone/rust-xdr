error_chain! {
    foreign_links {
        IOError(::std::io::Error);
        InvalidUtf8(::std::string::FromUtf8Error);
    }

    errors {
        InvalidCase(v: i32) {
            description("invalid union case")
            display("invalid union case: {} (0x{:X})", v, v)
        }
        InvalidEnum(v: i32) {
            description("invalid enum value")
            display("invalid enum value: {} (0x{:X})", v, v)
        }
        InvalidLen(v: usize) {
            description("invalid array len")
            display("invalid array len: {} (0x{:X})", v, v)
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
}
