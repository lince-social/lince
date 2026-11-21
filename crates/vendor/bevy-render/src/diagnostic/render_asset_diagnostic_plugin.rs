// non-lean-yet
// Scope: struct RenderAssetDiagnosticPlugin, struct RenderAssetMeasurements, fn
//   add_render_asset_measurement, fn measure_render_asset.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 66.
// - [rustc: elided_lifetimes_in_paths] Check API clarity and framework allowances before making
//   lifetimes explicit. Sites 61.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 13.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 13, 19,
//   26.

use core::{any::type_name, marker::PhantomData};

use bevy_app::{Plugin, PreUpdate};
use bevy_diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy_ecs::{resource::Resource, system::Res};
use bevy_platform::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    render_asset::{RenderAsset, RenderAssets},
    Extract, ExtractSchedule, RenderApp,
};

pub struct RenderAssetDiagnosticPlugin<A: RenderAsset> {
    suffix: &'static str,
    _phantom: PhantomData<A>,
}

impl<A: RenderAsset> RenderAssetDiagnosticPlugin<A> {
    pub fn new(suffix: &'static str) -> Self {
        Self {
            suffix,
            _phantom: PhantomData,
        }
    }

    pub fn render_asset_diagnostic_path() -> DiagnosticPath {
        DiagnosticPath::from_components(["render_asset", type_name::<A>()])
    }
}

impl<A: RenderAsset> Plugin for RenderAssetDiagnosticPlugin<A> {
    fn build(&self, app: &mut bevy_app::App) {
        app.register_diagnostic(
            Diagnostic::new(Self::render_asset_diagnostic_path()).with_suffix(self.suffix),
        )
        .init_resource::<RenderAssetMeasurements<A>>()
        .add_systems(PreUpdate, add_render_asset_measurement::<A>);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, measure_render_asset::<A>);
        }
    }
}

#[derive(Debug, Resource)]
struct RenderAssetMeasurements<A: RenderAsset> {
    assets: AtomicUsize,
    _phantom: PhantomData<A>,
}

impl<A: RenderAsset> Default for RenderAssetMeasurements<A> {
    fn default() -> Self {
        Self {
            assets: AtomicUsize::default(),
            _phantom: PhantomData,
        }
    }
}

fn add_render_asset_measurement<A: RenderAsset>(
    mut diagnostics: Diagnostics,
    measurements: Res<RenderAssetMeasurements<A>>,
) {
    diagnostics.add_measurement(
        &RenderAssetDiagnosticPlugin::<A>::render_asset_diagnostic_path(),
        || measurements.assets.load(Ordering::Relaxed) as f64,
    );
}

fn measure_render_asset<A: RenderAsset>(
    measurements: Extract<Res<RenderAssetMeasurements<A>>>,
    assets: Res<RenderAssets<A>>,
) {
    measurements
        .assets
        .store(assets.iter().count(), Ordering::Relaxed);
}
