//! Checks that layout emits through the observer used by the backend.

#![cfg(feature = "layout-telemetry")]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

use lince_host::layout::desktop::Workspace;
use lince_instrumentation::{Reading, install};

#[test]
fn layout_validation_reports_to_the_shared_observer() {
    static ELAPSED: AtomicBool = AtomicBool::new(false);
    static VALIDATIONS: AtomicU64 = AtomicU64::new(0);

    assert!(install(|reading| match reading {
        Reading::Count("desktop-model-validations", count) => {
            VALIDATIONS.fetch_add(count, Relaxed);
        }
        Reading::Elapsed("desktop-model-validation", _) => ELAPSED.store(true, Relaxed),
        _ => {}
    }));
    assert!(Workspace::default().valid());
    assert_eq!(VALIDATIONS.load(Relaxed), 1);
    assert!(ELAPSED.load(Relaxed));
}
