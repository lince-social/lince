#[cfg(feature = "native-runtime")]
pub use lince_interface::native::*;

#[cfg(feature = "native-runtime")]
pub mod app;

#[cfg(feature = "native-runtime")]
pub use app::run_native_interface;
