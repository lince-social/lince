// non-lean-yet
// Scope: mod app.
// BeautifulRust (baseline lines):
// - [review: §9 Modules] Check the intended public API and re-export individual items where
//   possible. Sites 1.

#[cfg(feature = "native-runtime")]
pub use lince_interface::native::*;

#[cfg(feature = "native-runtime")]
pub mod app;

#[cfg(feature = "native-runtime")]
pub use app::run_native_interface;
