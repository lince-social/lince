use super::*;
use lince_interface::{host::capture::{Policy, PolicyGate}, layout::desktop::PlacementId};

mod render;
#[cfg(all(test, feature = "nested", feature = "server-tests"))]
mod testing;

pub(crate) struct CaptureFrame {
    pub revision: u64,
    pub mapping_revision: u64,
    pub mappings: Vec<lince_interface::host::capture::Mapping>,
    pub policy: Policy,
    pub native_private_focus: bool,
    images: Option<std::collections::BTreeMap<lince_interface::host::OutputId, ExportedImage>>,
}

#[derive(Resource, Default)]
struct CaptureError(Option<String>);

impl Presentation {
    pub(super) fn retain_capture_windows(&self) {
        let demand = self.app.world().get_resource::<lince_interface::host::capture::Demand>().cloned().unwrap_or_default();
        let permitted = if self.suspended() { Default::default() } else {
            self.app.world().get_resource::<PolicyGate>().and_then(|gate| gate.read()).map(|read| {
                demand.windows.iter().copied().filter(|surface| read.permits(lince_interface::host::capture::Target::Window(*surface)))
                    .collect::<std::collections::BTreeSet<_>>()
            }).unwrap_or_default()
        };
        let mut pending = false;
        for (surface, window) in &self.painted_windows {
            for node in &window.nodes {
                if window.mapped && permitted.contains(surface) {
                    pending |= !self.renderer.retain_capture(&node.content);
                } else {
                    crate::graphics::retire_capture(&node.content);
                }
            }
        }
        if pending && let Some(wake) = self.app.world().get_resource::<WakeSignal>() {
            wake.after(std::time::Duration::from_millis(16));
        }
    }

    pub(super) fn capture_frame(&mut self) -> Option<CaptureFrame> {
        let gate = self.app.world().get_resource::<PolicyGate>()?.clone();
        let (revision, policy, filtered) = {
            let read = gate.read()?;
            (read.revision(), read.policy()?.clone(), read.requires_filter())
        };
        let mapping_revision = self.app.world().get_resource::<lince_interface::host::display::DisplaySnapshot>().map_or(0, |display| display.revision);
        let demand = self.app.world().get_resource::<lince_interface::host::capture::Demand>().cloned().unwrap_or_default();
        let mappings = self.app.world().get_resource::<lince_interface::host::display::DisplaySnapshot>().map(|display| display.outputs.iter().map(Into::into).collect()).unwrap_or_default();
        if !filtered { return Some(CaptureFrame { revision, mapping_revision, mappings, policy, native_private_focus: false, images: None }); }
        if demand.outputs.is_empty() {
            let additional = self.capture_private_roots(&policy);
            let native_private_focus = self.app.world().get_resource::<bevy::input_focus::InputFocus>().and_then(bevy::input_focus::InputFocus::get)
                .map_or(!policy.excluded.is_empty(), |entity| native::capture::excluded_with(self.app.world(), entity, &additional));
            return Some(CaptureFrame { revision, mapping_revision, mappings, policy, native_private_focus, images: Some(Default::default()) });
        }
        match self.filtered_capture(revision, &policy, &demand) {
            Ok((images, native_private_focus)) => {
                self.app.world_mut().insert_resource(CaptureError::default());
                Some(CaptureFrame { revision, mapping_revision, mappings, policy, native_private_focus, images: Some(images) })
            }
            Err(error) => {
                if self.app.world().get_resource::<CaptureError>().is_none_or(|previous| previous.0.as_deref() != Some(error.as_str())) {
                    native::notifications::report(self.app.world(), "External capture", &error);
                    self.app.world_mut().insert_resource(CaptureError(Some(error)));
                }
                if let Some(wake) = self.app.world().get_resource::<WakeSignal>() { wake.after(std::time::Duration::from_millis(16)); }
                None
            }
        }
    }

    pub(super) fn capture_private_roots(&mut self, policy: &Policy) -> std::collections::BTreeSet<Entity> {
        let mut roots: std::collections::BTreeSet<_> = self.windows.iter().filter(|(surface, _)| policy.windows.get(surface) != Some(&false)).map(|(_, window)| window.entity).collect();
        roots.extend(native::capture::private_window_sources(self.app.world_mut(), policy));
        let placements: std::collections::BTreeSet<_> = self.identities.iter().filter(|(surface, _)| policy.windows.get(surface) != Some(&false)).map(|(_, placement)| placement.0).collect();
        let inspectors: Vec<_> = self.app.world_mut().query_filtered::<(Entity, &native::full_record::window::Metadata), bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>>()
            .iter(self.app.world()).map(|(entity, metadata)| (entity, metadata.placement, metadata.uid.clone())).collect();
        for (entity, placement, uid) in inspectors {
            let saved = self.records.as_ref().is_none_or(|records| records.capture_metadata_excluded(PlacementId(placement), &uid));
            if saved || placements.contains(&placement) || policy.excluded.contains(&placement) { roots.insert(entity); }
        }
        roots
    }

    pub(super) fn release_capture_uploads(&mut self) {
        let uploads = std::mem::take(&mut self.uploads);
        self.renderer.queue.on_submitted_work_done(move || { for buffer in uploads { buffer.release(); } });
    }

    pub(super) fn capture_settings(&mut self) {
        self.capture_profiles();
        for edit in native::capture::take_edits(self.app.world_mut()) {
            let result = native::capture::validate_edit(self.app.world(), &edit).map_err(str::to_owned).and_then(|()| {
                let root = self.canvas_root().map_err(|error| error.detail)?;
                if root != edit.owner || self.suspended() { return Err("The original Park canvas is inactive or unavailable.".into()); }
                if let Some(gate) = self.app.world().get_resource::<PolicyGate>() { gate.invalidate(); }
                let records = self.records.as_mut().ok_or("Window profile storage is unavailable.")?;
                records.capture_exclusion(PlacementId(edit.placement), &edit.uid, edit.excluded).map_err(|error| error.to_string())?;
                let entities: Vec<_> = self.windows.iter().filter(|(surface, _)| self.identities.get(surface) == Some(&PlacementId(edit.placement))).map(|(_, window)| window.entity).collect();
                for entity in entities {
                    if let Some(mut layout) = self.app.world_mut().get_mut::<native::layout::LayoutBox>(entity)
                        && layout.id == edit.placement
                    { layout.capture_excluded = edit.excluded; }
                }
                Ok(())
            });
            self.capture_profiles();
            self.publish_capture_policy();
            native::capture::complete_edit(self.app.world_mut(), &edit, result);
        }
    }

    fn capture_profiles(&mut self) {
        let targets = native::capture::requested_windows(self.app.world_mut());
        let policies = targets.into_iter().filter_map(|(placement, uid)| {
            let profile = self.records.as_ref()?.profile(PlacementId(placement))?;
            if profile.uid.as_deref() != Some(uid.as_str()) { return None; }
            Some(native::capture::WindowPolicy { placement, uid, excluded: profile.layout.capture_excluded })
        }).collect();
        native::capture::publish_windows(self.app.world_mut(), self.records.is_some(), policies);
    }

    pub(crate) fn publish_capture_policy(&mut self) {
        let Some(gate) = self.app.world().get_resource::<PolicyGate>().cloned() else { return; };
        let mut policy = Policy::default();
        for (surface, window) in &self.windows {
            let mut private = native::capture::excluded(self.app.world(), window.entity);
            let mut parent = window.parent;
            let mut seen = std::collections::BTreeSet::from([*surface]);
            while let Some(id) = parent {
                if seen.len() >= native::layout::MAX_DEPTH || !seen.insert(id) { private = true; break; }
                let Some(window) = self.windows.get(&id) else { private = true; break; };
                private |= native::capture::excluded(self.app.world(), window.entity);
                parent = window.parent;
            }
            policy.windows.insert(*surface, private);
        }
        let additional = self.capture_private_roots(&policy);
        for entity in &additional {
            if let Some(metadata) = self.app.world().get::<native::full_record::window::Metadata>(*entity) { policy.excluded.insert(metadata.placement); }
        }
        let entities: Vec<_> = self.app.world_mut().query_filtered::<Entity, bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>>().iter(self.app.world()).take(lince_interface::host::capture::MAX_CONTRIBUTORS + 1).collect();
        if entities.len() > lince_interface::host::capture::MAX_CONTRIBUTORS { gate.invalidate(); return; }
        for entity in entities {
            if native::capture::excluded_with(self.app.world(), entity, &additional) {
                policy.private_entities.extend(native::capture::semantic_sources(self.app.world(), entity).into_iter().map(Entity::to_bits));
                if let Some(layout) = self.app.world().get::<native::layout::LayoutBox>(entity) { policy.excluded.insert(layout.id); }
            }
        }
        let targets = match native::capture::render::Targets::collect(self.app.world_mut(), &additional) {
            Ok(targets) => targets,
            Err(_) => { gate.invalidate(); return; }
        };
        policy.private_entities.extend(targets.private_entities);
        policy.unresolved_images = targets.unresolved;
        gate.publish(policy);
    }
}

impl Frame {
    pub(crate) fn capture_source(&self) -> Option<&CaptureFrame> { self.capture.as_ref() }

    pub(crate) fn capture_dmabuf(&self, output: lince_interface::host::OutputId, revision: u64, mapping_revision: u64) -> Option<&smithay::backend::allocator::dmabuf::Dmabuf> {
        let capture = self.capture.as_ref().filter(|capture| capture.revision == revision && capture.mapping_revision == mapping_revision)?;
        if let Some(images) = &capture.images { return images.get(&output).map(|image| &image.dmabuf); }
        if self.output.unwrap_or(lince_interface::host::OutputId(1)) == output { self.dmabuf() }
        else { self.outputs.iter().find(|image| image.id == output).map(OutputFrame::dmabuf) }
    }

    pub(crate) fn capture_ready(&self, output: lince_interface::host::OutputId) -> Result<bool, ProbeFailure> {
        let Some(capture) = &self.capture else { return Ok(false); };
        if let Some(images) = &capture.images { images.get(&output).map_or(Ok(false), ExportedImage::ready) }
        else { self.ready() }
    }
}
