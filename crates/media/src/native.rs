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
