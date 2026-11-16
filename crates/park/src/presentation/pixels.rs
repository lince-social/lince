use super::*;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Sync {
    Incompatible,
    Pending,
    Partial,
    Ready,
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
            if let Some(frame) = self.try_pixel_frame()? {
                return Ok(frame);
            }
            self.complete_pipelines();
        }
    }

    pub(crate) fn try_pixel_frame(&mut self) -> Result<Option<Frame>, ProbeFailure> {
        let _timing = lince_interface::host::performance::Span::new(
            lince_interface::host::performance::Stage::NativePixels,
        );
        let retained = self.retained.clone().ok_or_else(|| {
            input_failure("A native frame is required before retaining its presentation")
        })?;
        if let Some(time) = self.app.world().get_resource::<bevy::time::TimeReceiver>() {
            let _ = time.0.try_recv();
        }
        self.app.update_sub_app_by_label(RenderApp);
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
        self.export_frame(retained).map(Some)
    }
}
