// non-lean-yet
// Scope: mod audio, mod video, mod native, struct MediaError.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   5, 8, 18.

pub mod audio;
pub mod video;

#[cfg(feature = "native")]
pub mod native;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaError(pub String);

impl std::fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MediaError {}

pub type Result<T> = std::result::Result<T, MediaError>;
