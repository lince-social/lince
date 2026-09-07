#[cfg(feature = "legacy-runtime")]
include!("legacy.rs");

#[cfg(feature = "native-runtime")]
pub mod native;

#[cfg(feature = "native-runtime")]
pub use native::run_native_interface;
