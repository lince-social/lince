#![recursion_limit = "256"]

extern crate self as lince_interface;

#[cfg(feature = "models")]
pub mod transfers;

#[cfg(feature = "map-presentation")]
pub mod map_view;

pub mod blob_sync;
pub mod credits;
pub use lince_practice::{handbook, practice};
#[cfg(feature = "activation")]
pub mod activation;
pub mod host;
#[cfg(feature = "images")]
pub mod images;
pub mod layout;
pub mod motion;
pub mod settings;
pub mod sound;
#[cfg(feature = "models")]
pub mod terrain;
pub mod text;

#[cfg(feature = "models")]
pub mod presentation;

#[cfg(feature = "ui")]
pub mod controls;
#[cfg(feature = "ui")]
pub mod location;
#[cfg(feature = "ui")]
pub mod style;
#[cfg(feature = "ui")]
pub mod theme;
pub mod tokens;
#[cfg(feature = "ui")]
pub mod visibility;
#[cfg(feature = "ui")]
pub mod wake;

#[cfg(feature = "models")]
pub mod calendar;
#[cfg(feature = "models")]
pub mod frequency;
#[cfg(feature = "models")]
pub mod karma;
#[cfg(feature = "models")]
pub mod markup;
#[cfg(feature = "models")]
pub mod organ;
#[cfg(feature = "models")]
pub mod qr;
#[cfg(feature = "models")]
pub mod queries;
#[cfg(feature = "models")]
pub mod records;
#[cfg(feature = "models")]
pub mod time_castle;

#[cfg(feature = "native-presentation")]
pub mod native;
