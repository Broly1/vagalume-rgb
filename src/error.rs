use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The card could not be found, opened or written to.
    Device(String),
    /// A value or frame was refused before reaching the card.
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Device(text) | Error::Invalid(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for Error {}
