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

#[derive(Clone)]
pub struct Output {
    pub name: String,
    pub viewport: OutputViewport,
}

pub(super) struct Target {
    pub image: Handle<Image>,
    pub size: [u32; 2],
    pub origin: [f64; 2],
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
    pub fn activate_application(
        &mut self,
        surface: lince_interface::host::SurfaceId,
    ) -> Result<(), ProbeFailure> {
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
        if let Some(output) = self.output_state.assignments.focused() {
            self.output_state
                .assignments
                .select(output, workspace)
                .map_err(|_| input_failure("could not activate the application's workspace"))?;
            self.select_output_workspace()?;
            self.refresh_outputs()?;
        } else {
            native::workspace::switch_output(self.app.world_mut(), root, workspace);
        }
        self.focus_application(surface);
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
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let pixels: u64 = outputs
            .iter()
            .map(|output| {
                u64::from(output.viewport.physical_size[0])
                    * u64::from(output.viewport.physical_size[1])
            })
            .fold(0, u64::saturating_add);
        if outputs.is_empty()
            || pixels > 64 * 1024 * 1024
            || outputs.len() > 16
            || outputs.iter().any(|output| {
                output.name.is_empty()
                    || output.name.len() > 512
                    || !ids.insert(output.viewport.id)
                    || !names.insert(&output.name)
                    || validate_size(output.viewport.physical_size).is_err()
            })
        {
            return Err(input_failure("invalid Park output configuration"));
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

    pub(super) fn output_action(&mut self, action: DesktopAction) -> Result<bool, ProbeFailure> {
        if self.output_state.outputs.is_empty() {
            return Ok(false);
        }
        let root = self.canvas_root()?;
        if matches!(
            action,
            DesktopAction::Workspace(_)
                | DesktopAction::MoveToOutput(_)
                | DesktopAction::MoveWorkspaceToOutput(_)
        ) {
            native::laboratory::close(self.app.world_mut());
        }
        match action {
            DesktopAction::Workspace(index) => {
                let Some(workspace) = index.checked_sub(1).and_then(|index| {
                    self.app
                        .world()
                        .get::<native::workspace::Workspaces>(root)?
                        .entries
                        .get(index as usize)
                        .map(|space| space.id)
                }) else {
                    return Ok(true);
                };
                let output = self.output_state.assignments.focused().unwrap();
                self.focus(false)?;
                self.output_state
                    .assignments
                    .select(output, workspace)
                    .map_err(|_| input_failure("could not select the workspace"))?;
                self.select_output_workspace()?;
                self.refresh_outputs()?;
                self.focus(true)?;
            }
            DesktopAction::FocusOutput(direction) => {
                if let Some(output) = self.output_state.assignments.directional(direction) {
                    self.focus_output(output)?;
                }
            }
            DesktopAction::MoveWorkspaceToOutput(direction) => {
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
            DesktopAction::MoveToOutput(direction) => {
                if let Some(output) = self.output_state.assignments.directional(direction) {
                    let workspace = self
                        .output_state
                        .assignments
                        .outputs()
                        .iter()
                        .find(|candidate| candidate.id == output)
                        .unwrap()
                        .active;
                    let focused = self
                        .app
                        .world()
                        .get::<native::workspace::Workspaces>(root)
                        .unwrap()
                        .entries
                        .iter()
                        .find(|space| {
                            space.id
                                == self
                                    .app
                                    .world()
                                    .get::<native::workspace::Workspaces>(root)
                                    .unwrap()
                                    .active
                        })
                        .and_then(|space| space.desktop.focused());
                    if let Some(focused) = focused {
                        let entity = {
                            let world = self.app.world_mut();
                            world
                                .query::<(Entity, &native::layout::desktop::Managed, &ChildOf)>()
                                .iter(world)
                                .find(|(_, item, parent)| {
                                    parent.parent() == root && item.id == focused
                                })
                                .map(|(entity, _, _)| entity)
                        };
                        if let Some(entity) = entity {
                            self.app
                                .world_mut()
                                .entity_mut(entity)
                                .insert(native::workspace::WorkspaceMember(workspace));
                        }
                        self.focus_output(output)?;
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn select_output_workspace(&mut self) -> Result<(), ProbeFailure> {
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
        for output in &self.output_state.outputs {
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
            let size = region.map_or([1; 2], |region| {
                region.size().map(|value| value.round().max(1.0) as u32)
            });
            validate_size(size)?;
            let origin = region.map_or(output.viewport.bounds.origin(), |region| region.origin());
            let target = self
                .output_state
                .targets
                .entry(output.viewport.id)
                .or_insert_with(|| Target {
                    image: self
                        .app
                        .world_mut()
                        .resource_mut::<Assets<Image>>()
                        .add(output_image(size)),
                    size,
                    origin,
                    fullscreen,
                });
            if target.size != size {
                let previous = std::mem::replace(
                    &mut target.image,
                    self.app
                        .world_mut()
                        .resource_mut::<Assets<Image>>()
                        .add(output_image(size)),
                );
                self.app
                    .world_mut()
                    .resource_mut::<Assets<Image>>()
                    .remove(previous.id());
                target.size = size;
            }
            target.origin = origin;
            target.fullscreen = fullscreen;
            let bounds =
                lince_interface::host::Rectangle::new([0.0; 2], size.map(f64::from)).unwrap();
            locals.push(OutputViewport {
                id: assignment.id,
                bounds,
                usable: region.map(|_| bounds),
                physical_size: size,
                workspace,
            });
        }
        let mut views = native::output_views::OutputViews::new(locals, focused)
            .ok_or_else(|| input_failure("invalid canvas output views"))?;
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
        let destination = RenderTarget::Image(self.target.clone().into());
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
            if !matches!(self.app.world().get::<RenderTarget>(camera), Some(RenderTarget::Image(current)) if current.handle == self.target)
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
            viewport: OutputViewport {
                id: OutputId(id),
                bounds,
                usable: Some(bounds),
                physical_size: size,
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
    if frame.dmabuf().size() != (640, 400).into()
        || frame.outputs[0].dmabuf().size() != (800, 480).into()
    {
        return Err(input_failure("output views did not keep independent sizes"));
    }
    let green = super::probe_pixels(frame.dmabuf())?;
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
    let hole_pixels = super::probe_pixels(hole_frame.dmabuf())?;
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
    if laboratory_frame.input.workspaces.entries.iter().any(|entry| entry.id == temporary)
        || laboratory_frame.input.workspaces.owner != root.to_bits()
        || !laboratory_frame.input.workspaces.entries.iter().any(|entry| entry.id == previous) {
        return Err(input_failure("the external workspace snapshot leaked temporary Laboratory state or lost ordinary ownership"));
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
        return Err(input_failure("focusing another output closed the Laboratory"));
    }
    let ordinary_workspace = canvas.app.world()
        .get::<native::workspace::Workspaces>(root).unwrap().active;
    let other = lince_interface::host::SurfaceId(u64::MAX - 1);
    canvas.open_window(other, "Ordinary output during Laboratory", "social.lince.probe")?;
    let other_entity = canvas.windows[&other].entity;
    if ordinary_workspace == temporary || canvas.app.world()
        .get::<native::workspace::WorkspaceMember>(other_entity)
        .is_none_or(|member| member.0 != ordinary_workspace)
    {
        return Err(input_failure("an ordinary output inherited the Laboratory workspace"));
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
    fn output_preferences_are_private_and_atomically_replace_the_previous_document() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park-outputs.json");
        let first = vec![Preference {
            workspace: 1,
            output: "eDP-1".into(),
        }];
        save(&path, &first).unwrap();
        let second = vec![Preference {
            workspace: 1,
            output: "DP-1".into(),
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
