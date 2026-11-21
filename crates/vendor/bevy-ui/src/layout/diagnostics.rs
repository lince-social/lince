// non-lean-yet
// Scope: struct UiLayoutDiagnostics, struct UiLayoutReading.
// BeautifulRust (baseline lines):
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 5, 8.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 8,
//   9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26.

use bevy_ecs::resource::Resource;
use std::time::Duration;

#[derive(Resource)]
pub struct UiLayoutDiagnostics(pub fn(UiLayoutReading));

#[derive(Default)]
pub struct UiLayoutReading {
    pub synchronization: Duration,
    pub computation: Duration,
    pub geometry: Duration,
    pub nodes: u64,
    pub styles: u64,
    pub measures: u64,
    pub empty_measures: u64,
    pub image_measures: u64,
    pub fixed_measures: u64,
    pub text_input_measures: u64,
    pub text_measures: u64,
    pub custom_measures: u64,
    pub measure_noops: u64,
    pub targets: u64,
    pub hierarchy: u64,
    pub geometry_inputs: u64,
    pub roots: u64,
    pub geometry_nodes: u64,
}
