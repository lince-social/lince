#[cfg(target_os = "linux")]
pub mod launch;

#[cfg(all(
    target_os = "linux",
    any(feature = "session", feature = "clipboard-gate")
))]
pub mod environment;

#[cfg(all(target_os = "linux", feature = "clipboard-gate"))]
pub mod clipboard;

#[cfg(all(target_os = "linux", feature = "graphics-probe"))]
pub mod graphics;

#[cfg(all(target_os = "linux", feature = "native-presentation"))]
pub mod presentation;

#[cfg(all(target_os = "linux", feature = "native-presentation"))]
pub mod runtime;

#[cfg(all(target_os = "linux", feature = "native-presentation"))]
pub mod native_input;

#[cfg(all(target_os = "linux", feature = "nested"))]
pub mod nested;

#[cfg(all(target_os = "linux", feature = "nested"))]
pub(crate) mod desktop;

#[cfg(all(target_os = "linux", feature = "nested"))]
mod commands;

#[cfg(all(target_os = "linux", feature = "session"))]
pub mod session;

#[cfg(all(target_os = "linux", feature = "server"))]
pub mod server;
