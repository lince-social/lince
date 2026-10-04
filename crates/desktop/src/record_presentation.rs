use crate::{area::InfluenceArea, protein_area::RecordBinding};
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
    pub(crate) state: crate::protein_area::presentation::State,
    pub presentation: RecordPresentation,
    pub offset: Option<[f64; 3]>,
}

impl Saved {
    pub fn valid(&self) -> bool {
        self.state.valid()
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
    world
        .entity_mut(row)
        .insert((state, saved.map(|saved| saved.state).unwrap_or_default()));
}

pub(crate) fn save(world: &mut World, row: Entity) {
    let Some(binding) = world.get::<RecordBinding>(row).cloned() else {
        return;
    };
    let Some(presentation) = world.get::<RecordPresentation>(row).copied() else {
        return;
    };
    let state = crate::protein_area::presentation::capture(world, row);
    if let Some(mut area) = world.get_mut::<InfluenceArea>(binding.area) {
        let saved = area.records.entry(binding.uid).or_default();
        saved.presentation = presentation;
        saved.state = state;
    }
}

pub(crate) fn capture(world: &World, owner: Entity, mut area: InfluenceArea) -> InfluenceArea {
    let center = crate::topology::position(world, owner).unwrap_or_default();
    let rotation = crate::topology::spatial(world, owner).rotation();
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
                .and_then(|_| crate::topology::position(world, entity))
                .map(|point| (rotation.inverse() * (point - center)).to_array());
            area.records.insert(
                binding.uid.clone(),
                Saved {
                    presentation,
                    state: crate::protein_area::presentation::capture(world, entity),
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
    let spatial = crate::topology::spatial(world, binding.area);
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
