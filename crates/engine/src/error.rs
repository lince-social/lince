use std::fmt;

#[derive(Debug)]
pub enum EngineError {
    Store(store::StoreError),
    Nucleus(nucleus::NucleusError),
    UnknownRecord(String),
    Consequence(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Forbidden(String),
    Conflict { code: &'static str, message: String },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Nucleus(e) => write!(f, "nucleus: {e}"),
            Self::UnknownRecord(t) => write!(f, "unknown record `{t}`"),
            Self::Consequence(m) => write!(f, "consequence: {m}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Json(e) => write!(f, "json: {e}"),
            Self::Forbidden(m) => write!(f, "forbidden: {m}"),
            Self::Conflict { message, .. } => write!(f, "conflict: {message}"),
        }
    }
}

impl EngineError {
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Conflict { code, .. } => Some(code),
            Self::Forbidden(_) => Some("forbidden"),
            Self::UnknownRecord(_) => Some("unknown_record"),
            _ => None,
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

impl From<std::io::Error> for EngineError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
