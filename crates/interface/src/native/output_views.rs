use crate::{
    host::{OutputId, OutputViewport},
    native::{canvas::CanvasView, workspace::Workspaces},
};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct OutputNames(pub Vec<(OutputId, String)>);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandFocusRequest {
    pub output: OutputId,
    pub workspace: u64,
    pub root: Entity,
    pub created: std::time::Instant,
}

impl HandFocusRequest {
    pub fn fresh(&self) -> bool {
        self.created.elapsed() <= std::time::Duration::from_millis(350)
    }

    pub fn matches(&self, root: Entity, output: OutputId, workspace: u64) -> bool {
        self.root == root && self.output == output && self.workspace == workspace
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct OutputViews {
    pub canvas: Option<Entity>,
    pub outputs: Vec<OutputViewport>,
    pub focused: OutputId,
}

impl OutputViews {
    pub fn new(outputs: Vec<OutputViewport>, focused: OutputId) -> Option<Self> {
        let mut ids = HashSet::new();
        let mut workspaces = HashSet::new();
        let pixels: u64 = outputs
            .iter()
            .map(|output| u64::from(output.physical_size[0]) * u64::from(output.physical_size[1]))
            .fold(0, u64::saturating_add);
        if outputs.is_empty()
            || pixels > 64 * 1024 * 1024
            || outputs.len() > crate::native::render_layers::OUTPUTS.len
            || !outputs.iter().any(|output| output.id == focused)
            || outputs.iter().any(|output| {
                output.workspace == 0
                    || !ids.insert(output.id)
                    || !workspaces.insert(output.workspace)
                    || !output.valid_raster()
                    || output
                        .physical_size
                        .iter()
                        .any(|size| *size == 0 || *size > 16384)
            })
        {
            return None;
        }
        Some(Self {
            canvas: None,
            outputs,
            focused,
        })
    }

    pub fn applies_to(&self, root: Entity) -> bool {
        self.canvas.is_none_or(|canvas| canvas == root)
    }

    pub fn focused(&self) -> &OutputViewport {
        self.outputs
            .iter()
            .find(|output| output.id == self.focused)
            .unwrap()
    }
}

pub(crate) fn presents(world: &World, mut root: Entity, workspace: u64) -> bool {
    let spaces = loop {
        if let Some(spaces) = world.get::<Workspaces>(root) {
            break spaces;
        }
        let Some(parent) = world.get::<ChildOf>(root) else {
            return true;
        };
        root = parent.parent();
    };
    let live = |visible| crate::native::topology::holes::live_in(world, root, visible, workspace);
    live(spaces.active)
        || world.get_resource::<OutputViews>().is_some_and(|views| {
            views.applies_to(root) && views.outputs.iter().any(|output| live(output.workspace))
        })
        || world
            .get_resource::<crate::native::topology::previews::Previews>()
            .is_some_and(|previews| {
                previews
                    .0
                    .iter()
                    .any(|preview| preview.root == root && live(preview.workspace))
            })
}

pub(crate) fn presents_entity(world: &World, mut entity: Entity) -> bool {
    let mut workspace = None;
    loop {
        if world.get_entity(entity).is_err()
            || world
                .get::<bevy::ecs::entity_disabling::Disabled>(entity)
                .is_some()
            || world
                .get::<Node>(entity)
                .is_some_and(|node| node.display == Display::None)
            || world
                .get::<crate::native::layout::desktop::Suppressed>(entity)
                .is_some()
        {
            return false;
        }
        if workspace.is_none() {
            workspace = world
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .map(|member| member.0);
        }
        if world.get::<Workspaces>(entity).is_some()
            && let Some(workspace) = workspace.take()
            && !presents(world, entity, workspace)
        {
            return false;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return true;
        };
        entity = parent.parent();
    }
}

pub fn canvas(world: &World, root: Entity, workspace: u64) -> CanvasView {
    if let Some(spaces) = world.get::<Workspaces>(root) {
        if spaces.active == workspace {
            return world.get::<CanvasView>(root).copied().unwrap_or_default();
        }
        if let Some(space) = spaces.entries.iter().find(|space| space.id == workspace) {
            return CanvasView {
                center: bevy::math::DVec2::from_array(space.center),
                zoom: space.zoom,
            };
        }
    }
    CanvasView::default()
}

pub(crate) mod tests {
    use super::*;
    use crate::host::Rectangle;

    fn output(id: u64, workspace: u64) -> OutputViewport {
        let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
        OutputViewport {
            rotation: Default::default(),
            id: OutputId(id),
            bounds,
            usable: Some(bounds),
            physical_size: [800, 600],
            raster: crate::host::raster::RasterMapping {
                origin: (bounds).origin(),
                scale: [1.0; 2],
            },
            workspace,
        }
    }

    #[test]
    fn views_require_unique_outputs_workspaces_and_a_live_focus() {
        assert!(OutputViews::new(vec![output(1, 1), output(2, 2)], OutputId(1)).is_some());
        assert!(OutputViews::new(vec![output(1, 1), output(1, 2)], OutputId(1)).is_none());
        assert!(OutputViews::new(vec![output(1, 1), output(2, 1)], OutputId(1)).is_none());
        assert!(OutputViews::new(vec![output(1, 1)], OutputId(2)).is_none());
    }

    #[test]
    fn inactive_views_read_the_canonical_workspace_camera() {
        let mut world = World::new();
        let mut spaces = Workspaces::default();
        let mut other = spaces.entries[0].clone();
        other.id = 2;
        other.center = [25.0, 90.0];
        other.zoom = 1.5;
        spaces.entries.push(other);
        let root = world.spawn((spaces, CanvasView::default())).id();
        assert_eq!(canvas(&world, root, 2).center.to_array(), [25.0, 90.0]);
        assert_eq!(canvas(&world, root, 2).zoom, 1.5);
        assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
    }

    #[cfg_attr(test, test)]
    fn panel_demand_follows_every_output_preview_and_portal_without_waiting_for_layout() {
        use crate::native::{
            topology::{holes, previews},
            workspace::WorkspaceMember,
        };
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let root = world.spawn(Workspaces::default()).id();
        let owner = world
            .spawn((WorkspaceMember(2), Node::default(), ChildOf(root)))
            .id();
        let panel = world.spawn((Node::default(), ChildOf(owner))).id();
        assert!(!presents_entity(&world, panel));
        world.insert_resource(
            OutputViews::new(vec![output(1, 1), output(2, 2)], OutputId(1)).unwrap(),
        );
        assert!(presents_entity(&world, panel));
        world.get_mut::<Node>(owner).unwrap().display = Display::None;
        assert!(!presents_entity(&world, panel));
        world.get_mut::<Node>(owner).unwrap().display = Display::Flex;
        world.remove_resource::<OutputViews>();
        world.insert_resource(previews::Previews(vec![previews::Preview {
            id: owner,
            root,
            workspace: 2,
            view: CanvasView::default(),
            pixels: UVec2::splat(100),
            image: default(),
        }]));
        assert!(presents_entity(&world, panel));
        world.remove_resource::<previews::Previews>();
        world
            .entity_mut(root)
            .insert(holes::Layers(vec![holes::Layer {
                workspace: 2,
                depth: 1,
                openings: Vec::new(),
                cuts: Vec::new(),
            }]));
        assert!(presents_entity(&world, panel));
        world
            .entity_mut(owner)
            .insert(bevy::ecs::entity_disabling::Disabled);
        assert!(!presents_entity(&world, panel));
        world
            .entity_mut(owner)
            .remove::<bevy::ecs::entity_disabling::Disabled>();
        world.entity_mut(root).remove::<holes::Layers>();
        assert!(!presents_entity(&world, panel));
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
        assert!(presents_entity(&world, panel));
    }

    crate::native::laboratory_cases! {
        panel_demand_follows_every_output_preview_and_portal_without_waiting_for_layout,
    }
}
