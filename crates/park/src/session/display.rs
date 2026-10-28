use super::*;
use crate::server::{OutputHead, OutputRequest};
mod adaptive_sync;
pub(super) mod color;
pub(super) use adaptive_sync::observe_adaptive_sync;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use std::time::Duration;

type Preferences = BTreeMap<String, crate::commands::OutputPreference>;

#[derive(Default)]
pub(super) struct PreferencePreview(Option<(u64, Preferences)>, BTreeMap<OutputId, Arc<color::Pipeline>>);

impl PreferencePreview {
    fn begin(&mut self, token: u64, preferences: Preferences) {
        self.0 = Some((token, preferences));
        self.1.clear();
    }

    fn synchronize(&mut self, preferences: &mut Preferences, active: Option<u64>, kept: Option<u64>) {
        if self.0.as_ref().is_some_and(|(token, _)| Some(*token) != active) {
            if let Some((token, before)) = self.0.take() && Some(token) != kept { *preferences = before; }
            self.1.clear();
        }
    }

    fn base<'a>(&'a self, current: &'a Preferences, reverting: bool) -> &'a Preferences {
        if reverting { self.0.as_ref().map(|(_, before)| before).unwrap_or(current) } else { current }
    }

    fn topology_before(&self, current: &[OutputHead], proposed: &[OutputHead], kept: Option<u64>) -> Option<&Preferences> {
        let (token, before) = self.0.as_ref()?;
        if Some(*token) == kept { return None; }
        let same = current.len() == proposed.len() && current.iter().all(|head| proposed.iter().any(|other| {
            head.id == other.id && head.name == other.name && head.modes == other.modes
        }));
        (!same).then_some(before)
    }

    fn reverted(&mut self) { self.0 = None; self.1.clear(); }
}

pub(super) fn assign_controllers<T: Copy + Eq>(choices: &[Vec<T>]) -> Vec<Option<T>> {
    fn assign<T: Copy + Eq>(index: usize, choices: &[Vec<T>], owners: &mut Vec<(T, usize)>, visited: &mut Vec<T>) -> bool {
        for controller in &choices[index] {
            if visited.contains(controller) { continue; }
            visited.push(*controller);
            if let Some(position) = owners.iter().position(|(candidate, _)| candidate == controller) {
                let previous = owners[position].1;
                if assign(previous, choices, owners, visited) { owners[position].1 = index; return true; }
            } else { owners.push((*controller, index)); return true; }
        }
        false
    }
    let mut owners = Vec::new();
    for index in 0..choices.len() { assign(index, choices, &mut owners, &mut Vec::new()); }
    (0..choices.len()).map(|index| owners.iter().find_map(|(controller, owner)| (*owner == index).then_some(*controller))).collect()
}

#[derive(Debug)]
struct RecoveryFailure { configuration: String, rollback: String }

impl std::fmt::Display for RecoveryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Output configuration failed: {}; restoring the previous DRM configuration also failed: {}", self.configuration, self.rollback)
    }
}

impl std::error::Error for RecoveryFailure {}

#[derive(Clone)]
pub(super) struct PlannedOutput {
    pub crtc: crtc::Handle,
    pub connector: connector::Handle,
    pub mode: control::Mode,
    pub id: OutputId,
    pub output: smithay::output::Output,
    pub monitor: Monitor,
    pub scale: f64,
    pub rotation: lince_interface::host::mapping::QuarterTurn,
    pub adaptive_sync: bool,
    pub color: lince_interface::host::color::ColorMode,
    pub color_restore: Option<Arc<color::Pipeline>>,
}

pub(super) struct OutputPlan {
    pub screens: Vec<PlannedOutput>,
    pub catalog: Vec<OutputHead>,
    pub size: [u32; 2],
    pub strict: bool,
}

impl OutputPlan {
    pub fn validate(&self) -> io::Result<()> {
        crate::server::validate_output_catalog(&self.catalog)?;
        let mut crtcs = HashSet::new();
        let mut ids = HashSet::new();
        for screen in &self.screens {
            if !crtcs.insert(screen.crtc) || !ids.insert(screen.id) || screen.monitor.id != screen.id
                || screen.output.name() != screen.monitor.name
                || !self.catalog.iter().any(|head| head.id == screen.id && head.current.is_some_and(|index| head.modes[index].size == screen.monitor.size)) {
                return Err(io::Error::other("Physical output ownership is inconsistent"));
            }
        }
        if self.catalog.iter().any(|head| head.current.is_some() && !ids.contains(&head.id)) {
            return Err(io::Error::other("An enabled connector has no display controller"));
        }
        validate_layout(self.screens.iter().map(|screen| (screen.rotation.source_size(screen.monitor.size.map(|size| size as u32)).map(|size| size as i32), screen.monitor.origin, screen.scale)))
    }
}

fn validate_layout(entries: impl IntoIterator<Item = ([i32; 2], [i32; 2], f64)>) -> io::Result<()> {
    let mut pixels = 0_u64;
    let mut logical_pixels = 0_u64;
    for (size, origin, scale) in entries {
        if !scale.is_finite() || !(0.5..=4.0).contains(&scale) || (scale * 120.0 - (scale * 120.0).round()).abs() > 0.000001
            || size.iter().any(|size| !(1..=16384).contains(size)) || origin.iter().any(|position| *position < 0) {
            return Err(io::Error::other("Invalid physical output configuration"));
        }
        let logical = size.map(|size| (f64::from(size) / scale).round() as i32);
        if logical.iter().any(|size| !(1..=16384).contains(size)) || origin.iter().zip(logical)
            .any(|(position, size)| position.checked_add(size).is_none_or(|extent| extent > 16384)) {
            return Err(io::Error::other("Physical output layout exceeds supported bounds"));
        }
        pixels += size[0] as u64 * size[1] as u64;
        logical_pixels += logical[0] as u64 * logical[1] as u64;
    }
    if pixels > 64 * 1024 * 1024 || logical_pixels > 64 * 1024 * 1024 {
        return Err(io::Error::other("Physical output configuration exceeds its memory budget"));
    }
    Ok(())
}

impl Hardware {
    fn saved_output_plan(&self, server: &Server) -> io::Result<OutputPlan> {
        let screens = self.screens.iter().map(|(crtc, screen)| {
            let output = server.output(screen.id).ok_or_else(|| io::Error::other("The current physical output is unavailable"))?;
            let mode = smithay::output::Mode::from(screen.mode);
            Ok(PlannedOutput { crtc: *crtc, connector: screen.connector, mode: screen.mode, id: screen.id,
                monitor: Monitor { id: screen.id, name: output.name(), origin: screen.origin, size: [mode.size.w, mode.size.h] },
                scale: output.current_scale().fractional_scale(), rotation: crate::server::output_rotation(output.current_transform()).ok_or_else(|| io::Error::other("Unsupported current output rotation"))?,
                adaptive_sync: self.catalog.iter().find(|head| head.id == screen.id).is_some_and(|head| head.adaptive_sync),
                color: self.catalog.iter().find(|head| head.id == screen.id).map_or(Default::default(), |head| head.color),
                color_restore: screen.color.prepared.clone(), output })
        }).collect::<io::Result<_>>()?;
        Ok(OutputPlan { screens, catalog: self.catalog.clone(), size: self.size, strict: false })
    }

    fn stage_outputs(&mut self, plan: &OutputPlan, restore: bool) -> io::Result<()> {
        for (crtc, screen) in &mut self.screens {
            let keep = plan.screens.iter().any(|planned| planned.crtc == *crtc && planned.connector == screen.connector);
            if (!keep || restore || plan.screens.iter().any(|planned| planned.crtc == *crtc && planned.mode != screen.mode))
                && let Some(prepared) = screen.prepared.take() { self.retired.push(prepared); }
        }
        self.screens.retain(|crtc, screen| plan.screens.iter().any(|planned| planned.crtc == *crtc && planned.connector == screen.connector));
        for planned in &plan.screens {
            let source = smithay::output::OutputModeSource::Static { size: (planned.monitor.size[0], planned.monitor.size[1]).into(),
                scale: planned.scale.into(), transform: crate::server::output_transform(planned.rotation) };
            if let Some(screen) = self.screens.get_mut(&planned.crtc) {
                screen.output.with_compositor(|compositor| compositor.set_output_mode_source(source));
                let changed_mode = restore || screen.mode != planned.mode;
                if changed_mode && !self.outputs.device().is_atomic() {
                    screen.output.use_mode::<GlesRenderer, TextureRenderElement<GlesTexture>>(planned.mode, &mut self.renderer, &DrmOutputRenderElements::default()).map_err(error)?;
                }
                adaptive_sync::prepare(screen, planned, plan.strict)?;
                color::prepare(&self.outputs, &mut self.renderer, screen, planned, self.native_color, plan.strict)?;
                if changed_mode {
                    screen.power.reconfigure();
                    screen.pending = false;
                    screen.protected = false;
                    screen.output.reset_buffers();
                }
                screen.mode = planned.mode;
                continue;
            }
            let mut planes = self.outputs.device().planes(&planned.crtc).map_err(error)?;
            planes.overlay.clear();
            let output = self.outputs.initialize_output::<GlesRenderer, TextureRenderElement<GlesTexture>>(
                planned.crtc, planned.mode, &[planned.connector], source, Some(planes), &mut self.renderer,
                &DrmOutputRenderElements::default(),
            ).map_err(error)?;
            self.screens.insert(planned.crtc, Screen { protected: false, prepared: None, id: planned.id, connector: planned.connector,
                mode: planned.mode, output, origin: planned.monitor.origin,
                logical_size: planned.rotation.source_size(planned.monitor.size.map(|size| size as u32)).map(|size| (f64::from(size) / planned.scale).round() as i32), pending: false, texture: None,
                adaptive_sync: Default::default(), color: Default::default(), power: Default::default() });
            adaptive_sync::prepare(self.screens.get_mut(&planned.crtc).unwrap(), planned, plan.strict)?;
            color::prepare(&self.outputs, &mut self.renderer, self.screens.get_mut(&planned.crtc).unwrap(), planned, self.native_color, plan.strict)?;
        }
        Ok(())
    }

    fn install_output_plan(&mut self, server: &mut Server, plan: &OutputPlan) -> io::Result<()> {
        plan.validate()?;
        self.stage_outputs(plan, false)?;
        if plan.screens.is_empty() {
            server.disconnect_outputs();
        } else {
            let monitors: Vec<_> = plan.screens.iter().map(|screen| screen.monitor.clone()).collect();
            let scales = plan.screens.iter().map(|screen| (screen.id, screen.scale)).collect();
            let rotations = plan.screens.iter().map(|screen| (screen.id, screen.rotation)).collect();
            let prepared = plan.screens.iter().map(|screen| (screen.id, (screen.output.clone(), smithay::output::Mode::from(screen.mode)))).collect();
            server.configure_prepared_outputs(&monitors, &scales, &rotations, &prepared)?;
        }
        server.output_catalog(plan.catalog.clone())?;
        for planned in &plan.screens {
            let mode = smithay::output::Mode::from(planned.mode);
            eprintln!("Park output {}: {}x{} at {} mHz, scale {}, position {},{}", planned.monitor.name,
                mode.size.w, mode.size.h, mode.refresh, planned.scale, planned.monitor.origin[0], planned.monitor.origin[1]);
            if let Some(screen) = self.screens.get_mut(&planned.crtc) {
                screen.output.with_compositor(|compositor| compositor.set_output_mode_source((&planned.output).into()));
                screen.origin = planned.monitor.origin;
                screen.logical_size = planned.rotation.source_size(planned.monitor.size.map(|size| size as u32)).map(|size| (f64::from(size) / planned.scale).round() as i32);
            }
            if let Some(mode) = plan.catalog.iter().find(|head| head.id == planned.id).and_then(|head| head.modes.iter().find(|mode| mode.preferred)) {
                planned.output.set_preferred(smithay::output::Mode { size: (mode.size[0], mode.size[1]).into(), refresh: mode.refresh_mhz });
            }
        }
        self.catalog = plan.catalog.clone();
        self.publish_adaptive_sync(server);
        self.publish_color(server);
        self.publish_power(server);
        self.size = plan.size;
        Ok(())
    }

    fn replace_output_plan(&mut self, server: &mut Server, plan: OutputPlan) -> io::Result<()> {
        let previous = self.saved_output_plan(server)?;
        if let Err(failure) = self.install_output_plan(server, &plan) {
            self.stage_outputs(&previous, true).and_then(|_| self.install_output_plan(server, &previous))
                .map_err(|rollback| io::Error::other(RecoveryFailure { configuration: failure.to_string(), rollback: rollback.to_string() }))?;
            return Err(failure);
        }
        Ok(())
    }

    pub(super) fn rescan(&mut self, server: &mut Server) -> io::Result<()> {
        let preferences = self.preferences.clone();
        let mut plan = self.output_plan(server, &preferences, false)?;
        if let Some(before) = self.preview_preferences.topology_before(&self.catalog, &plan.catalog, server.kept_display_token()).cloned() {
            self.preferences = before.clone();
            plan = self.output_plan(server, &before, false)?;
        }
        self.replace_output_plan(server, plan)
    }

    fn request_preferences(&self, request: &OutputRequest, reverting: bool) -> Option<Preferences> {
        if request.heads.len() != self.catalog.len() { return None; }
        let mut preferences = self.preview_preferences.base(&self.preferences, reverting).clone();
        for head in &self.catalog {
            let settings = request.heads.get(&head.id)?;
            let preference = preferences.entry(head.name.clone()).or_default();
            preference.enabled = settings.is_some();
            if let Some(settings) = settings {
                if !head.modes.contains(&settings.mode) { return None; }
                preference.scale = settings.scale;
                preference.rotation = settings.rotation;
                preference.adaptive_sync = settings.adaptive_sync;
                preference.color = settings.color;
                preference.position = Some(settings.position);
                preference.mode = Some(settings.mode.size.map(|size| size as u32));
                preference.refresh_mhz = Some(settings.mode.refresh_mhz);
            }
        }
        Some(preferences)
    }

    pub(super) fn configuration_timeout(&self) -> Option<Duration> {
        self.active.then(|| self.configurations.front().map(|(_, deadline)| deadline.saturating_duration_since(Instant::now()))).flatten()
    }

    pub(super) fn resume_output_requests(&mut self, server: &Server) {
        for (request, deadline) in &mut self.configurations {
            if server.output_configuration_recovery(request) { *deadline = Instant::now() + Duration::from_secs(2); }
        }
    }

    pub(super) fn configure_requests(&mut self, desktop: &mut Desktop) -> io::Result<()> {
        self.preview_preferences.synchronize(&mut self.preferences, desktop.server.display_preview_token(), desktop.server.kept_display_token());
        for request in desktop.server.output_requests() {
            if desktop.server.output_configuration_recovery(&request) {
                if self.configurations.len() >= 8 && let Some((superseded, _)) = self.configurations.pop_back() {
                    desktop.server.output_configuration_finished(superseded.token, false);
                }
                self.configurations.push_front((request, Instant::now() + Duration::from_secs(2)));
            }
            else if self.configurations.len() >= 8 { desktop.server.output_configuration_finished(request.token, false); }
            else { self.configurations.push_back((request, Instant::now() + Duration::from_secs(2))); }
        }
        loop {
            let Some((request, deadline)) = self.configurations.front() else { break };
            if !self.active && desktop.server.output_configuration_recovery(request) { break; }
            let current = self.active && desktop.server.output_configuration_current(request);
            let busy = self.screens.values().any(|screen| screen.pending || screen.prepared.is_some());
            if current && busy && *deadline > Instant::now() { break; }
            let (request, deadline) = self.configurations.pop_front().unwrap();
            if !current || busy || deadline <= Instant::now() { desktop.server.output_configuration_finished(request.token, false); continue; }
            let before = desktop.server.output_configuration_preview(&request).then(|| self.preferences.clone());
            let before_colors = before.as_ref().map(|_| self.screens.values().filter_map(|screen|
                screen.color.prepared.as_ref().map(|pipeline| (screen.id, pipeline.clone()))).collect::<BTreeMap<_, _>>());
            let reverting = desktop.server.output_configuration_recovery(&request);
            let result = (|| {
                let preferences = self.request_preferences(&request, reverting).ok_or_else(|| io::Error::other("The output configuration references unavailable connector modes"))?;
                let mut plan = self.output_plan(&desktop.server, &preferences, true)?;
                if reverting {
                    plan.strict = false;
                    for planned in &mut plan.screens {
                        planned.color_restore = self.preview_preferences.1.get(&planned.id)
                            .filter(|pipeline| pipeline.restores(planned.mode, planned.connector)).cloned();
                    }
                }
                if plan.catalog.len() != request.heads.len() || plan.catalog.iter().any(|head| !request.heads.contains_key(&head.id)) {
                    return Err(io::Error::other("The connector topology changed before the output request"));
                }
                if plan.screens.is_empty() { return Err(io::Error::other("Keep at least one output enabled")); }
                if !reverting { self.test_adaptive_sync(&plan)?; self.test_color(&plan)?; }
                if !request.test {
                    self.replace_output_plan(&mut desktop.server, plan)?;
                    self.preferences = preferences;
                    if let Some(before) = before {
                        self.preview_preferences.begin(request.token, before);
                        self.preview_preferences.1 = before_colors.unwrap_or_default();
                    }
                    if reverting { self.preview_preferences.reverted(); }
                }
                Ok(())
            })();
            let accepted = result.is_ok();
            desktop.server.output_configuration_finished(request.token, accepted);
            if !request.test {
                if accepted { desktop.size = self.size; desktop.invalidate_outputs()?; }
                desktop.redraw = true;
            }
            if let Err(failure) = result {
                if failure.get_ref().is_some_and(|error| error.downcast_ref::<RecoveryFailure>().is_some()) { return Err(failure); }
                eprintln!("Park output configuration: {failure}");
                desktop.send(Input::Feedback(format!("Could not apply the output configuration: {failure}")))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preferences() -> Preferences {
        BTreeMap::from([
            ("DP-1".into(), crate::commands::OutputPreference { scale: 1.25, mode: Some([1920, 1080]), ..Default::default() }),
            ("DP-2".into(), crate::commands::OutputPreference { enabled: false, scale: 1.5, position: Some([1920, 0]), mode: Some([1680, 1050]), ..Default::default() }),
        ])
    }

    #[test]
    fn rollback_uses_original_profiles_even_when_the_changed_monitor_is_disconnected() {
        let before = preferences();
        let mut current = before.clone();
        current.get_mut("DP-1").unwrap().scale = 2.0;
        current.get_mut("DP-2").unwrap().enabled = true;
        current.get_mut("DP-2").unwrap().mode = Some([2560, 1440]);
        let mut preview = PreferencePreview::default();
        preview.begin(7, before.clone());
        assert_eq!(preview.base(&current, false), &current);
        assert_eq!(preview.base(&current, true), &before);
        let mut restored = preview.base(&current, true).clone();
        restored.insert("DP-3".into(), Default::default());
        preview.reverted();
        preview.synchronize(&mut restored, None, None);
        assert_eq!(restored["DP-1"], before["DP-1"]);
        assert_eq!(restored["DP-2"], before["DP-2"]);
        assert!(restored.contains_key("DP-3"));
    }

    #[test]
    fn only_a_confirmed_preview_can_replace_the_pre_preview_profiles() {
        let before = preferences();
        let mut current = before.clone();
        current.get_mut("DP-1").unwrap().scale = 2.0;
        let mut preview = PreferencePreview::default();
        preview.begin(7, before.clone());
        preview.synchronize(&mut current, Some(7), None);
        assert_eq!(current["DP-1"].scale, 2.0);
        preview.synchronize(&mut current, None, None);
        assert_eq!(current, before);
        current.get_mut("DP-1").unwrap().scale = 2.0;
        preview.begin(8, before);
        preview.synchronize(&mut current, None, Some(8));
        assert_eq!(current["DP-1"].scale, 2.0);
        assert!(preview.0.is_none());
    }

    #[test]
    fn only_connector_or_mode_catalogue_changes_cancel_an_unconfirmed_physical_profile() {
        let before = preferences();
        let mut preview = PreferencePreview::default();
        preview.begin(7, before.clone());
        let heads: Vec<_> = [1, 2].into_iter().map(|id| OutputHead { color: Default::default(), adaptive_sync: Default::default(), rotation: Default::default(),
            id: OutputId(id), name: format!("DP-{id}"), modes: vec![crate::server::OutputMode { size: [1920, 1080], refresh_mhz: 60000, preferred: true }],
            current: Some(0), position: [0, 0], scale: 1.0,
        }).collect();
        let mut own_update = heads.clone();
        own_update[0].scale = 2.0;
        own_update[1].position = [960, 0];
        own_update[1].current = None;
        assert!(preview.topology_before(&heads, &own_update, None).is_none());
        assert_eq!(preview.topology_before(&heads, &heads[..1], None), Some(&before));
        assert_eq!(preview.topology_before(&heads, &[], None), Some(&before));
        assert!(preview.topology_before(&heads, &[], Some(7)).is_none());
        let mut modes_changed = heads.clone();
        modes_changed[0].modes[0].refresh_mhz = 75000;
        assert_eq!(preview.topology_before(&heads, &modes_changed, None), Some(&before));
    }

    #[test]
    fn controller_assignment_finds_alternate_routes_and_preserves_available_preferences() {
        assert_eq!(assign_controllers(&[vec![1, 2], vec![1]]), vec![Some(2), Some(1)]);
        assert_eq!(assign_controllers(&[vec![1, 2], vec![2, 1]]), vec![Some(1), Some(2)]);
        assert_eq!(assign_controllers(&[vec![1], vec![1], vec![]]), vec![Some(1), None, None]);
        assert_eq!(assign_controllers::<u32>(&[]), Vec::new());
    }

    #[test]
    fn all_small_controller_graphs_reach_the_maximum_feasible_output_count() {
        for mask in 0_u32..512 {
            let choices: Vec<Vec<u32>> = (0..3).map(|output| (0..3).filter(|controller| mask & (1 << (output * 3 + controller)) != 0).collect()).collect();
            let actual = assign_controllers(&choices);
            let mut controllers = HashSet::new();
            for (output, controller) in actual.iter().enumerate() {
                if let Some(controller) = controller { assert!(choices[output].contains(controller)); assert!(controllers.insert(*controller)); }
            }
            let mut maximum = 0;
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        let selected = [a, b, c];
                        let mut unique = HashSet::new();
                        if selected.iter().enumerate().all(|(output, controller)| *controller == 3 || choices[output].contains(controller) && unique.insert(*controller)) {
                            maximum = maximum.max(unique.len());
                        }
                    }
                }
            }
            assert_eq!(controllers.len(), maximum, "controller graph {mask}");
        }
    }

    #[test]
    fn physical_layouts_validate_scaled_bounds_and_both_memory_budgets() {
        assert!(validate_layout([([3840, 2160], [0, 0], 1.5), ([1920, 1080], [2560, 0], 1.0)]).is_ok());
        for (size, origin, scale) in [([1920, 1080], [-1, 0], 1.0), ([1920, 1080], [16000, 0], 1.0),
            ([16384, 100], [0, 0], 0.5), ([1920, 1080], [0, 0], f64::NAN), ([1920, 1080], [0, 0], 1.001)] {
            assert!(validate_layout([(size, origin, scale)]).is_err());
        }
        assert!(validate_layout([([8192, 8192], [0, 0], 1.0), ([1, 1], [0, 0], 1.0)]).is_err());
        assert!(validate_layout([([8192, 4096], [0, 0], 0.5)]).is_err());
        assert!(validate_layout([([16384, 8192], [0, 0], 4.0)]).is_err());
    }
}
