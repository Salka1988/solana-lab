use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Domain(domain::Error),
    DuplicateCommand,
    ReplayMismatch,
    ActorClosed,
    JournalAppendFailed,
}

impl From<domain::Error> for Error {
    fn from(value: domain::Error) -> Self {
        Self::Domain(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(f, "{error}"),
            Self::DuplicateCommand => f.write_str("duplicate command"),
            Self::ReplayMismatch => f.write_str("replay mismatch"),
            Self::ActorClosed => f.write_str("actor closed"),
            Self::JournalAppendFailed => f.write_str("journal append failed"),
        }
    }
}

impl std::error::Error for Error {}
