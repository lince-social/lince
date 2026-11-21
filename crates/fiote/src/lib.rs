// non-lean-yet
// Scope: mod assistant, mod binary, mod communication, mod config.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   3, 4, 5, 6, 7, 8, 9, 10, 11, 12.

pub mod assistant;
pub mod binary;
pub mod communication;
pub mod config;
pub mod conversation;
pub mod credential;
pub mod credentials;
pub mod cub;
pub mod prompt;
pub mod runtime;
pub mod supervisor;
pub mod tools;

pub use binary::{FioteBinary, locate};
pub use communication::{acp, adapters, connection, driver, provider, speech};
pub use credential::{CredentialSource, ProviderCredential};
pub use cub::{Cub, CubEvent, CubSpec, CubStatus};
pub use supervisor::Supervisor;
