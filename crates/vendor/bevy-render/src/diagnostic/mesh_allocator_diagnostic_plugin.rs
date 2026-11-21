// non-lean-yet
// Scope: static MESH_ALLOCATOR_SLABS, static MESH_ALLOCATOR_SLABS_SIZE, static
//   MESH_ALLOCATOR_ALLOCATIONS, struct MeshAllocatorDiagnosticPlugin.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 39, 42, 45.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 68, 71, 74.
// - [rustc: elided_lifetimes_in_paths] Check API clarity and framework allowances before making
//   lifetimes explicit. Sites 64.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 19.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 19.

use bevy_app::{Plugin, PreUpdate};
use bevy_diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy_ecs::{resource::Resource, system::Res};
use bevy_platform::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::{mesh::allocator::MeshAllocator, Extract, ExtractSchedule, RenderApp};

/// Number of meshes allocated by the allocator
static MESH_ALLOCATOR_SLABS: DiagnosticPath = DiagnosticPath::const_new("mesh_allocator_slabs");

/// Total size of all slabs
static MESH_ALLOCATOR_SLABS_SIZE: DiagnosticPath =
    DiagnosticPath::const_new("mesh_allocator_slabs_size");

/// Number of meshes allocated into slabs
static MESH_ALLOCATOR_ALLOCATIONS: DiagnosticPath =
    DiagnosticPath::const_new("mesh_allocator_allocations");

pub struct MeshAllocatorDiagnosticPlugin;

impl MeshAllocatorDiagnosticPlugin {
    /// Get the [`DiagnosticPath`] for slab count
    pub fn slabs_diagnostic_path() -> &'static DiagnosticPath {
        &MESH_ALLOCATOR_SLABS
    }
    /// Get the [`DiagnosticPath`] for total slabs size
    pub fn slabs_size_diagnostic_path() -> &'static DiagnosticPath {
        &MESH_ALLOCATOR_SLABS_SIZE
    }
    /// Get the [`DiagnosticPath`] for mesh allocations
    pub fn allocations_diagnostic_path() -> &'static DiagnosticPath {
        &MESH_ALLOCATOR_ALLOCATIONS
    }
}

impl Plugin for MeshAllocatorDiagnosticPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        app.register_diagnostic(
            Diagnostic::new(MESH_ALLOCATOR_SLABS.clone()).with_suffix(" slabs"),
        )
        .register_diagnostic(
            Diagnostic::new(MESH_ALLOCATOR_SLABS_SIZE.clone()).with_suffix(" bytes"),
        )
        .register_diagnostic(
            Diagnostic::new(MESH_ALLOCATOR_ALLOCATIONS.clone()).with_suffix(" meshes"),
        )
        .init_resource::<MeshAllocatorMeasurements>()
        .add_systems(PreUpdate, add_mesh_allocator_measurement);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, measure_allocator);
        }
    }
}

#[derive(Debug, Default, Resource)]
struct MeshAllocatorMeasurements {
    slabs: AtomicUsize,
    slabs_size: AtomicU64,
    allocations: AtomicUsize,
}

fn add_mesh_allocator_measurement(
    mut diagnostics: Diagnostics,
    measurements: Res<MeshAllocatorMeasurements>,
) {
    diagnostics.add_measurement(&MESH_ALLOCATOR_SLABS, || {
        measurements.slabs.load(Ordering::Relaxed) as f64
    });
    diagnostics.add_measurement(&MESH_ALLOCATOR_SLABS_SIZE, || {
        measurements.slabs_size.load(Ordering::Relaxed) as f64
    });
    diagnostics.add_measurement(&MESH_ALLOCATOR_ALLOCATIONS, || {
        measurements.allocations.load(Ordering::Relaxed) as f64
    });
}

fn measure_allocator(
    measurements: Extract<Res<MeshAllocatorMeasurements>>,
    allocator: Res<MeshAllocator>,
) {
    measurements
        .slabs
        .store(allocator.slab_count(), Ordering::Relaxed);
    measurements
        .slabs_size
        .store(allocator.slabs_size(), Ordering::Relaxed);
    measurements
        .allocations
        .store(allocator.index_allocation_count(), Ordering::Relaxed);
}
