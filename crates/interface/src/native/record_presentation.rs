use crate::native::{area::InfluenceArea, protein_area::RecordBinding};
use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordPresentation {
    pub hide_filled: bool,
    pub expanded: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub room_member: Option<nucleus::canvas::room::Member>,
    #[serde(default)]
    pub room_world: Option<nucleus::canvas::Pose>,
    #[serde(default)]
    pub material: Option<nucleus::canvas::appearance::Material>,
    #[serde(default)]
    pub light: Option<nucleus::canvas::appearance::Light>,
    #[serde(default)]
    pub shader_placement: Option<crate::native::shader_effects::FloatingSettings>,
    #[serde(default)]
    pub shader_surface: Option<crate::native::shader_effects::Surface>,
    #[serde(default)]
    pub(crate) state: crate::native::protein_area::presentation::State,
    pub presentation: RecordPresentation,
    pub offset: Option<[f64; 3]>,
}

impl Saved {
    pub fn valid(&self) -> bool {
        self.room_member
            .as_ref()
            .is_none_or(nucleus::canvas::room::Member::valid)
            && self
                .room_world
                .as_ref()
                .is_none_or(nucleus::canvas::room::valid_pose)
            && !(self.room_world.is_some() && self.room_member.is_some())
            && self
                .material
                .as_ref()
                .is_none_or(nucleus::canvas::appearance::Material::valid)
            && self
                .light
                .as_ref()
                .is_none_or(nucleus::canvas::appearance::Light::valid)
            && self
                .shader_surface
                .as_ref()
                .is_none_or(crate::native::shader_effects::Surface::valid)
            && self
                .shader_placement
                .as_ref()
                .is_none_or(crate::native::shader_effects::FloatingSettings::valid)
            && self.state.valid()
            && self
                .offset
                .is_none_or(|offset| DVec3::from_array(offset).is_finite())
    }
}

pub(crate) fn restore(world: &mut World, row: Entity, hide_filled: bool) {
    if world.get::<RecordPresentation>(row).is_some() {
        return;
    }
    let saved = world
        .get::<RecordBinding>(row)
        .and_then(|binding| {
            world
                .get::<InfluenceArea>(binding.area)?
                .records
                .get(&binding.uid)
        })
        .cloned();
    let state = saved.as_ref().map_or(
        RecordPresentation {
            hide_filled,
            expanded: false,
        },
        |saved| saved.presentation,
    );
    let surface = saved
        .as_ref()
        .and_then(|saved| saved.shader_surface.clone());
    if let Some(member) = saved.as_ref().and_then(|saved| saved.room_member.clone()) {
        world
            .entity_mut(row)
            .insert(crate::native::topology::rooms::Member(member));
    }
    if let Some(pose) = saved.as_ref().and_then(|saved| saved.room_world.clone()) {
        world
            .entity_mut(row)
            .insert(crate::native::topology::rooms::WorldPlacement(pose));
    }
    if let Some(material) = saved.as_ref().and_then(|saved| saved.material.clone()) {
        world
            .entity_mut(row)
            .insert(crate::native::topology::appearance::Material(material));
    }
    if let Some(light) = saved.as_ref().and_then(|saved| saved.light.clone()) {
        world
            .entity_mut(row)
            .insert(crate::native::topology::appearance::Light(light));
    }
    world
        .entity_mut(row)
        .insert((state, saved.map(|saved| saved.state).unwrap_or_default()));
    if let Some(surface) = surface {
        world.entity_mut(row).insert(surface);
    }
}

pub(crate) fn save(world: &mut World, row: Entity) {
    let Some(binding) = world.get::<RecordBinding>(row).cloned() else {
        return;
    };
    let Some(presentation) = world.get::<RecordPresentation>(row).copied() else {
        return;
    };
    let state = crate::native::protein_area::presentation::capture(world, row);
    let shader_surface = world
        .get::<crate::native::shader_effects::Surface>(row)
        .cloned();
    let material = world
        .get::<crate::native::topology::appearance::Material>(row)
        .map(|value| value.0.clone());
    let light = world
        .get::<crate::native::topology::appearance::Light>(row)
        .map(|value| value.0.clone());
    let room_member = world
        .get::<crate::native::topology::rooms::Member>(row)
        .map(|value| value.0.clone());
    let room_world = world
        .get::<crate::native::topology::rooms::WorldPlacement>(row)
        .map(|value| value.0.clone());
    if let Some(mut area) = world.get_mut::<InfluenceArea>(binding.area) {
        let saved = area.records.entry(binding.uid).or_default();
        saved.presentation = presentation;
        saved.state = state;
        saved.shader_surface = shader_surface;
        saved.material = material;
        saved.light = light;
        saved.room_member = room_member;
        saved.room_world = room_world;
    }
}

pub(crate) fn capture(world: &World, owner: Entity, mut area: InfluenceArea) -> InfluenceArea {
    let center = crate::native::topology::position(world, owner).unwrap_or_default();
    let rotation = crate::native::topology::spatial(world, owner).rotation();
    if let Some(children) = world
        .get::<ChildOf>(owner)
        .and_then(|parent| world.get::<Children>(parent.parent()))
    {
        for entity in children.iter() {
            let Some(binding) = world
                .get::<RecordBinding>(entity)
                .filter(|binding| binding.area == owner)
            else {
                continue;
            };
            let Some(presentation) = world.get::<RecordPresentation>(entity).copied() else {
                continue;
            };
            let offset = area
                .protein
                .as_ref()
                .filter(|config| config.motion.is_some())
                .and_then(|_| crate::native::topology::position(world, entity))
                .map(|point| (rotation.inverse() * (point - center)).to_array());
            let shader_placement = area
                .records
                .get(&binding.uid)
                .and_then(|saved| saved.shader_placement.clone());
            area.records.insert(
                binding.uid.clone(),
                Saved {
                    room_world: world
                        .get::<crate::native::topology::rooms::WorldPlacement>(entity)
                        .map(|value| value.0.clone()),
                    room_member: world
                        .get::<crate::native::topology::rooms::Member>(entity)
                        .map(|value| value.0.clone()),
                    material: world
                        .get::<crate::native::topology::appearance::Material>(entity)
                        .map(|value| value.0.clone()),
                    light: world
                        .get::<crate::native::topology::appearance::Light>(entity)
                        .map(|value| value.0.clone()),
                    shader_placement,
                    shader_surface: world
                        .get::<crate::native::shader_effects::Surface>(entity)
                        .cloned(),
                    presentation,
                    state: crate::native::protein_area::presentation::capture(world, entity),
                    offset,
                },
            );
        }
    }
    area
}

pub(crate) fn position(world: &World, row: Entity) -> Option<DVec3> {
    let binding = world.get::<RecordBinding>(row)?;
    let area = world.get::<InfluenceArea>(binding.area)?;
    let offset = area.records.get(&binding.uid)?.offset?;
    let spatial = crate::native::topology::spatial(world, binding.area);
    Some(
        spatial.position(bevy::math::DVec2::from_array(area.center))
            + spatial.rotation() * DVec3::from_array(offset),
    )
}

pub(crate) fn remember(world: &mut World, owner: Entity) {
    let Some(area) = world.get::<InfluenceArea>(owner).cloned() else {
        return;
    };
    let saved = capture(world, owner, area);
    if world
        .get::<InfluenceArea>(owner)
        .is_some_and(|area| area.records != saved.records)
    {
        world.get_mut::<InfluenceArea>(owner).unwrap().records = saved.records;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::topology::appearance::{Light, Material};

    #[test]
    fn record_appearance_and_room_membership_survive_projection_recreation_and_workspace_serialization()
     {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let area = world
            .spawn((
                InfluenceArea::new(
                    crate::native::area::AreaShape::Circle,
                    bevy::math::DVec2::ZERO,
                    bevy::math::DVec2::splat(100.0),
                ),
                ChildOf(root),
            ))
            .id();
        let binding = RecordBinding {
            area,
            uid: "appearance-record".into(),
            source: crate::native::protein_area::Source::Local,
        };
        let material = nucleus::canvas::appearance::Material {
            metallic: 0.4,
            texture: nucleus::canvas::appearance::Texture::Tiles,
            ..default()
        };
        let light = nucleus::canvas::appearance::Light {
            drive: nucleus::canvas::appearance::LightDrive::Progress,
            intensity: 300.0,
            ..default()
        };
        let member = nucleus::canvas::room::Member {
            room: nucleus::new_uid("placement"),
            parent: None,
            local: nucleus::canvas::room::identity_pose(),
            role: nucleus::canvas::room::Role::Content,
        };
        let row = world
            .spawn((
                binding.clone(),
                ChildOf(root),
                RecordPresentation::default(),
                Material(material.clone()),
                Light(light.clone()),
                crate::native::topology::rooms::Member(member.clone()),
            ))
            .id();
        remember(&mut world, area);
        let encoded = serde_json::to_string(world.get::<InfluenceArea>(area).unwrap()).unwrap();
        let restored: InfluenceArea = serde_json::from_str(&encoded).unwrap();
        assert!(restored.records[&binding.uid].valid());
        world.entity_mut(area).insert(restored);
        world.despawn(row);
        let row = world.spawn((binding.clone(), ChildOf(root))).id();
        restore(&mut world, row, false);
        assert_eq!(world.get::<Material>(row).unwrap().0, material);
        assert_eq!(world.get::<Light>(row).unwrap().0, light);
        assert_eq!(
            world
                .get::<crate::native::topology::rooms::Member>(row)
                .unwrap()
                .0,
            member
        );
        world.entity_mut(row).remove::<Material>();
        world
            .entity_mut(row)
            .remove::<crate::native::topology::rooms::Member>();
        world.get_mut::<Light>(row).unwrap().0.enabled = false;
        save(&mut world, row);
        let saved = &world.get::<InfluenceArea>(area).unwrap().records[&binding.uid];
        assert!(saved.material.is_none());
        assert!(saved.room_member.is_none());
        assert!(!saved.light.as_ref().unwrap().enabled);
        let mut invalid = saved.clone();
        invalid.light.as_mut().unwrap().intensity = f32::NAN;
        assert!(!invalid.valid());
        invalid = saved.clone();
        invalid.material = Some(nucleus::canvas::appearance::Material {
            roughness: f32::NAN,
            ..default()
        });
        assert!(!invalid.valid());
    }
}
