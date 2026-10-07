pub mod binary;
pub mod communication;
pub mod config;
pub mod conversation;
pub mod credential;
pub mod cub;
pub mod prompt;
pub mod runtime;
pub mod supervisor;
pub mod tools;

pub use communication::{acp, adapters, connection, driver, provider, speech};
pub use {
    binary::{FioteBinary, locate},
    credential::{CredentialSource, ProviderCredential},
    cub::{Cub, CubEvent, CubSpec, CubStatus},
    supervisor::Supervisor,
};
