// non-lean-yet
// Scope: enum Sync, struct Retained.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 56, 67.

use super::*;
use bevy::{
    asset::AssetId,
    camera::NormalizedRenderTarget,
    render::{
        Render, RenderSystems,
        camera::{SortedCameras, sort_cameras},
    },
};
use std::collections::{BTreeSet, HashMap};

pub(super) type Exports = HashMap<AssetId<Image>, ExportedImage>;

#[derive(Resource)]
struct ReusedTargets(BTreeSet<AssetId<Image>>);

pub(super) fn install(app: &mut App) {
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        skip_reused_targets
            .in_set(RenderSystems::CreateViews)
            .after(sort_cameras),
    );
}

fn skip_reused_targets(reused: Option<Res<ReusedTargets>>, mut cameras: ResMut<SortedCameras>) {
    let Some(reused) = reused else {
        return;
    };
    cameras.0.retain(|camera| {
        !matches!(&camera.target, Some(NormalizedRenderTarget::Image(image))
            if reused.0.contains(&image.handle.id()))
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sync {
    Incompatible,
    Compatible { pending: bool, update: Update },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Update {
    // Coalesced commits use the strongest update required by any member.
    None,
    Composition,
    Canvas,
}

#[derive(Clone)]
pub(super) struct Retained {
    pub geometry: Option<Arc<lince_interface::host::GeometrySnapshot>>,
    pub input: Arc<InputSnapshot>,
    pub windows: Arc<[WindowConfiguration]>,
    pub previewed: Arc<[lince_interface::host::SurfaceId]>,
}

impl Presentation {
    pub(crate) fn retains_frame(&self) -> bool {
        self.retained.is_some() && !self.suspended()
    }

    pub(crate) fn can_reuse_canvas_for_composition(&self) -> bool {
        // An active capture still needs its own source readiness and policy pass.
        self.app
            .world()
            .get_resource::<lince_interface::host::capture::Demand>()
            .is_none_or(lince_interface::host::capture::Demand::is_empty)
    }

    pub(crate) fn pixel_scene(&self, scene: &crate::runtime::Scene) -> bool {
        self.retains_frame()
            && scene.outputs.len() == self.output_state.outputs.len()
            && scene
                .outputs
                .iter()
                .zip(&self.output_state.outputs)
                .all(|(next, current)| {
                    current.name == next.name && current.viewport == next.viewport
                })
            && scene.display.as_ref().is_none_or(|next| {
                self.app
                    .world()
                    .get_resource::<lince_interface::host::display::DisplaySnapshot>()
                    == Some(next)
            })
    }

    #[cfg(test)]
    pub(crate) fn pixel_frame(&mut self) -> Result<Frame, ProbeFailure> {
        loop {
            if let Some(frame) = self.try_pixel_frame(None)? {
                return Ok(frame);
            }
            self.complete_pipelines();
        }
    }

    pub(crate) fn try_pixel_frame(
        &mut self,
        previous: Option<&Frame>,
    ) -> Result<Option<Frame>, ProbeFailure> {
        let _timing = lince_interface::host::performance::Span::new(
            lince_interface::host::performance::Stage::NativePixels,
        );
        let retained = self.retained.clone().ok_or_else(|| {
            input_failure("A native frame is required before retaining its presentation")
        })?;
        if let Some(time) = self.app.world().get_resource::<bevy::time::TimeReceiver>() {
            let _ = time.0.try_recv();
        }
        let previous = previous.filter(|previous| {
            Arc::ptr_eq(&retained.input, &previous.input)
                && self.can_reuse_canvas_for_composition()
                && !self.pipelines_pending()
        });
        let reused = previous.map_or_else(Exports::new, |previous| self.reuse_exports(previous));
        let render = self.app.sub_app_mut(RenderApp);
        if previous.is_some() {
            render.insert_resource(native::topology::RetainCaptureTopology);
        }
        if !reused.is_empty() {
            render.insert_resource(ReusedTargets(reused.keys().copied().collect()));
        }
        self.trace_render(true);
        self.app.update_sub_app_by_label(RenderApp);
        let render = self.app.sub_app_mut(RenderApp).world_mut();
        if previous.is_some() {
            render.remove_resource::<native::topology::RetainCaptureTopology>();
        }
        if !reused.is_empty() {
            render.remove_resource::<ReusedTargets>();
        }
        if let Some(error) = self.app.world().get_resource::<FrameFailure>() {
            return Err(ProbeFailure {
                stage: "native_frame".into(),
                detail: error.0.clone(),
            });
        }
        self.app
            .world_mut()
            .resource_mut::<Messages<bevy::asset::AssetEvent<clients::ClientMaterial>>>()
            .update();
        self.retain_capture_windows();
        if self.pipelines_pending() {
            self.release_capture_uploads();
            return Ok(None);
        }
        lince_interface::host::performance::count(
            lince_interface::host::performance::Count::PixelFrame,
        );
        let frame = self.export_frame(retained, reused)?;
        self.pixel_changed_owners.clear();
        Ok(Some(frame))
    }

    fn reuse_exports(&self, previous: &Frame) -> Exports {
        if self.pixel_changed_owners.is_empty() {
            return Exports::new();
        }
        let Some(focused) = self.output_state.assignments.focused() else {
            return Exports::new();
        };
        let targets = self
            .output_state
            .targets
            .iter()
            .filter(|(id, _)| **id != focused)
            .filter_map(|(id, target)| {
                previous
                    .outputs
                    .iter()
                    .find(|output| output.id == *id)
                    .map(|output| (target.image.id(), &output.image))
            })
            .chain(previous.image.iter().map(|image| (self.target.id(), image)))
            .collect::<Vec<_>>();
        if targets.len() < 2 {
            return Exports::new();
        }
        let independent = native::topology::independent_output_images(
            self.app.world(),
            targets.iter().map(|(target, _)| *target),
            &self.pixel_changed_owners,
        );
        targets
            .into_iter()
            .filter(|(target, _)| independent.contains(target))
            .map(|(target, image)| (target, image.clone()))
            .collect()
    }
}
