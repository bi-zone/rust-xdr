use std::io::Error as IOError;

use crate::spec::{Decl, Value, Type};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("can't have unnamed type: {0:?}")]
    UnnamedType(Type),
    #[error("parsing error: {0}")]
    Parse(String),
    #[error("IO Error: {0}")]
    IOError(IOError),
    #[error("incompat selector {selector:?} case {value:?}")]
    IncompatSelector{selector: Decl, value: Value},
    #[error("discriminant value {value:?} unknown")]
    DiscriminantValueUnknown{value: Value},
    #[error("unimplemented type: {ty:?}")]
    UnimplementedType{ty: Type},
}

impl From<IOError> for Error {
    fn from(err: IOError) -> Self {
        Self::IOError(err)
    }
}
