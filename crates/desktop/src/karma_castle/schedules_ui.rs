use bevy::{prelude::*, text::EditableText};
use model::schedules::{DateKind, Draft};
use nucleus::karma::rule_field::RuleFieldKind;

use super::*;

#[derive(Component)]
struct ScheduleView {
    panel: Entity,
    rows: Vec<Value>,
    dirty: bool,
    pending: Option<(String, Operation)>,
    dates: [Vec<Value>; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Read,
    Write,
    Resolve(usize),
}

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Clone)]
enum Command {
    New(bool),
    Close,
    Save,
    Refresh,
    Edit(String),
    Cancel(String, i64),
    Retry(String, i64, String),
    Mode(usize, DateKind),
    Gap(usize),
    Fold(usize),
    Rule(String),
    Resolve(usize),
    SelectDate(usize, nucleus::karma::scheduled_change::DateInput),
    Consequence(usize, String),
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let panel = ui::stack(world, parent);
    world.entity_mut(owner).insert(ScheduleView {
        panel,
        rows: Vec::new(),
        dirty: true,
        pending: None,
        dates: Default::default(),
    });
    render(world, owner);
}

pub(super) fn dirty(world: &mut World, owner: Entity) {
    if let Some(mut view) = world.get_mut::<ScheduleView>(owner) {
        view.dirty = true;
    }
}

fn capture(world: &mut World, owner: Entity) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    let Some(mut castle) = world.get_mut::<KarmaCastle>(owner) else {
        return;
    };
    let Some(draft) = castle.schedule.as_mut() else {
        return;
    };
    for (index, value) in values {
        match index {
            0 => draft.name = value,
            1 | 4 => draft.dates[(index - 1) / 3].value = value,
            2 | 5 => draft.dates[(index - 2) / 3].timezone = value,
            3 | 6 => draft.consequences[(index - 3) / 3] = value,
            _ => {}
        }
    }
}

fn submit(world: &mut World, owner: Entity, action: engine::actions::Action, operation: Operation) {
    let id = nucleus::new_uid("schedule-ui");
    match send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<ScheduleView>(owner).unwrap().pending = Some((id, operation));
            if operation != Operation::Read {
                status(world, owner, "Saving the schedule…");
            }
        }
        Err(error) => {
            if operation != Operation::Read {
                status(world, owner, error);
            }
        }
    }
}

impl crate::actions::Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner)
            || world
                .get::<ScheduleView>(owner)
                .is_none_or(|view| view.pending.is_some())
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::New(range) => {
                world.get_mut::<KarmaCastle>(owner).unwrap().schedule = Some(Draft {
                    range: *range,
                    ..Default::default()
                })
            }
            Self::Close => world.get_mut::<KarmaCastle>(owner).unwrap().schedule = None,
            Self::Refresh => {
                dirty(world, owner);
                return;
            }
            Self::Edit(uid) => {
                let value = world
                    .get::<ScheduleView>(owner)
                    .unwrap()
                    .rows
                    .iter()
                    .find(|row| row["uid"] == *uid)
                    .cloned();
                let Some(value) = value else { return };
                match Draft::from_value(&value) {
                    Ok(draft) => {
                        world.get_mut::<KarmaCastle>(owner).unwrap().schedule = Some(draft)
                    }
                    Err(error) => status(world, owner, error),
                }
            }
            Self::Save => {
                let Some(draft) = world.get::<KarmaCastle>(owner).unwrap().schedule.clone() else {
                    return;
                };
                match draft.inputs() {
                    Ok(boundaries) => submit(
                        world,
                        owner,
                        engine::actions::Action::SaveKarmaSchedule {
                            schedule: draft.uid,
                            expected_revision: draft.revision,
                            name: draft.name,
                            boundaries,
                            request_id: nucleus::new_uid("schedule-save"),
                        },
                        Operation::Write,
                    ),
                    Err(error) => status(world, owner, error),
                }
                return;
            }
            Self::Cancel(uid, revision) => {
                submit(
                    world,
                    owner,
                    engine::actions::Action::CancelKarmaSchedule {
                        schedule: uid.clone(),
                        expected_revision: *revision,
                        request_id: nucleus::new_uid("schedule-cancel"),
                    },
                    Operation::Write,
                );
                return;
            }
            Self::Retry(uid, revision, boundary) => {
                submit(
                    world,
                    owner,
                    engine::actions::Action::RetryKarmaSchedule {
                        schedule: uid.clone(),
                        expected_revision: *revision,
                        boundary: boundary.clone(),
                        request_id: nucleus::new_uid("schedule-retry"),
                    },
                    Operation::Write,
                );
                return;
            }
            Self::Mode(index, mode) => {
                if let Some(draft) = world
                    .get_mut::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_mut()
                {
                    draft.dates[*index].kind = *mode;
                }
            }
            Self::Gap(index) => {
                if let Some(draft) = world
                    .get_mut::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_mut()
                {
                    let gap = &mut draft.dates[*index].gap;
                    *gap = if *gap == nucleus::karma::GapPolicy::Pause {
                        nucleus::karma::GapPolicy::ShiftForward
                    } else {
                        nucleus::karma::GapPolicy::Pause
                    };
                }
            }
            Self::Fold(index) => {
                if let Some(draft) = world
                    .get_mut::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_mut()
                {
                    let fold = &mut draft.dates[*index].fold;
                    *fold = if *fold == nucleus::karma::FoldPolicy::First {
                        nucleus::karma::FoldPolicy::Second
                    } else {
                        nucleus::karma::FoldPolicy::First
                    };
                }
            }
            Self::Rule(uid) => {
                ui::Command::EditCell(uid.clone(), 2).apply(world, owner);
                return;
            }
            Self::Consequence(index, source) => {
                if let Some(draft) = world
                    .get_mut::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_mut()
                {
                    draft.consequences[*index] = source.clone();
                }
            }
            Self::Resolve(index) => {
                let Some(draft) = world.get::<KarmaCastle>(owner).unwrap().schedule.as_ref() else {
                    return;
                };
                match draft.dates[*index].input() {
                    Ok(nucleus::karma::scheduled_change::DateInput::Local {
                        date,
                        timezone,
                        gap,
                        fold,
                        ..
                    }) => submit(
                        world,
                        owner,
                        engine::actions::Action::PreviewKarmaScheduleDates {
                            date,
                            timezone,
                            gap,
                            fold,
                        },
                        Operation::Resolve(*index),
                    ),
                    Ok(_) => {}
                    Err(error) => status(world, owner, error),
                }
                return;
            }
            Self::SelectDate(index, date) => {
                let matches = world
                    .get::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_ref()
                    .is_some_and(|draft| same_local(&draft.dates[*index], date));
                if !matches {
                    status(world, owner, "The date changed; resolve it again");
                    return;
                }
                world
                    .get_mut::<KarmaCastle>(owner)
                    .unwrap()
                    .schedule
                    .as_mut()
                    .unwrap()
                    .dates[*index] = model::schedules::DateDraft::from_input(date);
                world.get_mut::<ScheduleView>(owner).unwrap().dates[*index].clear();
            }
        }
        render(world, owner);
    }
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) {
    crate::castle_feed::button(world, parent, owner, label, command);
}

fn input(world: &mut World, parent: Entity, owner: Entity, index: usize, label: &str, value: &str) {
    crate::edit_mode::label(world, parent, label, 13.0);
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .insert((
            ChildOf(parent),
            Input { owner, index },
            Node {
                width: percent(100),
                min_height: px(30),
                ..Default::default()
            },
        ))
        .id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    text.max_characters = Some(if index == 3 || index == 6 {
        16_384
    } else {
        256
    });
    let mut accessible = accesskit::Node::new(accesskit::Role::TextInput);
    accessible.set_label(label);
    world
        .entity_mut(entity)
        .insert(bevy::a11y::AccessibilityNode(accessible));
}

fn render(world: &mut World, owner: Entity) {
    let panel = world.get::<ScheduleView>(owner).unwrap().panel;
    if let Some(children) = world.get::<Children>(panel) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.entity_mut(child).despawn();
        }
    }
    let line = ui::row(world, panel);
    crate::edit_mode::label(world, line, "Scheduled changes", 18.0);
    button(world, line, owner, "One change", Command::New(false));
    button(world, line, owner, "Start and end", Command::New(true));
    button(world, line, owner, "Refresh outcomes", Command::Refresh);
    if let Some(draft) = world.get::<KarmaCastle>(owner).unwrap().schedule.clone() {
        let form = ui::stack(world, panel);
        input(world, form, owner, 0, "Schedule name", &draft.name);
        for index in 0..if draft.range { 2 } else { 1 } {
            crate::edit_mode::label(
                world,
                form,
                if index == 0 {
                    "Change / start"
                } else {
                    "End (the example sets the current quantity to zero)"
                },
                15.0,
            );
            let line = ui::row(world, form);
            for (label, kind) in [
                ("After a duration", DateKind::After),
                ("Date with offset", DateKind::Instant),
                ("Local date", DateKind::Local),
            ] {
                button(world, line, owner, label, Command::Mode(index, kind));
            }
            let date = &draft.dates[index];
            input(
                world,
                form,
                owner,
                1 + index * 3,
                match date.kind {
                    DateKind::After => "Duration from saving (for example 3d)",
                    DateKind::Instant => "Date and time with UTC offset",
                    DateKind::Local => "Local date and time",
                },
                &date.value,
            );
            if date.kind == DateKind::Local {
                input(
                    world,
                    form,
                    owner,
                    2 + index * 3,
                    "Timezone",
                    &date.timezone,
                );
                let line = ui::row(world, form);
                button(
                    world,
                    line,
                    owner,
                    if date.gap == nucleus::karma::GapPolicy::Pause {
                        "Missing time: refuse"
                    } else {
                        "Missing time: shift forward"
                    },
                    Command::Gap(index),
                );
                button(
                    world,
                    line,
                    owner,
                    if date.fold == nucleus::karma::FoldPolicy::First {
                        "Repeated time: first"
                    } else {
                        "Repeated time: second"
                    },
                    Command::Fold(index),
                );
                button(
                    world,
                    line,
                    owner,
                    "Resolve local date",
                    Command::Resolve(index),
                );
                let choices = world.get::<ScheduleView>(owner).unwrap().dates[index].clone();
                for choice in choices {
                    let Ok(input) = serde_json::from_value(choice["date"].clone()) else {
                        continue;
                    };
                    let label = choice["at_ms"]
                        .as_i64()
                        .and_then(chrono::DateTime::from_timestamp_millis)
                        .map_or_else(|| "Unknown date".into(), |date| date.to_rfc3339());
                    button(
                        world,
                        form,
                        owner,
                        &format!(
                            "Use {label} · timezone rules {}",
                            choice["rules"].as_str().unwrap_or_default()
                        ),
                        Command::SelectDate(index, input),
                    );
                }
            }
            input(
                world,
                form,
                owner,
                3 + index * 3,
                "Consequence (for example @room = -1)",
                &draft.consequences[index],
            );
            let view = world.get::<View>(owner).unwrap();
            let choices = model::transfers::elements(
                RuleFieldKind::Consequence,
                &view.transfers,
                view.acting_person.as_deref(),
            );
            let rows = view.transfers.clone();
            for source in choices {
                button(
                    world,
                    form,
                    owner,
                    &model::transfers::label(&source, &rows),
                    Command::Consequence(index, source),
                );
            }
        }
        let line = ui::row(world, form);
        button(world, line, owner, "Save schedule", Command::Save);
        button(world, line, owner, "Close editor", Command::Close);
        crate::edit_mode::label(
            world,
            form,
            "Dates and outcomes stay saved when the forecast cache is rebuilt. Cancelling future work leaves the current quantity as it is.",
            13.0,
        );
    }
    let rows = world.get::<ScheduleView>(owner).unwrap().rows.clone();
    for value in rows {
        let Some(uid) = value["uid"].as_str() else {
            continue;
        };
        let revision = value["revision"].as_i64().unwrap_or(0);
        let group = ui::stack(world, panel);
        let line = ui::row(world, group);
        crate::edit_mode::label(
            world,
            line,
            &format!(
                "{} · revision {revision}{}",
                value["name"].as_str().unwrap_or("Schedule"),
                if value["cancelled"] == true {
                    " · cancelled"
                } else {
                    ""
                }
            ),
            15.0,
        );
        if value["cancelled"] != true {
            button(
                world,
                line,
                owner,
                "Edit dates and changes",
                Command::Edit(uid.into()),
            );
            button(
                world,
                line,
                owner,
                "Cancel future changes",
                Command::Cancel(uid.into(), revision),
            );
        }
        if let Some(boundaries) = value["boundaries"].as_array() {
            for boundary in boundaries.iter().rev().take(50).rev() {
                let input = &boundary["input"];
                let at = boundary["intended_at_ms"]
                    .as_i64()
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map_or_else(|| "Unknown date".into(), |date| date.to_rfc3339());
                let line = ui::row(world, group);
                crate::edit_mode::label(
                    world,
                    line,
                    &format!(
                        "{} · {at} · {}{}",
                        input["purpose"].as_str().unwrap_or("change"),
                        boundary["status"].as_str().unwrap_or("unknown"),
                        boundary["reason"]
                            .as_str()
                            .map_or(String::new(), |reason| format!(" · {reason}"))
                    ),
                    13.0,
                );
                if let Some(rule) = boundary["rule"].as_str() {
                    button(world, line, owner, "Open Rule", Command::Rule(rule.into()));
                }
                if boundary["current"] == true
                    && matches!(boundary["status"].as_str(), Some("failed" | "blocked"))
                {
                    if let Some(boundary_uid) = boundary["uid"].as_str() {
                        button(
                            world,
                            line,
                            owner,
                            "Retry original occurrence",
                            Command::Retry(uid.into(), revision, boundary_uid.into()),
                        );
                    }
                }
            }
            if boundaries.len() > 50 {
                crate::edit_mode::label(
                    world,
                    group,
                    "Showing the latest 50 boundary outcomes.",
                    13.0,
                );
            }
        }
    }
}

pub(super) fn maintain(world: &mut World) {
    let editors: Vec<_> = world
        .query_filtered::<Entity, With<ScheduleView>>()
        .iter(world)
        .collect();
    for owner in editors {
        capture(world, owner);
    }
    let owners: Vec<_> = world
        .query::<(Entity, &ScheduleView)>()
        .iter(world)
        .filter(|(_, view)| view.dirty && view.pending.is_none())
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        if world.get_non_send::<CellBridge>().is_none() || crate::laboratory::active(world) {
            continue;
        }
        submit(
            world,
            owner,
            engine::actions::Action::InspectKarmaSchedules { schedule: None },
            Operation::Read,
        );
        if world.get::<ScheduleView>(owner).unwrap().pending.is_some() {
            world.get_mut::<ScheduleView>(owner).unwrap().dirty = false;
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &ScheduleView)>()
        .iter(world)
        .find(|(_, view)| {
            view.pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
        })
        .map(|(owner, _)| owner);
    let Some(owner) = owner else { return false };
    let (_, operation) = world
        .get_mut::<ScheduleView>(owner)
        .unwrap()
        .pending
        .take()
        .unwrap();
    match message {
        ServerMessage::ActionOk { data, .. } => {
            if operation == Operation::Read {
                capture(world, owner);
                let rows = data
                    .as_ref()
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if world.get::<ScheduleView>(owner).unwrap().rows == rows {
                    return true;
                }
                world.get_mut::<ScheduleView>(owner).unwrap().rows = rows;
            } else if let Operation::Resolve(index) = operation {
                capture(world, owner);
                let choices = data
                    .as_ref()
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if choices.len() == 1 {
                    if let Ok(input) = serde_json::from_value(choices[0]["date"].clone()) {
                        if !world
                            .get::<KarmaCastle>(owner)
                            .unwrap()
                            .schedule
                            .as_ref()
                            .is_some_and(|draft| same_local(&draft.dates[index], &input))
                        {
                            status(world, owner, "The date changed; resolve it again");
                            return true;
                        }
                        Command::SelectDate(index, input).apply(world, owner);
                    }
                    status(
                        world,
                        owner,
                        "Local date resolved with the installed timezone rules",
                    );
                    return true;
                }
                world.get_mut::<ScheduleView>(owner).unwrap().dates[index] = choices;
                status(world, owner, "Choose the resolved date and timezone rules");
            } else {
                world.get_mut::<KarmaCastle>(owner).unwrap().schedule = None;
                dirty(world, owner);
                status(world, owner, "Schedule saved; outcomes will update here");
            }
            render(world, owner);
        }
        ServerMessage::Error { message, .. } => status(world, owner, message.clone()),
        _ => {}
    }
    true
}

fn same_local(
    draft: &model::schedules::DateDraft,
    input: &nucleus::karma::scheduled_change::DateInput,
) -> bool {
    let Ok(mut current) = draft.input() else {
        return false;
    };
    if let (
        nucleus::karma::scheduled_change::DateInput::Local { tzdb, .. },
        nucleus::karma::scheduled_change::DateInput::Local { tzdb: selected, .. },
    ) = (&mut current, input)
    {
        *tzdb = selected.clone();
    }
    current == *input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_scheduling_uses_transfer_templates_and_preserves_the_other_boundary() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let owner = super::super::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle::default(),
        );
        let transfer = "r_K7T3ZG08G5EBWRSTF8NBWMTZYV";
        let person = "r_K7T3ZG08G5EBWRSTF8NBWMTZYW";
        let state = nucleus::transfer::karma::Snapshot {
            transfer: transfer.into(),
            revision: 1,
            active: false,
            published: false,
            ready: false,
            participants: std::collections::BTreeMap::from([(
                person.into(),
                nucleus::transfer::karma::Participant {
                    guard: nucleus::transfer::AgreementGuard {
                        level: 0,
                        change_uid: None,
                    },
                    changed_at_ms: None,
                },
            )]),
        };
        {
            let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
            view.transfers =
                vec![serde_json::json!({"uid":transfer,"head":"Trade","karma_state":state})];
            view.acting_person = Some(person.into());
        }
        Command::New(true).apply(app.world_mut(), owner);
        let source = format!("@{transfer}: agreement(@{person}, 0)");
        Command::Consequence(0, source.clone()).apply(app.world_mut(), owner);
        let draft = app
            .world()
            .get::<KarmaCastle>(owner)
            .unwrap()
            .schedule
            .as_ref()
            .unwrap();
        assert_eq!(draft.consequences, [source, "@room = 0".into()]);
        let boundaries = draft.inputs().unwrap();
        assert_eq!(boundaries[0].target, transfer);
        assert!(
            matches!(&boundaries[0].consequences[0], nucleus::karma::Consequence::SetTransferAgreement { person: acting, level: Some(level), .. } if acting == person && *level == store::exact::zero())
        );
        assert_eq!(boundaries[1].target, "room");
    }

    #[test]
    fn native_range_editing_captures_both_dates_and_explicit_zero_without_saving() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let owner = super::super::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle::default(),
        );
        Command::New(true).apply(app.world_mut(), owner);
        let inputs: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .map(|(entity, input)| (entity, input.index))
            .collect();
        for (entity, index) in inputs {
            let text = match index {
                0 => "Room range",
                1 => "2d",
                3 => "@room = -5",
                4 => "4d",
                6 => "@room = 0",
                _ => continue,
            };
            app.world_mut()
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(text);
        }
        maintain(app.world_mut());
        let draft = app
            .world()
            .get::<KarmaCastle>(owner)
            .unwrap()
            .schedule
            .as_ref()
            .unwrap();
        let boundaries = draft.inputs().unwrap();
        assert_eq!(draft.name, "Room range");
        assert_eq!(boundaries.len(), 2);
        assert_eq!(
            boundaries[0].date,
            nucleus::karma::scheduled_change::DateInput::After {
                milliseconds: 172_800_000
            }
        );
        assert_eq!(
            boundaries[1].consequences,
            vec![nucleus::karma::Consequence::SetQuantity {
                value: Some(nucleus::DecimalValue::parse_inferred("0").unwrap())
            }]
        );
        Command::Save.apply(app.world_mut(), owner);
        assert!(
            app.world()
                .get::<ScheduleView>(owner)
                .unwrap()
                .pending
                .is_none()
        );
        assert!(
            app.world()
                .get::<KarmaCastle>(owner)
                .unwrap()
                .schedule
                .is_some()
        );
    }
}
