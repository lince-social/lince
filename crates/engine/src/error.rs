use std::fmt;

#[derive(Debug)]
pub enum EngineError {
    Store(store::StoreError),
    Nucleus(nucleus::NucleusError),
    UnknownRecord(String),
    Consequence(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Nucleus(e) => write!(f, "nucleus: {e}"),
            Self::UnknownRecord(t) => write!(f, "unknown record `{t}`"),
            Self::Consequence(m) => write!(f, "consequence: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<store::StoreError> for EngineError {
    fn from(e: store::StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<nucleus::NucleusError> for EngineError {
    fn from(e: nucleus::NucleusError) -> Self {
        Self::Nucleus(e)
    }
}
