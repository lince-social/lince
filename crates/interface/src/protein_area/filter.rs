use super::*;
use crate::{
    actions::{Action, ActionButton},
    area::RecordProperties,
    icons::{Icon, IconButton, Tooltip},
    workspace::WorkspaceMember,
};
use std::collections::HashSet;

#[derive(Component)]
pub(crate) struct Subscription(pub Entity);

#[derive(Component, Clone, PartialEq, Eq)]
pub(crate) struct Matches {
    pub source: Source,
    pub uids: HashSet<String>,
    pub current: bool,
}

impl Matches {
    pub(crate) fn allows(
        &self,
        record: &RecordProperties,
        binding: Option<&RecordBinding>,
    ) -> bool {
        self.current
            && binding.map_or(&Source::Local, |binding| &binding.source) == &self.source
            && record.0["uid"]
                .as_str()
                .is_some_and(|uid| self.uids.contains(uid))
    }
}

pub(super) fn query(config: &Config) -> Result<protein::Protein, String> {
    if !config.valid() || config.draft.query["source"] != "record" {
        return Err("Area filters need a Record query".into());
    }
    let mut draft = config.draft.clone();
    draft.query["include"] = serde_json::json!({});
    draft.query["fields"] = serde_json::json!(["uid"]);
    draft.query["aggregate"] = Value::Null;
    draft.query["limit"] = Value::Null;
    draft.compile()
}

fn ensure(world: &mut World, owner: Entity) -> Option<Entity> {
    if let Some(entity) = world
        .query::<(Entity, &Subscription)>()
        .iter(world)
        .find(|(_, s)| s.0 == owner)
        .map(|(e, _)| e)
    {
        return Some(entity);
    }
    let root = world.get::<ChildOf>(owner)?.parent();
    let workspace = *world.get::<WorkspaceMember>(owner)?;
    Some(
        world
            .spawn((Subscription(owner), ChildOf(root), workspace))
            .id(),
    )
}

pub(super) fn maintain(world: &mut World) {
    let subscriptions: Vec<_> = world
        .query::<(Entity, &Subscription)>()
        .iter(world)
        .map(|(e, s)| (e, s.0))
        .collect();
    for (entity, owner) in subscriptions {
        let valid = world
            .get::<InfluenceArea>(owner)
            .is_some_and(|a| a.filter.is_some());
        if !valid {
            stop(world, entity);
            let editors: Vec<_> = world
                .query::<(Entity, &QueryEditor)>()
                .iter(world)
                .filter(|(_, q)| q.0 == entity)
                .map(|(e, _)| e)
                .collect();
            for editor in editors {
                world.despawn(editor);
            }
            world.despawn(entity);
            if world.get_entity(owner).is_ok() {
                world.entity_mut(owner).remove::<Matches>();
            }
        } else if let (Some(root), Some(member)) = (
            world.get::<ChildOf>(owner).map(ChildOf::parent),
            world.get::<WorkspaceMember>(owner).copied(),
        ) {
            if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(root)
                || world.get::<WorkspaceMember>(entity) != Some(&member)
            {
                world.entity_mut(entity).insert((ChildOf(root), member));
            }
        }
    }
    let owners: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter(|(_, a)| a.filter.is_some())
        .map(|(e, _)| e)
        .collect();
    for owner in owners {
        ensure(world, owner);
    }
}

pub(super) fn publish(world: &mut World) {
    let subscriptions: Vec<_> = world
        .query::<(Entity, &Subscription)>()
        .iter(world)
        .map(|(e, s)| (e, s.0))
        .collect();
    for (entity, owner) in subscriptions {
        let Some(config) = configuration(world, entity) else {
            continue;
        };
        let Some(state) = world.resource::<Runtime>().areas.get(&entity) else {
            continue;
        };
        let current =
            state.ready && config.enabled && state.applied.as_ref().is_some_and(|c| c.enabled);
        if !state.dirty
            && world.get::<Matches>(owner).is_some_and(|matches| {
                matches.current == current && matches.source == config.source
            })
        {
            continue;
        }
        let matches = Matches {
            source: config.source,
            current,
            uids: state
                .data
                .iter()
                .filter_map(|row| row["uid"].as_str().map(str::to_string))
                .collect(),
        };
        if world.get::<Matches>(owner) != Some(&matches) {
            if !matches.current && crate::area_mutation::armed(world, owner) {
                crate::area_mutation::disarm(
                    world,
                    owner,
                    "Disarmed because the Protein filter stopped. Run the filter before arming again.",
                );
            }
            world.entity_mut(owner).insert(matches);
        }
        world
            .resource_mut::<Runtime>()
            .areas
            .get_mut(&entity)
            .unwrap()
            .dirty = false;
    }
}

#[derive(Clone)]
pub(crate) struct Enable;
impl Action for Enable {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            return;
        };
        if !crate::area_panel::owns(world, root, owner)
            || !world
                .get::<crate::edit_mode::EditMode>(root)
                .is_some_and(|mode| mode.enabled && mode.areas)
        {
            return;
        }
        let mut config = Config {
            bindings: Vec::new(),
            ..default()
        };
        config.draft.name = "Area filter".into();
        config.draft.query["limit"] = Value::Null;
        world.get_mut::<InfluenceArea>(owner).unwrap().filter = Some(config);
        ensure(world, owner);
        crate::edit_mode::render_panel(world, root);
    }
}

pub(crate) fn controls(world: &mut World, panel: Entity, owner: Entity) {
    if world
        .get::<InfluenceArea>(owner)
        .is_some_and(|area| area.filter.is_some())
    {
        if let Some(entity) = ensure(world, owner) {
            ui::controls(world, owner, panel, entity);
        }
    } else {
        let title = crate::edit_mode::label(world, panel, "Protein filter", 18.0);
        world.entity_mut(title).insert(Tooltip("Choose existing Record Sands using Protein conditions and sorting rules. Replaces the simple matching rules for Area effects. It does not spawn or move Sands by itself.".into()));
        world.spawn((
            crate::sand::Square,
            IconButton::new(Icon::Plus, "Use a Protein filter for existing Sands"),
            ActionButton::new(owner, crate::actions![Enable]),
            ChildOf(panel),
        ));
    }
}

pub(crate) fn editor(world: &World, entity: Entity) -> bool {
    world
        .get::<QueryEditor>(entity)
        .is_some_and(|link| world.get::<Subscription>(link.0).is_some())
}
