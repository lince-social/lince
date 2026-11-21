// non-lean-yet
// Scope: mod elements, mod wayland.
// BeautifulRust (baseline lines):
// - [review: §9 Modules] Check the intended public API and re-export individual items where
//   possible. Sites 7, 8.

//! Utilities and helpers around the `Element` trait.

mod elements;
#[cfg(feature = "wayland_frontend")]
mod wayland;

pub use elements::*;
#[cfg(feature = "wayland_frontend")]
pub use wayland::*;
