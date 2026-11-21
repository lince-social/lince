// non-lean-yet
// Scope: mod capture, mod codec, mod ice, mod network.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 5,
//   6, 7, 9.

pub mod capture;
mod codec;
mod ice;
mod network;
pub mod peer;
pub mod playback;
pub mod preview;
mod relay;
pub mod session;
mod sources;

pub(crate) fn error(error: impl std::fmt::Display) -> crate::MediaError {
    crate::MediaError(error.to_string())
}

#[cfg(target_os = "linux")]
mod x11;
