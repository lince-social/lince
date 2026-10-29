use super::*;
use lince_interface::{
    host::{DesktopAction, OutputId, OutputViewport},
    layout::outputs::{Assignments, Preference},
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    io,
    path::Path,
};

mod suspension;
pub(super) use suspension::install as install_suspension;

#[derive(Debug)]
pub(super) enum ActionFailure { Refused(String), Backend(ProbeFailure) }

impl From<ProbeFailure> for ActionFailure {
    fn from(error: ProbeFailure) -> Self { Self::Backend(error) }
}

impl std::fmt::Display for ActionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::Refused(reason) => formatter.write_str(reason), Self::Backend(error) => std::fmt::Display::fmt(error, formatter) }
    }
}

#[derive(Clone)]
pub struct Output {
    pub name: String,
    pub viewport: OutputViewport,
}

fn default_output_order(a: &Output, b: &Output) -> std::cmp::Ordering {
    let builtin = |name: &str| name.starts_with("eDP-") || name.starts_with("LVDS-") || name.starts_with("DSI-");
    builtin(&b.name).cmp(&builtin(&a.name))
        .then_with(|| a.viewport.bounds.origin()[0].total_cmp(&b.viewport.bounds.origin()[0]))
        .then_with(|| a.name.cmp(&b.name))
}

pub(super) struct Target {
    pub image: Handle<Image>,
    pub size: [u32; 2],
    pub origin: [f64; 2],
    pub raster: lince_interface::host::raster::RasterMapping,
    pub fullscreen: bool,
}

#[derive(Default)]
pub(super) struct State {
    pub assignments: Assignments,
    pub outputs: Vec<Output>,
    pub targets: HashMap<OutputId, Target>,
    path: Option<PathBuf>,
    saved: Vec<Preference>,
}

impl Presentation {
    pub(crate) fn suspended(&self) -> bool {
        self.app.world().resource::<suspension::Suspension>().active
    }

    pub fn activate_application(
        &mut self,
        surface: lince_interface::host::SurfaceId,
    ) -> Result<(), ProbeFailure> {
        self.activate_application_on(surface, None)
    }

    pub(super) fn activate_application_on(&mut self, surface: lince_interface::host::SurfaceId, output: Option<OutputId>) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        native::laboratory::close(self.app.world_mut());
        let Some(window) = self.windows.get(&surface) else {
            return Ok(());
        };
        let Some(workspace) = self
            .app
            .world()
            .get::<native::workspace::WorkspaceMember>(window.entity)
            .map(|member| member.0)
        else {
            return Ok(());
        };
        let root = self.canvas_root()?;
        native::layout::desktop::manipulation::pointer::cancel(self.app.world_mut());
        native::layout::desktop::overview::dismiss(self.app.world_mut());
        native::layout::desktop::recent::dismiss(self.app.world_mut());
        let prepared = native::layout::desktop::transfers::prepare_identity(self.app.world(), root, workspace);
        self.apply_workspace_transfer(prepared, output, true)?;
        if !self.app.world().get::<native::workspace::Workspaces>(root).is_some_and(|spaces| spaces.active == workspace) { return Ok(()); }
        if let Some(entity) = self.windows.get(&surface).map(|window| window.entity)
            && let Err(reason) = native::layout::desktop::activate_compact_member(self.app.world_mut(), root, entity)
        {
            native::notifications::report(self.app.world_mut(), "Window activation", reason);
            return Ok(());
        }
        self.focus_application(surface);
        if let Some(entity) = self.windows.get(&surface).map(|window| window.entity) {
            native::layout::desktop::focus::request_warp(self.app.world_mut(), root, entity);
        }
        Ok(())
    }

    pub fn output_preferences(&mut self, path: PathBuf) -> io::Result<()> {
        let preferences: Vec<Preference> = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => {
                serde_json::from_slice(&bytes).map_err(io::Error::other)?
            }
            Ok(_) => {
                return Err(io::Error::other(
                    "Park output preferences exceed their limit",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        self.output_state.assignments = Assignments::restore(preferences.clone())
            .map_err(|_| io::Error::other("invalid Park output preferences"))?;
        self.output_state.saved = preferences;
        self.output_state.path = Some(path);
        Ok(())
    }

    pub fn outputs(&mut self, outputs: Vec<Output>) -> Result<(), ProbeFailure> {
        let initial = self.output_state.assignments.outputs().is_empty();
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let pixels: u64 = outputs
            .iter()
            .map(|output| {
                u64::from(output.viewport.physical_size[0])
                    * u64::from(output.viewport.physical_size[1])
            })
            .fold(0, u64::saturating_add);
        if pixels > 64 * 1024 * 1024
            || outputs.len() > 16
            || outputs.iter().any(|output| {
                output.name.is_empty()
                    || output.name.len() > 512
                    || !ids.insert(output.viewport.id)
                    || !names.insert(&output.name)
                    || validate_size(output.viewport.physical_size).is_err()
                    || !output.viewport.valid_raster()
            })
        {
            return Err(input_failure("invalid Park output configuration"));
        }
        if self.output_state.outputs.len() != outputs.len()
            || self.output_state.outputs.iter().any(|current| {
                !outputs.iter().any(|output| output.name == current.name && output.viewport == current.viewport)
            })
        {
            self.cancel_gesture();
            native::layout::desktop::cancel_scratch_interactions(self.app.world_mut());
            if let Some(bridge) = &mut self.accessibility { bridge.invalidate(); }
        }
        if self.output_state.outputs.len() != outputs.len()
            || self.output_state.outputs.iter().any(|current| {
                !outputs.iter().any(|output| {
                    output.viewport.id == current.viewport.id && output.name == current.name
                })
            })
        {
            native::laboratory::close(self.app.world_mut());
        }
        let root = self.canvas_root()?;
        if outputs.is_empty() {
            native::shader_effects::set_compositor_presentation_active(self.app.world_mut(), false);
            if !self.suspended() { self.focus(false)?; }
            self.app.world_mut().remove_resource::<native::output_views::OutputViews>();
            self.app.world_mut().remove_resource::<native::output_views::OutputNames>();
            let removed: Vec<_> = self.output_state.assignments.outputs().iter().map(|output| output.id).collect();
            for id in removed { self.output_state.assignments.disconnect(id); }
            self.output_state.outputs.clear();
            self.app.world_mut().resource_mut::<suspension::Suspension>().active = true;
            self.save_output_preferences().map_err(|error| input_failure(&error.to_string()))?;
            return Ok(());
        }
        self.app.world_mut().resource_mut::<suspension::Suspension>().active = false;
        native::shader_effects::set_compositor_presentation_active(self.app.world_mut(), true);
        self.synchronize_workspaces(root)?;
        let active = self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        while self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .len()
            < outputs.len()
        {
            let count = self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .len();
            native::workspace::create(self.app.world_mut(), root);
            if self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .len()
                == count
            {
                return Err(input_failure("could not create an output workspace"));
            }
        }
        native::workspace::switch_output(self.app.world_mut(), root, active);
        let workspaces: BTreeSet<_> = self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .filter(|space| !native::laboratory::temporary(self.app.world(), root, space.id))
            .map(|space| space.id)
            .collect();
        self.output_state.assignments.retain_workspaces(&workspaces);
        for workspace in workspaces {
            if !self
                .output_state
                .assignments
                .preferences()
                .iter()
                .any(|preference| preference.workspace == workspace)
            {
                self.output_state
                    .assignments
                    .register_workspace(workspace)
                    .map_err(|_| input_failure("could not assign a workspace"))?;
            }
        }
        let stale: Vec<_> = self
            .output_state
            .assignments
            .outputs()
            .iter()
            .filter(|output| {
                !outputs.iter().any(|candidate| {
                    candidate.viewport.id == output.id && candidate.name == output.name
                })
            })
            .map(|output| output.id)
            .collect();
        for id in stale {
            self.output_state.assignments.disconnect(id);
        }
        for output in &outputs {
            if self
                .output_state
                .assignments
                .outputs()
                .iter()
                .any(|candidate| candidate.id == output.viewport.id)
            {
                self.output_state
                    .assignments
                    .resize(output.viewport.id, output.viewport.bounds)
                    .map_err(|_| input_failure("could not resize an output"))?;
            } else {
                self.output_state
                    .assignments
                    .connect(
                        output.viewport.id,
                        output.name.clone(),
                        output.viewport.bounds,
                    )
                    .map_err(|_| input_failure("could not connect an output"))?;
            }
        }
        if initial {
            self.output_state.assignments.restore_initial(active)
                .map_err(|_| input_failure("could not restore the initial output workspace"))?;
        }
        let stale: Vec<_> = self
            .output_state
            .targets
            .keys()
            .filter(|id| !ids.contains(id))
            .copied()
            .collect();
        for id in stale {
            if let Some(target) = self.output_state.targets.remove(&id) {
                self.app
                    .world_mut()
                    .resource_mut::<Assets<Image>>()
                    .remove(target.image.id());
            }
        }
        self.output_state.outputs = outputs;
        if native::laboratory::scope(self.app.world())
            .is_some_and(|(_, output, _, _)| !ids.contains(&output))
        {
            native::laboratory::close(self.app.world_mut());
        }
        self.select_output_workspace()?;
        self.refresh_outputs()
    }

    pub fn focus_output(&mut self, output: OutputId) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        if self.output_state.assignments.focused() == Some(output) {
            return Ok(());
        }
        self.application_focus = None;
        self.focus(false)?;
        self.output_state
            .assignments
            .focus(output)
            .map_err(|_| input_failure("the selected output is unavailable"))?;
        self.select_output_workspace()?;
        self.refresh_outputs()?;
        self.focus(true)
    }

    pub fn move_workspace_to_output(
        &mut self,
        workspace: u64,
        output: OutputId,
    ) -> Result<(), ProbeFailure> {
        self.output_state
            .assignments
            .move_workspace(workspace, output)
            .map_err(|_| input_failure("could not move the workspace"))?;
        self.select_output_workspace()?;
        self.refresh_outputs()
    }

    pub(super) fn apply_workspace_transfer(
        &mut self,
        prepared: Result<native::layout::desktop::transfers::Prepared, String>,
        output: Option<OutputId>,
        dismiss_laboratory: bool,
    ) -> Result<(), ProbeFailure> {
        match self.try_apply_workspace_transfer(prepared, output, dismiss_laboratory) {
            Ok(()) => Ok(()),
            Err(ActionFailure::Refused(message)) => { native::notifications::report(self.app.world_mut(), "Park", &message); Ok(()) }
            Err(ActionFailure::Backend(error)) => Err(error),
        }
    }

    pub(super) fn try_apply_workspace_transfer(
        &mut self,
        prepared: Result<native::layout::desktop::transfers::Prepared, String>,
        output: Option<OutputId>,
        dismiss_laboratory: bool,
    ) -> Result<(), ActionFailure> {
        let prepared = prepared.map_err(ActionFailure::Refused)?;
        let root = self.canvas_root()?;
        let destination = prepared.destination();
        let output = output.or_else(|| self.output_state.assignments.outputs().iter()
            .find(|output| output.active == destination).map(|output| output.id))
            .or(self.output_state.assignments.focused());
        let mut assignments = self.output_state.assignments.clone();
        let registered = assignments.preferences().iter().any(|preference| preference.workspace == destination)
            || assignments.register_workspace(destination).is_ok();
        let valid = registered && output.is_some_and(|output| {
            assignments.select(output, destination).is_ok() && assignments.focus(output).is_ok()
        });
        if !registered || !valid && !self.output_state.outputs.is_empty() {
            return Err(ActionFailure::Refused("The destination monitor is unavailable.".into()));
        }
        self.input_window()?;
        let unchanged = self.app.world().get::<native::workspace::Workspaces>(root)
            .is_some_and(|spaces| spaces.active == destination)
            && assignments.focused() == self.output_state.assignments.focused();
        let selection = prepared.commit(self.app.world_mut()).map_err(ActionFailure::Refused)?;
        if dismiss_laboratory || native::laboratory::scope(self.app.world()).is_some_and(|(owner, current, workspace, _)| {
            owner == root && Some(current) == output && workspace != destination
        }) {
            native::laboratory::close(self.app.world_mut());
        }
        if unchanged { return Ok(()); }
        self.application_focus = None;
        let modal = native::layout::desktop::recent::active(self.app.world());
        if !modal { self.focus(false)?; }
        if self.output_state.outputs.is_empty() {
            native::workspace::switch_output(self.app.world_mut(), root, destination);
        } else {
            self.output_state.assignments = assignments;
            self.select_output_workspace()?;
            self.refresh_outputs()?;
        }
        if !modal { self.focus(true)?; }
        if let Some(selection) = selection { selection.activate(self.app.world_mut()); }
        Ok(())
    }

    pub(super) fn output_action(&mut self, action: DesktopAction) -> Result<bool, ProbeFailure> {
        match self.try_output_action(action) {
            Ok(handled) => Ok(handled),
            Err(ActionFailure::Refused(message)) => { native::notifications::report(self.app.world_mut(), "Park", &message); Ok(true) }
            Err(ActionFailure::Backend(error)) => Err(error),
        }
    }

    pub(super) fn try_output_action(&mut self, action: DesktopAction) -> Result<bool, ActionFailure> {
        if self.output_state.outputs.is_empty()
            && matches!(action, DesktopAction::FocusOutput(_) | DesktopAction::MoveToOutput(_) | DesktopAction::MoveRowToOutput(_) | DesktopAction::MoveWorkspaceToOutput(_))
        {
            return Err(ActionFailure::Refused("No monitor is available for this command.".into()));
        }
        let root = self.canvas_root()?;
        let previous = matches!(action, DesktopAction::Workspace(_) | DesktopAction::PreviousWorkspace | DesktopAction::NextWorkspace | DesktopAction::MoveToWorkspace(_))
            .then(|| native::workspace::navigation::capture(self.app.world(), root)).flatten();
        match action {
            DesktopAction::PreviousWorkspace | DesktopAction::NextWorkspace => {
                let offset = if action == DesktopAction::PreviousWorkspace { -1 } else { 1 };
                let prepared = native::workspace::navigation::destination(self.app.world(), root, offset)
                    .map_err(str::to_owned).and_then(|destination| {
                        native::layout::desktop::transfers::prepare_identity(self.app.world(), root, destination)
                    });
                self.try_apply_workspace_transfer(prepared, None, true)?;
            }
            DesktopAction::MoveWorkspaceEarlier | DesktopAction::MoveWorkspaceLater => {
                let offset = if action == DesktopAction::MoveWorkspaceEarlier { -1 } else { 1 };
                native::workspace::navigation::reorder(self.app.world_mut(), root, offset).map_err(|message| ActionFailure::Refused(message.into()))?;
            }
            DesktopAction::Workspace(index) => {
                let prepared = native::layout::desktop::transfers::prepare_selection(self.app.world(), root, index);
                self.try_apply_workspace_transfer(prepared, None, true)?;
            }
            DesktopAction::MoveToWorkspace(index) => {
                let prepared = native::layout::desktop::transfers::prepare_numbered_move(
                    self.app.world_mut(), root, index, lince_interface::layout::desktop::TransferKind::Item,
                );
                self.try_apply_workspace_transfer(prepared, None, false)?;
            }
            DesktopAction::FocusOutput(direction) => {
                if let Some(output) = self.output_state.assignments.directional(direction) {
                    self.focus_output(output)?;
                }
            }
            DesktopAction::MoveWorkspaceToOutput(direction) => {
                native::laboratory::close(self.app.world_mut());
                if let Some(output) = self.output_state.assignments.directional(direction) {
                    let workspace = self
                        .app
                        .world()
                        .get::<native::workspace::Workspaces>(root)
                        .unwrap()
                        .active;
                    self.move_workspace_to_output(workspace, output)?;
                }
            }
            DesktopAction::MoveToOutput(direction) | DesktopAction::MoveRowToOutput(direction) => {
                if let Some(output) = self.output_state.assignments.directional(direction) {
                    let workspace = self
                        .output_state
                        .assignments
                        .outputs()
                        .iter()
                        .find(|candidate| candidate.id == output)
                        .unwrap()
                        .active;
                    let kind = if matches!(action, DesktopAction::MoveRowToOutput(_)) {
                        lince_interface::layout::desktop::TransferKind::Row
                    } else { lince_interface::layout::desktop::TransferKind::Item };
                    let prepared = native::layout::desktop::transfers::prepare_move(
                        self.app.world_mut(), root, workspace, kind,
                    );
                    self.try_apply_workspace_transfer(prepared, Some(output), false)?;
                }
            }
            _ => return Ok(false),
        }
        if let Some(previous) = previous {
            previous.remember(self.app.world_mut());
        }
        Ok(true)
    }

    pub(super) fn select_output_workspace(&mut self) -> Result<(), ProbeFailure> {
        let root = self.canvas_root()?;
        let output = self
            .output_state
            .assignments
            .focused()
            .ok_or_else(|| input_failure("no focused output"))?;
        let workspace = self
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|candidate| candidate.id == output)
            .unwrap()
            .active;
        let workspace = native::laboratory::scope(self.app.world())
            .filter(|(owner, current, _, _)| *owner == root && *current == output)
            .map_or(workspace, |(_, _, workspace, _)| workspace);
        native::workspace::switch_output(self.app.world_mut(), root, workspace);
        Ok(())
    }

    pub(super) fn refresh_outputs(&mut self) -> Result<(), ProbeFailure> {
        if self.output_state.outputs.is_empty() {
            return Ok(());
        }
        let root = self.canvas_root()?;
        if self.output_state.targets.is_empty() {
            self.app
                .world_mut()
                .resource_mut::<Assets<Image>>()
                .remove(self.target.id());
        }
        self.synchronize_workspaces(root)?;
        let active = self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let focused = self.output_state.assignments.focused().unwrap();
        let scope = native::laboratory::scope(self.app.world());
        if self
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|candidate| candidate.id == focused)
            .unwrap()
            .active
            != active
            && scope.is_none()
        {
            self.output_state
                .assignments
                .select(focused, active)
                .map_err(|_| input_failure("could not select the active workspace"))?;
            self.select_output_workspace()?;
        }
        let focused = self.output_state.assignments.focused().unwrap();
        let mut locals = Vec::new();
        let mut outputs: Vec<_> = self.output_state.outputs.iter().collect();
        outputs.sort_by(|a, b| default_output_order(a, b));
        for output in outputs {
            let assignment = self
                .output_state
                .assignments
                .outputs()
                .iter()
                .find(|candidate| candidate.id == output.viewport.id)
                .unwrap();
            let workspace = scope
                .filter(|(_, output, _, _)| *output == assignment.id)
                .map_or(assignment.active, |(_, _, workspace, _)| workspace);
            let fullscreen = self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .find(|space| space.id == workspace)
                .is_some_and(|space| space.desktop.fullscreen().is_some());
            let region = if fullscreen {
                Some(output.viewport.bounds)
            } else {
                output.viewport.usable
            };
            let (pixel_origin, size) = region
                .and_then(|region| output.viewport.physical_region(region))
                .unwrap_or(([0; 2], [1; 2]));
            let logical_size = region.map_or([1.0; 2], |region| region.size());
            validate_size(size)?;
            let origin = region.map_or(output.viewport.bounds.origin(), |region| region.origin());
            let raster = lince_interface::host::raster::RasterMapping {
                origin: output
                    .viewport
                    .raster
                    .logical_point(pixel_origin.map(f64::from)),
                scale: output.viewport.raster.scale,
            };
            let target = self
                .output_state
                .targets
                .entry(output.viewport.id)
                .or_insert_with(|| Target {
                    image: self
                        .app
                        .world_mut()
                        .resource_mut::<Assets<Image>>()
                        .add(output_image_format(size, self.linear_output)),
                    size,
                    origin,
                    raster,
                    fullscreen,
                });
            if target.size != size {
                let previous = std::mem::replace(
                    &mut target.image,
                    self.app
                        .world_mut()
                        .resource_mut::<Assets<Image>>()
                        .add(output_image_format(size, self.linear_output)),
                );
                self.app
                    .world_mut()
                    .resource_mut::<Assets<Image>>()
                    .remove(previous.id());
                target.size = size;
            }
            target.origin = origin;
            target.raster = raster;
            target.fullscreen = fullscreen;
            let bounds = lince_interface::host::Rectangle::new([0.0; 2], logical_size).unwrap();
            locals.push(OutputViewport { rotation: Default::default(),
                id: assignment.id,
                bounds,
                usable: region.map(|_| bounds),
                physical_size: size,
                raster: lince_interface::host::raster::RasterMapping {
                    origin: std::array::from_fn(|axis| raster.origin[axis] - origin[axis]),
                    scale: raster.scale,
                },
                workspace,
            });
        }
        let mut views = native::output_views::OutputViews::new(locals, focused)
            .ok_or_else(|| input_failure("invalid canvas output views"))?;
        let names = native::output_views::OutputNames(self.output_state.outputs.iter()
            .map(|output| (output.viewport.id, output.name.clone())).collect());
        if self.app.world().get_resource::<native::output_views::OutputNames>() != Some(&names) {
            self.app.world_mut().insert_resource(names);
        }
        views.canvas = Some(root);
        let local = views.focused().clone();
        if self
            .app
            .world()
            .get::<native::layout::desktop::HostViewport>(root)
            .is_none_or(|current| current.0 != local)
        {
            self.app.world_mut().entity_mut(root).insert((
                native::layout::desktop::Output(focused),
                native::layout::desktop::HostViewport(local),
            ));
        }
        let target = &self.output_state.targets[&focused];
        self.target = target.image.clone();
        self.render_size = target.size;
        let mut viewport = self
            .output_state
            .outputs
            .iter()
            .find(|output| output.viewport.id == focused)
            .unwrap()
            .viewport
            .clone();
        viewport.workspace = self
            .output_state
            .assignments
            .outputs()
            .iter()
            .find(|output| output.id == focused)
            .unwrap()
            .active;
        self.render_origin =
            std::array::from_fn(|axis| target.origin[axis] - viewport.bounds.origin()[axis]);
        self.render_fullscreen = target.fullscreen;
        self.output_view = Some(viewport);
        let destination = RenderTarget::Image(bevy::camera::ImageRenderTarget {
            handle: self.target.clone(),
            scale_factor: target.raster.scale[0] as f32,
        });
        for camera in [
            self.app
                .world()
                .get_resource::<native::topology::presentation::SceneCamera>()
                .map(|camera| camera.0),
            self.app
                .world()
                .get_resource::<native::topology::presentation::BackgroundCamera>()
                .map(|camera| camera.0),
        ]
        .into_iter()
        .flatten()
        {
            if !matches!(self.app.world().get::<RenderTarget>(camera), Some(RenderTarget::Image(current))
                if current.handle == self.target && current.scale_factor == target.raster.scale[0] as f32)
            {
                self.app
                    .world_mut()
                    .entity_mut(camera)
                    .insert(destination.clone());
            }
        }
        let world = self.app.world_mut();
        for mut window in world
            .query_filtered::<&mut Window, With<PrimaryWindow>>()
            .iter_mut(world)
        {
            if window.resolution.physical_size().to_array() != target.size {
                window
                    .resolution
                    .set_physical_resolution(target.size[0], target.size[1]);
            }
            if window.resolution.scale_factor() != target.raster.scale[0] as f32 {
                window
                    .resolution
                    .set_scale_factor_override(Some(target.raster.scale[0] as f32));
            }
        }
        world.insert_resource(OutputTarget(destination));
        world.insert_resource(native::topology::output_views::Targets(
            self.output_state
                .targets
                .iter()
                .map(|(id, target)| (*id, target.image.clone()))
                .collect(),
        ));
        if world.get_resource::<native::output_views::OutputViews>() != Some(&views) {
            world.insert_resource(views);
        }
        self.save_output_preferences()
            .map_err(|error| input_failure(&error.to_string()))
    }

    pub(super) fn input_mapping(&self) -> lince_interface::host::raster::RasterMapping {
        if let Some(target) = self
            .output_state
            .assignments
            .focused()
            .and_then(|output| self.output_state.targets.get(&output))
        {
            return target.raster;
        }
        lince_interface::host::raster::RasterMapping {
            origin: self.output_view.as_ref().map_or([0.0; 2], |output| {
                std::array::from_fn(|axis| output.bounds.origin()[axis] + self.render_origin[axis])
            }),
            scale: [1.0; 2],
        }
    }

    fn synchronize_workspaces(&mut self, root: Entity) -> Result<(), ProbeFailure> {
        let previous = self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        while self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .len()
            < self.output_state.outputs.len()
        {
            let count = self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .len();
            native::workspace::create(self.app.world_mut(), root);
            if self
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .len()
                == count
            {
                return Err(input_failure("could not create an output workspace"));
            }
        }
        native::workspace::switch_output(self.app.world_mut(), root, previous);
        let ids: BTreeSet<_> = self
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .filter(|space| !native::laboratory::temporary(self.app.world(), root, space.id))
            .map(|space| space.id)
            .collect();
        self.output_state.assignments.retain_workspaces(&ids);
        for workspace in ids {
            if !self
                .output_state
                .assignments
                .preferences()
                .iter()
                .any(|preference| preference.workspace == workspace)
            {
                self.output_state
                    .assignments
                    .register_workspace(workspace)
                    .map_err(|_| input_failure("could not register a workspace"))?;
            }
        }
        Ok(())
    }

    fn save_output_preferences(&mut self) -> io::Result<()> {
        let preferences = self.output_state.assignments.preferences();
        if preferences == self.output_state.saved {
            return Ok(());
        }
        if let Some(path) = &self.output_state.path {
            save(path, preferences)?;
        }
        self.output_state.saved = preferences.to_vec();
        Ok(())
    }
}

pub(super) fn save<T: serde::Serialize + ?Sized>(path: &Path, preferences: &T) -> io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(preferences)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

pub(super) fn probe(canvas: &mut Presentation) -> Result<(), ProbeFailure> {
    use lince_interface::host::{Direction, Rectangle};
    use native::actions::Action;
    use smithay::backend::allocator::Buffer;
    let output = |id, name: &str, x, size: [u32; 2]| {
        let bounds = Rectangle::new([x, 0.0], size.map(f64::from)).unwrap();
        Output {
            name: name.into(),
            viewport: OutputViewport { rotation: Default::default(),
                id: OutputId(id),
                bounds,
                usable: Some(bounds),
                physical_size: size,
                raster: lince_interface::host::raster::RasterMapping {
                    origin: (bounds).origin(),
                    scale: [1.0; 2],
                },
                workspace: 1,
            },
        }
    };
    let first = output(1, "eDP-1", 0.0, [800, 480]);
    let second = output(2, "DP-1", 800.0, [640, 400]);
    canvas.outputs(vec![first.clone(), second.clone()])?;
    let root = canvas.canvas_root()?;
    let mut widgets = Vec::new();
    for (id, color) in [
        (OutputId(1), Color::srgb(1.0, 0.0, 0.0)),
        (OutputId(2), Color::srgb(0.0, 1.0, 0.0)),
    ] {
        canvas.focus_output(id)?;
        let workspace = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let size = Vec2::splat(180.0);
        let layout = native::layout::LayoutBox::new(size);
        let entity = canvas
            .app
            .world_mut()
            .spawn((
                native::castle::Castle,
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size,
                },
                layout,
                native::workspace::WorkspaceMember(workspace),
                BackgroundColor(color),
                ChildOf(root),
            ))
            .id();
        widgets.push((entity, layout.id, workspace));
    }
    let mut frame = None;
    for _ in 0..12 {
        frame = Some(canvas.probe_frame()?);
    }
    let frame = frame.unwrap();
    let capture_cameras: Vec<_> = widgets
        .iter()
        .map(|(entity, _, _)| {
            canvas
                .app
                .world()
                .get::<native::topology::presentation::Surface>(*entity)
                .map(|surface| surface.camera)
        })
        .collect();
    if frame.outputs.len() != 1
        || frame
            .geometry
            .as_ref()
            .is_none_or(|geometry| geometry.outputs.len() != 2)
    {
        return Err(input_failure(
            "both canonical output views were not rendered",
        ));
    }
    if frame.dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?.size() != (640, 400).into()
        || frame.outputs[0].dmabuf().size() != (800, 480).into()
    {
        return Err(input_failure("output views did not keep independent sizes"));
    }
    let green = super::probe_pixels(frame.dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?)?;
    let red = super::probe_pixels(frame.outputs[0].dmabuf())?;
    let colored = |pixels: &[u8], channel| {
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[channel] > 160
                    && pixel[(channel + 1) % 3] < 80
                    && pixel[(channel + 2) % 3] < 80
            })
            .count()
    };
    if colored(&green, 1) < 1000 || colored(&red, 0) < 1000 {
        return Err(input_failure(
            "a passive output did not render its own canonical widget texture",
        ));
    }
    for output in [OutputId(1), OutputId(2)] {
        canvas.focus_output(output)?;
        canvas.desktop_action(DesktopAction::SetLayout(
            lince_interface::host::WorkspaceLayout::Free,
        ))?;
        let mut view = canvas
            .app
            .world_mut()
            .get_mut::<native::canvas::CanvasView>(root)
            .unwrap();
        view.center = bevy::math::DVec2::ZERO;
        view.zoom = 1.0;
    }
    let kept: HashSet<_> = widgets.iter().map(|(entity, _, _)| *entity).collect();
    let extra: Vec<_> = {
        let world = canvas.app.world_mut();
        world
            .query::<(Entity, &ChildOf, &native::canvas::CanvasItem)>()
            .iter(world)
            .filter(|(entity, parent, _)| parent.parent() == root && !kept.contains(entity))
            .map(|(entity, _, _)| entity)
            .collect()
    };
    for entity in extra {
        canvas.app.world_mut().despawn(entity);
    }
    for (entity, _, _) in &widgets {
        let mut item = canvas
            .app
            .world_mut()
            .get_mut::<native::canvas::CanvasItem>(*entity)
            .unwrap();
        item.position = bevy::math::DVec2::ZERO;
        item.size = Vec2::splat(180.0);
    }
    let mut area = native::area::InfluenceArea::new(
        native::area::AreaShape::Square,
        bevy::math::DVec2::ZERO,
        bevy::math::DVec2::splat(100.0),
    );
    area.hole = true;
    native::area::spawn_area(canvas.app.world_mut(), root, widgets[1].2, area)
        .ok_or_else(|| input_failure("could not create the output workspace hole"))?;
    let mut hole_frame = None;
    for _ in 0..12 {
        hole_frame = Some(canvas.probe_frame()?);
    }
    let hole_frame = hole_frame.unwrap();
    let hole_pixels = super::probe_pixels(hole_frame.dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?)?;
    if colored(&hole_pixels, 0) < 1000 {
        if let Ok(file) = tempfile::Builder::new()
            .prefix("park-hole-")
            .suffix(".png")
            .tempfile()
            && let Ok((_, path)) = file.keep()
        {
            let image = Image::new(
                wgpu::Extent3d {
                    width: 640,
                    height: 400,
                    depth_or_array_layers: 1,
                },
                wgpu::TextureDimension::D2,
                hole_pixels.clone(),
                wgpu::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            if let Ok(image) = image.try_into_dynamic()
                && image.save(&path).is_ok()
            {
                eprintln!("Park hole frame: {}", path.display());
            }
        }
        let world = canvas.app.world();
        let widgets: Vec<_> = widgets
            .iter()
            .map(|(entity, _, workspace)| {
                (
                    workspace,
                    world
                        .get::<native::canvas::CanvasItem>(*entity)
                        .map(|item| (item.position, item.size)),
                    world
                        .get::<native::topology::presentation::Surface>(*entity)
                        .map(|surface| surface.visible),
                )
            })
            .collect();
        return Err(input_failure(&format!(
            "a shared native widget was not rendered through the other output's workspace hole: center={:?}, widgets={widgets:?}, views={:?}, geometry={:?}",
            &hole_pixels[(200 * 640 + 320) * 4..(200 * 640 + 320) * 4 + 4],
            world.get_resource::<native::output_views::OutputViews>(),
            hole_frame.geometry.as_ref().map(|geometry| &geometry.items),
        )));
    }
    let exposed = hole_frame
        .geometry
        .as_ref()
        .and_then(|geometry| {
            geometry.items.iter().find(|item| {
                item.id == lince_interface::host::ItemId::Native(widgets[0].1)
                    && item.output == OutputId(2)
            })
        })
        .ok_or_else(|| input_failure("the exposed widget has no paired hole input geometry"))?;
    let center = [1120.0, 200.0];
    if exposed.surface_point(center).is_none()
        || exposed
            .surface_point([center[0] + 70.0, center[1]])
            .is_some()
    {
        return Err(input_failure(
            "workspace hole input did not match its rendered shared texture",
        ));
    }
    let preferences = canvas.output_state.assignments.preferences().to_vec();
    let previous = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    native::laboratory::LaboratoryAction::Open.apply(canvas.app.world_mut(), root);
    let (owner, output_id, temporary, restored) = native::laboratory::scope(canvas.app.world())
        .ok_or_else(|| {
            input_failure("the Laboratory did not use an output-scoped temporary workspace")
        })?;
    if owner != root || output_id != OutputId(2) || restored != previous {
        return Err(input_failure(
            "the Laboratory changed canonical output ownership",
        ));
    }
    let fake = lince_interface::host::SurfaceId(u64::MAX);
    canvas.open_window(fake, "Opened during the Laboratory", "social.lince.probe")?;
    let entity = canvas.windows[&fake].entity;
    if canvas
        .app
        .world()
        .get::<native::workspace::WorkspaceMember>(entity)
        .map(|member| member.0)
        != Some(previous)
    {
        return Err(input_failure(
            "an application was assigned to the Laboratory's temporary workspace",
        ));
    }
    let mut laboratory_frame = None;
    for _ in 0..8 {
        laboratory_frame = Some(canvas.probe_frame()?);
    }
    let laboratory_frame = laboratory_frame.unwrap();
    if laboratory_frame
        .input
        .workspaces
        .entries
        .iter()
        .any(|entry| entry.id == temporary)
        || laboratory_frame.input.workspaces.owner != root.to_bits()
        || !laboratory_frame
            .input
            .workspaces
            .entries
            .iter()
            .any(|entry| entry.id == previous)
    {
        return Err(input_failure(
            "the external workspace snapshot leaked temporary Laboratory state or lost ordinary ownership",
        ));
    }
    if colored(
        &super::probe_pixels(
            laboratory_frame
                .outputs
                .iter()
                .find(|output| output.id == OutputId(1))
                .unwrap()
                .dmabuf(),
        )?,
        0,
    ) < 1000
        || canvas.output_state.assignments.preferences() != preferences
    {
        return Err(input_failure(
            "entering the Laboratory disrupted another output or saved monitor associations",
        ));
    }
    canvas.focus_output(OutputId(1))?;
    if native::laboratory::scope(canvas.app.world()).is_none() {
        return Err(input_failure(
            "focusing another output closed the Laboratory",
        ));
    }
    let ordinary_workspace = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    let other = lince_interface::host::SurfaceId(u64::MAX - 1);
    canvas.open_window(
        other,
        "Ordinary output during Laboratory",
        "social.lince.probe",
    )?;
    let other_entity = canvas.windows[&other].entity;
    if ordinary_workspace == temporary
        || canvas
            .app
            .world()
            .get::<native::workspace::WorkspaceMember>(other_entity)
            .is_none_or(|member| member.0 != ordinary_workspace)
    {
        return Err(input_failure(
            "an ordinary output inherited the Laboratory workspace",
        ));
    }
    native::laboratory::close(canvas.app.world_mut());
    canvas.probe_frame()?;
    if canvas.app.world().get_entity(entity).is_err()
        || canvas.app.world().get_entity(other_entity).is_err()
        || canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .is_none_or(|spaces| {
                spaces.active != ordinary_workspace
                    || spaces.entries.iter().any(|space| space.id == temporary)
            })
        || canvas.output_state.assignments.preferences() != preferences
    {
        return Err(input_failure(
            "closing the Laboratory lost an application or retained temporary workspace data",
        ));
    }
    canvas.close_window(other);
    canvas.focus_output(OutputId(1))?;
    canvas.activate_application(fake)?;
    canvas.probe_frame()?;
    if canvas.output_state.assignments.focused() != Some(OutputId(2))
        || canvas.application_focus != Some(fake)
    {
        return Err(input_failure(
            "activation did not focus the application's existing monitor while its capture was pending",
        ));
    }
    native::workspace::create(canvas.app.world_mut(), root);
    let hidden = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    canvas.probe_frame()?;
    canvas.desktop_action(DesktopAction::Workspace(2))?;
    canvas.probe_frame()?;
    canvas
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(native::workspace::WorkspaceMember(hidden));
    canvas.focus_output(OutputId(1))?;
    canvas.activate_application(fake)?;
    canvas.probe_frame()?;
    if canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active
        != hidden
        || canvas.application_focus != Some(fake)
    {
        return Err(input_failure(
            "activation did not reveal a hidden workspace while its application capture was pending",
        ));
    }
    canvas.desktop_action(DesktopAction::Workspace(1))?;
    canvas.close_window(fake);
    canvas
        .app
        .world_mut()
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .retain(|entry| entry.id != hidden);
    canvas.focus_output(OutputId(2))?;
    canvas.probe_frame()?;
    for (command, action) in [
        ("/monitor/left", DesktopAction::FocusOutput(Direction::Left)),
        (
            "/monitor/right",
            DesktopAction::FocusOutput(Direction::Right),
        ),
        (
            "/workspace/monitor/left",
            DesktopAction::MoveWorkspaceToOutput(Direction::Left),
        ),
        (
            "/workspace/monitor/right",
            DesktopAction::MoveWorkspaceToOutput(Direction::Right),
        ),
    ] {
        operation(canvas, command, action)?;
    }
    canvas.desktop_action(DesktopAction::FocusOutput(Direction::Left))?;
    canvas.desktop_action(DesktopAction::Workspace(2))?;
    if canvas.output_state.assignments.focused() != Some(OutputId(2)) {
        return Err(input_failure(
            "selecting a visible workspace did not focus its monitor",
        ));
    }
    canvas.move_workspace_to_output(widgets[1].2, OutputId(1))?;
    canvas.outputs(vec![first.clone()])?;
    for _ in 0..3 {
        canvas.probe_frame()?;
    }
    canvas.outputs(vec![first, output(8, "DP-1", 800.0, [640, 400])])?;
    for _ in 0..6 {
        canvas.probe_frame()?;
    }
    let world = canvas.app.world();
    for ((entity, identity, workspace), capture) in widgets.into_iter().zip(capture_cameras) {
        if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(root)
            || world
                .get::<native::layout::LayoutBox>(entity)
                .is_none_or(|layout| layout.id != identity)
            || world
                .get::<native::workspace::WorkspaceMember>(entity)
                .is_none_or(|member| member.0 != workspace)
            || world
                .get::<native::topology::presentation::Surface>(entity)
                .map(|surface| surface.camera)
                != capture
        {
            return Err(input_failure(
                "monitor focus or reconnect changed canonical widget ownership",
            ));
        }
    }
    Ok(())
}

fn operation(
    canvas: &mut Presentation,
    command: &str,
    expected: DesktopAction,
) -> Result<(), ProbeFailure> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    canvas.host_requests(sender);
    canvas.focus(true)?;
    canvas.desktop_action(DesktopAction::OpenOperation)?;
    for _ in 0..3 {
        canvas.probe_frame()?;
    }
    canvas.text(command.into())?;
    for _ in 0..2 {
        canvas.probe_frame()?;
    }
    canvas.key(
        KeyCode::Enter,
        bevy::input::keyboard::Key::Enter,
        None,
        true,
        false,
    )?;
    canvas.probe_frame()?;
    canvas.key(
        KeyCode::Enter,
        bevy::input::keyboard::Key::Enter,
        None,
        false,
        false,
    )?;
    canvas.probe_frame()?;
    match receiver.try_recv() {
        Ok(lince_interface::host::HostRequest::Action(action)) if action == expected => {
            canvas.desktop_action(action)
        }
        request => Err(input_failure(&format!(
            "Operation {command} did not dispatch its desktop action: {request:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_control_defaults_to_builtin_then_leftmost_output() {
        use lince_interface::host::{Rectangle, raster::RasterMapping};
        let output = |id, name: &str, x| {
            let bounds = Rectangle::new([x, 0.0], [800.0, 600.0]).unwrap();
            Output {
                name: name.into(),
                viewport: OutputViewport {
                    id: OutputId(id), bounds, usable: Some(bounds), physical_size: [800, 600],
                    raster: RasterMapping { origin: [x, 0.0], scale: [1.0; 2] },
                    workspace: id, rotation: Default::default(),
                },
            }
        };
        let mut outputs = [output(1, "HDMI-A-1", 0.0), output(2, "eDP-1", 800.0), output(3, "DP-1", -800.0)];
        outputs.sort_by(default_output_order);
        assert_eq!(outputs.iter().map(|output| output.name.as_str()).collect::<Vec<_>>(), ["eDP-1", "DP-1", "HDMI-A-1"]);
    }

    #[test]
    fn output_preferences_are_private_and_atomically_replace_the_previous_document() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park-outputs.json");
        let first = vec![Preference {
            workspace: 1,
            output: "eDP-1".into(),
            active: false,
        }];
        save(&path, &first).unwrap();
        let second = vec![Preference {
            workspace: 1,
            output: "DP-1".into(),
            active: true,
        }];
        save(&path, &second).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let restored: Vec<Preference> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(restored, second);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
