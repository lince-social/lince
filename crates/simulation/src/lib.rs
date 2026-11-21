// non-lean-yet
// Scope: mod artifacts, mod publication, mod location_workflows, mod assumptions.
// BeautifulRust (baseline lines):
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 21, 24, 26, 27.

pub mod artifacts;
#[cfg(test)]
mod publication;
pub mod location_workflows;
pub mod assumptions;
pub mod loans;
pub mod campaign;
pub mod checks;
pub mod comparison;
pub mod proposal;
pub mod record_evidence;
pub mod limits;
pub mod cli;
pub mod fixtures;
pub mod lingua;
pub mod karma_preview;
mod network;
pub mod scenario;
mod transfers;
pub mod worker;
pub mod world;
pub use store::simulation_checks as saved_checks;

pub const BUILD_HASH: &str = env!("LINCE_SIMULATION_BUILD_HASH");

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
