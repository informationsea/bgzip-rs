use failure::{Backtrace, Context, Fail};
use std::fmt::{self, Display};

#[derive(Debug)]
pub struct BGZFError {
    inner: failure::Context<BGZFErrorKind>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum BGZFErrorKind {
    #[fail(display = "Failed to parse header at position: {}", _0)]
    HeaderParseError(u64),
    #[fail(display = "not tabix format")]
    NotTabix,
    #[fail(display = "not BGZF format")]
    NotBGZF,
    #[fail(display = "I/O Error")]
    IoError,
    #[fail(display = "Utf8 Error")]
    Utf8Error,
    #[fail(display = "Error: {}", _0)]
    Other(&'static str),
}

impl Fail for BGZFError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for BGZFError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl BGZFError {
    pub fn kind(&self) -> BGZFErrorKind {
        *self.inner.get_context()
    }
}

impl From<BGZFErrorKind> for BGZFError {
    fn from(kind: BGZFErrorKind) -> BGZFError {
        BGZFError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<BGZFErrorKind>> for BGZFError {
    fn from(inner: Context<BGZFErrorKind>) -> BGZFError {
        BGZFError { inner }
    }
}

impl From<std::io::Error> for BGZFError {
    fn from(e: std::io::Error) -> BGZFError {
        e.context(BGZFErrorKind::IoError).into()
    }
}

impl From<std::str::Utf8Error> for BGZFError {
    fn from(e: std::str::Utf8Error) -> BGZFError {
        e.context(BGZFErrorKind::Utf8Error).into()
    }
}
