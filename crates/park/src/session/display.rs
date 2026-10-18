use super::*;
use crate::server::{OutputHead, OutputRequest};
use std::time::Duration;

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
}

pub(super) struct OutputPlan {
    pub screens: Vec<PlannedOutput>,
    pub catalog: Vec<OutputHead>,
    pub size: [u32; 2],
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
        validate_layout(self.screens.iter().map(|screen| (screen.monitor.size, screen.monitor.origin, screen.scale)))
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
                scale: output.current_scale().fractional_scale(), output })
        }).collect::<io::Result<_>>()?;
        Ok(OutputPlan { screens, catalog: self.catalog.clone(), size: self.size })
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
                scale: planned.scale.into(), transform: Transform::Normal };
            if let Some(screen) = self.screens.get_mut(&planned.crtc) {
                screen.output.with_compositor(|compositor| compositor.set_output_mode_source(source));
                if restore || screen.mode != planned.mode {
                    screen.output.use_mode::<GlesRenderer, TextureRenderElement<GlesTexture>>(planned.mode, &mut self.renderer, &DrmOutputRenderElements::default()).map_err(error)?;
                    screen.mode = planned.mode;
                    screen.pending = false;
                    screen.protected = false;
                    screen.output.reset_buffers();
                }
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
                logical_size: planned.monitor.size.map(|size| (f64::from(size) / planned.scale).round() as i32), pending: false });
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
            let prepared = plan.screens.iter().map(|screen| (screen.id, (screen.output.clone(), smithay::output::Mode::from(screen.mode)))).collect();
            server.configure_prepared_outputs(&monitors, &scales, &prepared)?;
        }
        server.output_catalog(plan.catalog.clone())?;
        for planned in &plan.screens {
            let mode = smithay::output::Mode::from(planned.mode);
            eprintln!("Park output {}: {}x{} at {} mHz, scale {}, position {},{}", planned.monitor.name,
                mode.size.w, mode.size.h, mode.refresh, planned.scale, planned.monitor.origin[0], planned.monitor.origin[1]);
            if let Some(screen) = self.screens.get_mut(&planned.crtc) {
                screen.output.with_compositor(|compositor| compositor.set_output_mode_source((&planned.output).into()));
                screen.origin = planned.monitor.origin;
                screen.logical_size = planned.monitor.size.map(|size| (f64::from(size) / planned.scale).round() as i32);
            }
            if let Some(mode) = plan.catalog.iter().find(|head| head.id == planned.id).and_then(|head| head.modes.iter().find(|mode| mode.preferred)) {
                planned.output.set_preferred(smithay::output::Mode { size: (mode.size[0], mode.size[1]).into(), refresh: mode.refresh_mhz });
            }
        }
        self.catalog = plan.catalog.clone();
        if self.size != plan.size { self.texture = None; self.size = plan.size; }
        Ok(())
    }

    fn replace_output_plan(&mut self, server: &mut Server, plan: OutputPlan) -> io::Result<()> {
        let previous = self.saved_output_plan(server)?;
        if let Err(failure) = self.install_output_plan(server, &plan) {
            self.stage_outputs(&previous, true).map_err(|rollback| io::Error::other(RecoveryFailure { configuration: failure.to_string(), rollback: rollback.to_string() }))?;
            for planned in &previous.screens {
                if let Some(screen) = self.screens.get_mut(&planned.crtc) {
                    screen.output.with_compositor(|compositor| compositor.set_output_mode_source((&planned.output).into()));
                }
            }
            return Err(failure);
        }
        Ok(())
    }

    pub(super) fn rescan(&mut self, server: &mut Server) -> io::Result<()> {
        let preferences = self.preferences.clone();
        let plan = self.output_plan(server, &preferences, false)?;
        self.replace_output_plan(server, plan)
    }

    fn request_preferences(&self, request: &OutputRequest) -> Option<BTreeMap<String, crate::commands::OutputPreference>> {
        if request.heads.len() != self.catalog.len() { return None; }
        let mut preferences = self.preferences.clone();
        for head in &self.catalog {
            let settings = request.heads.get(&head.id)?;
            let preference = preferences.entry(head.name.clone()).or_default();
            preference.enabled = settings.is_some();
            if let Some(settings) = settings {
                if !head.modes.contains(&settings.mode) { return None; }
                preference.scale = settings.scale;
                preference.position = Some(settings.position);
                preference.mode = Some(settings.mode.size.map(|size| size as u32));
                preference.refresh_mhz = Some(settings.mode.refresh_mhz);
            }
        }
        Some(preferences)
    }

    pub(super) fn configuration_timeout(&self) -> Option<Duration> {
        self.configurations.front().map(|(_, deadline)| deadline.saturating_duration_since(Instant::now()))
    }

    pub(super) fn configure_requests(&mut self, desktop: &mut Desktop) -> io::Result<()> {
        for request in desktop.server.output_requests() {
            if self.configurations.len() >= 8 { desktop.server.output_configuration_finished(request.token, false); }
            else { self.configurations.push_back((request, Instant::now() + Duration::from_secs(2))); }
        }
        loop {
            let Some((request, deadline)) = self.configurations.front() else { break };
            let current = self.active && desktop.server.output_configuration_current(request);
            let busy = self.screens.values().any(|screen| screen.pending || screen.prepared.is_some());
            if current && busy && *deadline > Instant::now() { break; }
            let (request, deadline) = self.configurations.pop_front().unwrap();
            if !current || busy || deadline <= Instant::now() { desktop.server.output_configuration_finished(request.token, false); continue; }
            let result = (|| {
                let preferences = self.request_preferences(&request).ok_or_else(|| io::Error::other("The output configuration references unavailable connector modes"))?;
                let plan = self.output_plan(&desktop.server, &preferences, true)?;
                if plan.catalog.len() != request.heads.len() || plan.catalog.iter().any(|head| !request.heads.contains_key(&head.id)) {
                    return Err(io::Error::other("The connector topology changed before the output request"));
                }
                if plan.screens.is_empty() { return Err(io::Error::other("Keep at least one output enabled")); }
                if !request.test {
                    self.replace_output_plan(&mut desktop.server, plan)?;
                    self.preferences = preferences;
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
