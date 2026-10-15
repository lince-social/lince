use bevy::{
    math::{DQuat, DVec2, DVec3},
    prelude::*,
};
use serde::{Deserialize, Serialize};

mod area_reach;
mod area_summary;
pub(crate) use area_summary::{SummaryAtBottom, SummaryInEditMode};
pub mod appearance;
pub mod areas;
pub mod assets;
mod font_cache;
pub mod groups;
pub(crate) mod holes;
pub mod influence;
pub mod input;
pub mod output_views;
pub mod physics;
pub mod presentation;
mod resizing;
pub mod rooms;
pub mod splats;
mod surface_budget;
pub(crate) mod surface_render;
pub mod terrain;
pub mod ui;
pub mod view;

pub struct TopologyPlugin;

impl Plugin for TopologyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(surface_render::SurfaceRenderPlugin)
            .init_resource::<terrain::Terrain>()
            .init_resource::<assets::Imports>()
            .init_resource::<physics::Runtime>()
            .init_resource::<input::PointerState>()
            .add_systems(Last, font_cache::trim)
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn(input::CONTENT_POINTER);
            })
            .add_systems(
                First,
                input::pointer.in_set(bevy::picking::PickingSystems::Input),
            )
            .add_systems(
                PreUpdate,
                input::gestures.after(crate::native::inspection::InspectInput),
            )
            .add_systems(
                Update,
                (
                    holes::refresh,
                    assets::update,
                    area_summary::update,
                    area_summary::hover,
                    presentation::synchronize,
                    appearance::update
                        .after(crate::native::description::shader::RenderShaderEffects),
                    areas::update,
                    area_reach::update,
                    ui::update,
                    terrain::draw,
                    terrain::editor::update,
                )
                    .chain()
                    .after(crate::native::workspace::PrepareWorkspaces)
                    .after(crate::native::physics::SimulateWorkspaces),
            );
        terrain::install(app);
        app.add_systems(
            Update,
            rooms::synchronize
                .after(crate::native::physics::SimulateWorkspaces)
                .before(presentation::synchronize),
        );
        app.add_systems(Update, rooms::update_controls.after(ui::update));
        app.add_systems(
            Update,
            view::synchronize
                .before(crate::native::physics::SimulateWorkspaces)
                .after(crate::native::workspace::PrepareWorkspaces),
        );
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spatial {
    pub elevation: f64,
    pub rotation: [f64; 4],
    pub depth: Option<f64>,
    pub world_pinned: bool,
}

impl Default for Spatial {
    fn default() -> Self {
        Self {
            elevation: 0.0,
            rotation: DQuat::IDENTITY.to_array(),
            depth: None,
            world_pinned: false,
        }
    }
}

impl Spatial {
    pub fn valid(&self) -> bool {
        self.elevation.is_finite()
            && DQuat::from_array(self.rotation).is_finite()
            && (DQuat::from_array(self.rotation).length_squared() - 1.0).abs() < 1e-6
            && self
                .depth
                .is_none_or(|depth| depth.is_finite() && depth > 0.0 && depth <= 100_000.0)
    }

    pub fn depth(&self, size: Vec2) -> f64 {
        self.depth.unwrap_or(f64::from(size.min_element()))
    }

    pub fn position(&self, point: DVec2) -> DVec3 {
        DVec3::new(point.x, self.elevation, point.y)
    }

    pub fn rotation(&self) -> DQuat {
        DQuat::from_array(self.rotation)
    }

    pub fn local_point(&self, origin: DVec2, point: DVec3) -> DVec3 {
        self.rotation().inverse() * (point - self.position(origin))
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
}

impl Attachment {
    pub fn valid(&self) -> bool {
        DVec3::from_array(self.position).is_finite()
            && DQuat::from_array(self.rotation).is_finite()
            && (DQuat::from_array(self.rotation).length_squared() - 1.0).abs() < 1e-6
    }
}

pub fn spatial(world: &World, entity: Entity) -> Spatial {
    world.get::<Spatial>(entity).copied().unwrap_or_default()
}

pub fn position(world: &World, entity: Entity) -> Option<DVec3> {
    let item = world.get::<crate::native::canvas::CanvasItem>(entity)?;
    Some(spatial(world, entity).position(item.position))
}

pub(crate) fn participation_point(
    world: &World,
    entity: Entity,
    item: &crate::native::canvas::CanvasItem,
) -> DVec3 {
    let placement = spatial(world, entity);
    let point = placement.position(item.position);
    let Some(parent) = world.get::<ChildOf>(entity) else {
        return point;
    };
    let Some(member) = world.get::<crate::native::workspace::WorkspaceMember>(entity) else {
        return point;
    };
    if !world.contains_resource::<terrain::Terrain>()
        || world
            .get::<crate::native::area::InfluenceArea>(entity)
            .is_some()
        || world.get::<assets::ImportedAsset>(entity).is_some()
        || world.get::<terrain::Effect>(entity).is_some()
    {
        return point;
    }
    let scale = world
        .get::<crate::native::area_effects::AreaScale>(entity)
        .map_or(1.0, |s| f64::from(s.0));
    let mut foot = point - placement.rotation() * DVec3::Y * placement.depth(item.size) * scale;
    let ground = terrain::height(world, parent.parent(), member.0, DVec2::new(foot.x, foot.z));
    if (foot.y - ground).abs() < 2.0 {
        foot.y = ground;
    }
    foot
}

pub fn set_position(world: &mut World, entity: Entity, position: DVec3) {
    if !position.is_finite()
        || world
            .get::<crate::native::canvas::CanvasItem>(entity)
            .is_none()
    {
        return;
    }
    let planar = DVec2::new(position.x, position.z);
    world
        .get_mut::<crate::native::canvas::CanvasItem>(entity)
        .unwrap()
        .position = planar;
    let mut placement = spatial(world, entity);
    placement.elevation = position.y;
    world.entity_mut(entity).insert(placement);
    if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity) {
        area.center = planar.to_array();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_follows_resizing_until_overridden() {
        let mut placement = Spatial::default();
        assert_eq!(placement.depth(Vec2::new(300.0, 100.0)), 100.0);
        assert_eq!(placement.depth(Vec2::new(80.0, 200.0)), 80.0);
        placement.depth = Some(45.0);
        assert_eq!(placement.depth(Vec2::new(80.0, 200.0)), 45.0);
        let saved = serde_json::to_string(&placement).unwrap();
        assert_eq!(serde_json::from_str::<Spatial>(&saved).unwrap(), placement);
        placement.depth = None;
        assert_eq!(placement.depth(Vec2::new(80.0, 200.0)), 80.0);
    }

    #[test]
    fn invalid_placements_are_rejected() {
        for depth in [0.0, -1.0, f64::NAN, f64::INFINITY, 100_001.0] {
            assert!(
                !Spatial {
                    depth: Some(depth),
                    ..default()
                }
                .valid()
            );
        }
        assert!(
            !Spatial {
                rotation: [0.0; 4],
                ..default()
            }
            .valid()
        );
    }

    #[test]
    fn local_coordinates_preserve_large_offsets_and_rotation() {
        let placement = Spatial {
            elevation: 20.0,
            rotation: DQuat::from_rotation_y(0.7).to_array(),
            ..default()
        };
        let origin = DVec2::new(1e9, -1e9);
        let local = DVec3::new(12.0, -4.0, 25.0);
        let point = placement.position(origin) + placement.rotation() * local;
        assert!(placement.local_point(origin, point).distance(local) < 1e-6);
    }
}
