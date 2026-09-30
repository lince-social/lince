pub mod artifacts;
pub mod assumptions;
pub mod loans;
pub mod campaign;
pub mod checks;
pub mod comparison;
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
