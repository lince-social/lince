#[cfg(test)]
mod tests;
mod ui;

use crate::{
    area::InfluenceArea,
    canvas::CanvasItem,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundArea {
    pub enter: String,
    pub leave: String,
    pub volume: f32,
}

impl Default for SoundArea {
    fn default() -> Self {
        Self {
            enter: String::new(),
            leave: String::new(),
            volume: 0.7,
        }
    }
}

impl SoundArea {
    pub fn valid(&self) -> bool {
        [&self.enter, &self.leave]
            .iter()
            .all(|path| path.is_empty() || crate::sound::library::valid_path(path))
            && self.volume.is_finite()
            && (0.0..=1.0).contains(&self.volume)
    }
}

#[derive(Component)]
pub struct SoundStatus(pub String);

#[derive(Default)]
struct Crossings {
    areas: HashMap<Entity, (SoundArea, HashMap<Entity, bool>)>,
}

#[derive(Clone, Debug)]
struct Playback {
    owner: Entity,
    path: String,
    volume: f32,
}

impl Crossings {
    fn sample(
        &mut self,
        owner: Entity,
        sound: &SoundArea,
        sands: HashMap<Entity, bool>,
    ) -> Vec<Playback> {
        let mut plays = Vec::new();
        if let Some((previous_sound, previous)) = self.areas.get(&owner)
            && previous_sound == sound
        {
            for (sand, inside) in &sands {
                let before = previous.get(sand).copied().unwrap_or(false);
                if before != *inside {
                    let path = if *inside { &sound.enter } else { &sound.leave };
                    if !path.is_empty() && !plays.iter().any(|play: &Playback| play.path == *path) {
                        plays.push(Playback {
                            owner,
                            path: path.clone(),
                            volume: sound.volume,
                        });
                    }
                }
            }
        }
        self.areas.insert(owner, (sound.clone(), sands));
        plays
    }
}

#[derive(Resource, Default)]
struct Runtime(Crossings);

pub struct SoundAreaPlugin;
impl Plugin for SoundAreaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_systems(PostUpdate, update.after(crate::area::ComputeAreaForces))
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

pub(crate) use ui::controls;

fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    let areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(_, area, parent, member)| {
            area.enabled
                && area.sound.is_some()
                && area.validate()
                && world
                    .get::<Workspaces>(parent.parent())
                    .is_some_and(|spaces| spaces.active == member.0)
        })
        .map(|(entity, area, parent, member)| (entity, area.clone(), parent.parent(), member.0))
        .collect();
    let sands: Vec<_> = world.query_filtered::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember), Without<InfluenceArea>>()
        .iter(world).map(|(e, i, p, m)| (e, *i, p.parent(), m.0)).collect();
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    runtime.0.areas.retain(|entity, _| {
        let active = areas.iter().any(|(e, ..)| e == entity);
        if !active {
            let _ = crate::sound::send(world, crate::sound::Command::Stop(*entity));
        }
        active
    });
    for (entity, area, root, workspace) in areas {
        let spatial = world
            .get::<crate::topology::presentation::SpatialRoot>(root)
            .is_some();
        let placement = crate::topology::spatial(world, entity);
        let members = sands
            .iter()
            .filter(|(_, _, parent, member)| *parent == root && *member == workspace)
            .map(|(sand, item, _, _)| {
                let record = world.get::<crate::area::RecordProperties>(*sand);
                let matches = if area.filter.is_some() {
                    record.is_some_and(|record| {
                        world
                            .get::<crate::protein_area::filter::Matches>(entity)
                            .is_some_and(|filter| {
                                filter.allows(
                                    record,
                                    world.get::<crate::protein_area::RecordBinding>(*sand),
                                )
                            })
                    })
                } else {
                    area.rules.is_empty() || record.is_some_and(|record| area.matches(record))
                };
                let inside = matches
                    && if spatial {
                        crate::topology::influence::contains(
                            &area,
                            placement,
                            crate::topology::spatial(world, *sand).position(item.position),
                        )
                    } else {
                        area.contains(item.position)
                    };
                (*sand, inside)
            })
            .collect();
        for play in runtime
            .0
            .sample(entity, area.sound.as_ref().unwrap(), members)
        {
            let result = crate::sound::send(
                world,
                crate::sound::Command::Play {
                    owner: play.owner,
                    path: play.path,
                    effects: None,
                    volume: play.volume,
                },
            );
            if let Err(error) = result {
                world.entity_mut(entity).insert(SoundStatus(error));
            }
        }
    }
    world.insert_resource(runtime);
}
