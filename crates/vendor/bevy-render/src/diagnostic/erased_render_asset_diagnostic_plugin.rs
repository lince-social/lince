// non-lean-yet
// Scope: struct ErasedRenderAssetDiagnosticPlugin, struct ErasedRenderAssetMeasurements, fn
//   add_erased_render_asset_measurement, fn measure_erased_render_asset.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 70.
// - [rustc: elided_lifetimes_in_paths] Check API clarity and framework allowances before making
//   lifetimes explicit. Sites 65.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 17.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 23, 30.

use core::{any::type_name, marker::PhantomData};

use bevy_app::{Plugin, PreUpdate};
use bevy_diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy_ecs::{resource::Resource, system::Res};
use bevy_platform::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    erased_render_asset::{ErasedRenderAsset, ErasedRenderAssets},
    Extract, ExtractSchedule, RenderApp,
};

/// Collects diagnostics for a [`ErasedRenderAsset`].
///
/// If the [`ErasedRenderAsset::ErasedAsset`] is shared between other
/// [`ErasedRenderAsset`], they all will report the same number.
pub struct ErasedRenderAssetDiagnosticPlugin<A: ErasedRenderAsset> {
    suffix: &'static str,
    _phantom: PhantomData<A>,
}

impl<A: ErasedRenderAsset> ErasedRenderAssetDiagnosticPlugin<A> {
    pub fn new(suffix: &'static str) -> Self {
        Self {
            suffix,
            _phantom: PhantomData,
        }
    }

    pub fn render_asset_diagnostic_path() -> DiagnosticPath {
        DiagnosticPath::from_components(["erased_render_asset", type_name::<A>()])
    }
}

impl<A: ErasedRenderAsset> Plugin for ErasedRenderAssetDiagnosticPlugin<A> {
    fn build(&self, app: &mut bevy_app::App) {
        app.register_diagnostic(
            Diagnostic::new(Self::render_asset_diagnostic_path()).with_suffix(self.suffix),
        )
        .init_resource::<ErasedRenderAssetMeasurements<A>>()
        .add_systems(PreUpdate, add_erased_render_asset_measurement::<A>);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, measure_erased_render_asset::<A>);
        }
    }
}

#[derive(Debug, Resource)]
struct ErasedRenderAssetMeasurements<A: ErasedRenderAsset> {
    assets: AtomicUsize,
    _phantom: PhantomData<A>,
}

impl<A: ErasedRenderAsset> Default for ErasedRenderAssetMeasurements<A> {
    fn default() -> Self {
        Self {
            assets: AtomicUsize::default(),
            _phantom: PhantomData,
        }
    }
}

fn add_erased_render_asset_measurement<A: ErasedRenderAsset>(
    mut diagnostics: Diagnostics,
    measurements: Res<ErasedRenderAssetMeasurements<A>>,
) {
    diagnostics.add_measurement(
        &ErasedRenderAssetDiagnosticPlugin::<A>::render_asset_diagnostic_path(),
        || measurements.assets.load(Ordering::Relaxed) as f64,
    );
}

fn measure_erased_render_asset<A: ErasedRenderAsset>(
    measurements: Extract<Res<ErasedRenderAssetMeasurements<A>>>,
    assets: Res<ErasedRenderAssets<A::ErasedAsset>>,
) {
    measurements
        .assets
        .store(assets.iter().count(), Ordering::Relaxed);
}
