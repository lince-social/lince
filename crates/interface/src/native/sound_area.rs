// non-lean-yet
// Scope: mod tests, mod ui, struct SoundArea, struct SoundStatus.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 74, 102.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (144).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   230.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (220, 221).

#[cfg(test)]
mod tests;
mod ui;

use crate::native::{area::InfluenceArea, canvas::CanvasItem, workspace::WorkspaceMember};
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
            .all(|path| path.is_empty() || crate::native::sound::library::valid_path(path))
            && self.volume.is_finite()
            && (0.0..=1.0).contains(&self.volume)
    }
}

#[derive(Component)]
pub struct SoundStatus(pub String);

#[derive(Default)]
struct Crossings {
    areas: HashMap<Entity, Crossing>,
}

struct Crossing {
    sound: SoundArea,
    members: HashMap<Entity, Membership>,
    epoch: bool,
}

struct Membership {
    inside: bool,
    epoch: bool,
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
        sands: impl IntoIterator<Item = (Entity, bool)>,
    ) -> Vec<Playback> {
        use std::collections::hash_map::Entry;
        let (crossing, continuing) = match self.areas.entry(owner) {
            Entry::Vacant(entry) => (
                entry.insert(Crossing {
                    sound: sound.clone(),
                    members: HashMap::new(),
                    epoch: false,
                }),
                false,
            ),
            Entry::Occupied(entry) => {
                let crossing = entry.into_mut();
                let continuing = crossing.sound == *sound;
                if !continuing {
                    crossing.sound.clone_from(sound);
                }
                (crossing, continuing)
            }
        };
        crossing.epoch = !crossing.epoch;
        let epoch = crossing.epoch;
        let mut plays = Vec::new();
        for (sand, inside) in sands {
            let member = crossing.members.entry(sand).or_insert(Membership {
                inside: false,
                epoch,
            });
            if continuing && member.inside != inside {
                let path = if inside { &sound.enter } else { &sound.leave };
                if !path.is_empty() && !plays.iter().any(|play: &Playback| play.path == *path) {
                    plays.push(Playback {
                        owner,
                        path: path.clone(),
                        volume: sound.volume,
                    });
                }
            }
            member.inside = inside;
            member.epoch = epoch;
        }
        crossing.members.retain(|_, member| member.epoch == epoch);
        plays
    }
}

#[derive(Resource, Default)]
struct Runtime(Crossings);

pub struct SoundAreaPlugin;
impl Plugin for SoundAreaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_systems(
                PostUpdate,
                update.after(crate::native::area::ComputeAreaForces),
            )
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions),
            );
    }
}

pub(crate) use ui::controls;
pub(crate) use ui::{assign, enable, preview};

fn update(world: &mut World) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    world
        .run_system_cached(sample_crossings)
        .expect("sample sound Area crossings");
}

fn sample_crossings(
    world: &mut World,
    sources: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
    items: &mut QueryState<
        (
            Entity,
            &'static CanvasItem,
            &'static ChildOf,
            &'static WorkspaceMember,
        ),
        Without<InfluenceArea>,
    >,
    shields: &mut crate::native::topology::influence::BlockerQuery,
) {
    let _sampling = nucleus::performance::Span::new("sound-crossing-sample");
    let areas: Vec<_> = sources
        .iter(world)
        .filter(|(entity, area, parent, member)| {
            area.sound.is_some()
                && !crate::native::laboratory::blocked(world, *entity)
                && crate::native::influence_report::activity(
                    world,
                    *entity,
                    area,
                    parent.parent(),
                    member.0,
                ) == crate::native::influence_report::Outcome::Active
                && area.validate()
                && crate::native::topology::holes::live(world, parent.parent(), member.0)
        })
        .map(|(entity, _, parent, member)| (entity, parent.parent(), member.0))
        .collect();
    if areas.is_empty()
        && world
            .get_resource::<Runtime>()
            .is_none_or(|runtime| runtime.0.areas.is_empty())
    {
        return;
    }
    let active: std::collections::HashSet<_> = areas.iter().map(|(entity, ..)| *entity).collect();
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    runtime.0.areas.retain(|entity, _| {
        let retained = active.contains(entity);
        if !retained {
            let _ = crate::native::sound::send(world, crate::native::sound::Command::Stop(*entity));
        }
        retained
    });
    if areas.is_empty() {
        world.insert_resource(runtime);
        return;
    }
    let mut scopes: HashMap<(Entity, u64), Vec<(Entity, CanvasItem)>> = HashMap::new();
    for (_, root, workspace) in &areas {
        scopes.entry((*root, *workspace)).or_default();
    }
    let mut prepared = 0;
    for (entity, item, parent, member) in items.iter(world) {
        if let Some(scope) = scopes.get_mut(&(parent.parent(), member.0)) {
            scope.push((entity, *item));
            prepared += 1;
        }
    }
    nucleus::performance::add("sound-crossing-items-prepared", prepared);
    let mut evaluated = 0;
    let mut requested = 0;
    for (entity, root, workspace) in areas {
        let plays = {
            let area = world.get::<InfluenceArea>(entity).unwrap();
            let sound = area.sound.as_ref().unwrap();
            let spatial = world
                .get::<crate::native::topology::presentation::SpatialRoot>(root)
                .is_some();
            let placement = crate::native::topology::spatial(world, entity);
            let filter = world.get::<crate::native::protein_area::filter::Matches>(entity);
            let blockers = crate::native::topology::influence::Blockers::prepare(
                world, shields, root, workspace, entity, false,
            );
            let members = scopes[&(root, workspace)]
                .iter()
                .filter_map(|(sand, item)| {
                    evaluated += 1;
                    let record = world.get::<crate::native::area::RecordProperties>(*sand);
                    let binding = world.get::<crate::native::protein_area::RecordBinding>(*sand);
                    let point = crate::native::topology::participation_point(world, *sand, item);
                    let matches = if area.filter.is_some() {
                        record.is_some_and(|record| {
                            filter.is_some_and(|filter| filter.allows(record, binding))
                        })
                    } else {
                        area.rules.is_empty() || record.is_some_and(|record| area.matches(record))
                    };
                    let inside = matches
                        && if spatial {
                            crate::native::topology::influence::contains(area, placement, point)
                        } else {
                            area.contains(item.position)
                        };
                    (!blockers.blocks(point, record, binding)).then_some((*sand, inside))
                });
            runtime.0.sample(entity, sound, members)
        };
        requested += plays.len() as u64;
        for play in plays {
            let result = crate::native::sound::send(
                world,
                crate::native::sound::Command::Play {
                    owner: play.owner,
                    path: play.path,
                    effects: None,
                    volume: play.volume,
                },
            );
            if let Err(error) = result
                && world
                    .get::<SoundStatus>(entity)
                    .is_none_or(|status| status.0 != error)
            {
                world.entity_mut(entity).insert(SoundStatus(error));
            }
        }
    }
    nucleus::performance::add("sound-crossing-members-evaluated", evaluated);
    nucleus::performance::add("sound-crossing-playback-requests", requested);
    world.insert_resource(runtime);
}
