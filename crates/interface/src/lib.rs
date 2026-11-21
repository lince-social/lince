// non-lean-yet
// Scope: mod testing, mod transfers, mod map_navigation, mod map_view.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

#![recursion_limit = "256"]

extern crate self as lince_interface;

#[cfg(feature = "test-driver")]
pub mod testing;

#[cfg(feature = "models")]
pub use lince_presentation_models::{
    calendar, frequency, karma, markup, organ, presentation, qr, queries, records, terrain,
    time_castle, transfers,
};
pub use lince_presentation_models::{settings, sound, tokens};

#[cfg(feature = "map-presentation")]
pub mod map_navigation;

#[cfg(feature = "map-presentation")]
pub mod map_view;

pub mod blob_sync;
pub mod credits;
pub use lince_practice::{content, handbook, practice};
#[cfg(feature = "activation")]
pub mod activation;
pub use lince_host::{self as host, layout};
pub use lince_i18n as i18n;
#[cfg(feature = "images")]
pub mod images;
#[cfg(feature = "ui")]
pub mod ithkuil;
pub mod motion;
pub mod text;

#[cfg(feature = "ui")]
pub mod controls;
#[cfg(feature = "ui")]
pub mod location;
#[cfg(feature = "ui")]
pub mod style;
#[cfg(feature = "ui")]
pub mod theme;
#[cfg(feature = "ui")]
pub mod visibility;
#[cfg(feature = "ui")]
pub mod wake;

#[cfg(any(feature = "native-presentation", all(test, feature = "ui")))]
mod record_date_order;

#[cfg(any(feature = "native-presentation", test))]
mod monitor_arrangement;

#[cfg(any(feature = "native-presentation", test))]
mod pixel_scale;

#[cfg(any(feature = "native-presentation", test))]
mod record_splice;

#[cfg(any(feature = "native-presentation", all(test, feature = "ui")))]
mod work_timer_logs;

#[cfg(any(feature = "native-presentation", test))]
mod workspace_save_queue;

#[cfg(feature = "native-presentation")]
pub mod native;
