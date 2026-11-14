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
