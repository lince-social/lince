mod buffer;
mod diff;
pub mod explorer;
pub mod files;
pub mod indent;
pub mod language;
pub mod lsp;
pub mod operations;
pub mod project_search;
pub mod recovery;
pub mod search;
pub mod tooling;
pub mod watch;
pub mod words;

pub use buffer::{
    Buffer, Checkpoint, Conflict, EncodedSave, Reconciled, Reconciliation, Resolution, SavePoint,
    TextWindow,
};
pub use diff::Edit;

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WINDOW_BYTES: usize = 64 * 1024;
