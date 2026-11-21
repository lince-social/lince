use std::time::Instant;

use bevy::prelude::*;
use nucleus::visibility::{Command, Context};
use serde::Deserialize;
use serde_json::Value;

use super::{
    Intent, Panel, Request,
    draft::{BoundEnd, Rule, Side, policy, read_fields},
    render::draw,
};

pub(super) fn request(world: &mut World, panel: &mut Panel, command: Command) {
    if !world.contains_resource::<Messages<Request>>() {
        return;
    }
    let id = nucleus::new_uid("visibility-ui");
    world.write_message(Request {
        owner: panel.owner,
        id: id.clone(),
        command,
    });
    panel.pending = Some((id, Instant::now()));
    panel.notice = "Loading visibility…".into();
}

pub(super) fn apply(world: &mut World, entity: Entity, intent: Intent) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    read_fields(world, &mut panel);
    if panel.pending.is_some()
        || panel.draft.is_some()
            && matches!(
                &intent,
                Intent::Save
                    | Intent::Refresh
                    | Intent::ChooseRecord
                    | Intent::Record(_)
                    | Intent::Data(_)
            )
    {
        world.entity_mut(entity).insert(panel);
        return;
    }
    panel.fields.clear();
    panel.radius_fields.clear();
    fn rules(panel: &mut Panel, side: Side) -> &mut Vec<Rule> {
        match side {
            Side::Include => &mut panel.include,
            Side::Exclude => &mut panel.exclude,
        }
    }
    panel.prepared_policy.take();
    match intent {
        Intent::ChooseRecord => panel.choosing_record = !panel.choosing_record,
        Intent::Record(record) => {
            panel.record = record;
            panel.context = None;
            panel.include.clear();
            panel.exclude.clear();
            panel.choosing_record = false;
            panel.choosing_organs = None;
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data: panel.data,
            };
            request(world, &mut panel, command);
        }
        Intent::Data(data) => {
            panel.data = data;
            panel.context = None;
            panel.include.clear();
            panel.exclude.clear();
            panel.choosing_organs = None;
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data,
            };
            request(world, &mut panel, command);
        }
        Intent::Add(side) => {
            if panel.include.len() + panel.exclude.len() < nucleus::visibility::MAX_CONDITIONS {
                rules(&mut panel, side).push(Rule {
                    lower_inclusive: true,
                    upper_inclusive: true,
                    ..Default::default()
                });
            } else {
                panel.notice = "At most 64 conditions are allowed".into();
            }
        }
        Intent::Remove(side, index) => {
            panel.choosing_organs = None;
            if index < rules(&mut panel, side).len() {
                rules(&mut panel, side).remove(index);
            }
        }
        Intent::Organ(side, index, organ) => {
            if let Some(rule) = rules(&mut panel, side).get_mut(index) {
                if rule.organs.contains(&organ) {
                    rule.organs.retain(|uid| uid != &organ);
                } else if rule.organs.len() < nucleus::visibility::MAX_ORGANS_PER_CONDITION {
                    rule.organs.push(organ);
                }
            }
        }
        Intent::Person(side, index, person) => {
            if let Some(rule) = rules(&mut panel, side).get_mut(index) {
                if rule.persons.contains(&person) {
                    rule.persons.retain(|uid| uid != &person);
                } else if rule.persons.len() < nucleus::visibility::MAX_ORGANS_PER_CONDITION {
                    rule.persons.push(person);
                }
            }
        }
        Intent::ChooseOrgans(side, index) => {
            panel.choosing_organs = if panel.choosing_organs == Some((side, index)) {
                None
            } else {
                Some((side, index))
            };
        }
        Intent::Inclusive(side, index, end) => {
            if let Some(rule) = rules(&mut panel, side).get_mut(index) {
                let inclusive = match end {
                    BoundEnd::Lower => &mut rule.lower_inclusive,
                    BoundEnd::Upper => &mut rule.upper_inclusive,
                };
                *inclusive = !*inclusive;
            }
        }
        Intent::Save => match policy(&panel) {
            Ok(policy) => {
                if let Some(context) = panel.context.as_ref().filter(|context| {
                    !panel.record.is_empty()
                        && context.record_uid == panel.record
                        && context.data == panel.data
                }) {
                    let expected_revision =
                        context.saved.as_ref().map_or(0, |saved| saved.revision);
                    let command = Command::Save {
                        record_uid: panel.record.clone(),
                        data: panel.data,
                        policy,
                        expected_revision,
                    };
                    request(world, &mut panel, command);
                } else {
                    panel.notice = "Load this Record's visibility before saving".into();
                }
            }
            Err(error) => panel.notice = error,
        },
        Intent::Refresh => {
            let command = Command::Context {
                record_uid: panel.record.clone(),
                data: panel.data,
            };
            request(world, &mut panel, command);
        }
        Intent::Preview => {
            panel.notice =
                "Preview reflects the edited rules. Save to apply them to future deliveries.".into()
        }
        Intent::Credits => panel.show_credits = !panel.show_credits,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

pub fn receive(world: &mut World, id: &str, result: Result<Value, String>) {
    receive_with(world, id, || {
        result.and_then(|value| {
            serde_json::from_value::<Context>(value).map_err(|error| error.to_string())
        })
    });
}

pub fn receive_ref(world: &mut World, id: &str, result: Result<&Value, &str>) {
    receive_with(world, id, || {
        result
            .map_err(str::to_owned)
            .and_then(|value| Context::deserialize(value).map_err(|error| error.to_string()))
    });
}

fn receive_with(world: &mut World, id: &str, decode: impl FnOnce() -> Result<Context, String>) {
    let entity = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| {
            panel
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(entity, _)| entity);
    let Some(entity) = entity else {
        return;
    };
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    let _reply = nucleus::performance::Span::new("visibility-reply-application");
    nucleus::performance::add("visibility-replies-accepted", 1);
    panel.pending = None;
    match decode() {
        Ok(context) => {
            panel.prepared_policy.take();
            panel.record = context.record_uid.clone();
            panel.data = context.data;
            if let Some(saved) = &context.saved {
                panel.include = saved.policy.include.iter().map(Rule::from).collect();
                panel.exclude = saved.policy.exclude.iter().map(Rule::from).collect();
            } else {
                panel.include.clear();
                panel.exclude.clear();
            }
            panel.fields.clear();
            panel.context = Some(context);
            panel.notice =
                "Saved rules loaded. Previously delivered data cannot be recalled.".into();
        }
        Err(error) => panel.notice = error,
    }
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

pub(super) fn maintain(world: &mut World) {
    let timed_out: Vec<_> = world
        .query::<&Panel>()
        .iter(world)
        .filter_map(|panel| {
            panel
                .pending
                .as_ref()
                .filter(|(_, at)| at.elapsed().as_secs() >= 15)
                .map(|(id, _)| id.clone())
        })
        .collect();
    for id in timed_out {
        receive_ref(
            world,
            &id,
            Err("Visibility request timed out; reload before saving"),
        );
    }
}
