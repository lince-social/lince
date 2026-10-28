use crate::{
    host::{OutputId, OutputViewport},
    native::{canvas::CanvasView, workspace::Workspaces},
};
use bevy::prelude::*;
use std::collections::HashSet;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Rectangle;

    fn output(id: u64, workspace: u64) -> OutputViewport {
        let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
        OutputViewport { rotation: Default::default(),
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
}
