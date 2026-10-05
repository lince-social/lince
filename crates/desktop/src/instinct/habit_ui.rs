use std::collections::HashMap;

use bevy::{math::DVec2, prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use engine::karma_habits::{Imported, Input, Kind, Preview};
use nucleus::karma::{FoldPolicy, GapPolicy, TimeZoneId};

#[derive(Clone, PartialEq, Eq)]
struct Form {
    time: String,
    timezone: String,
    gap: GapPolicy,
    fold: FoldPolicy,
}

impl Default for Form {
    fn default() -> Self {
        let input = Input::default();
        Self {
            time: input.time,
            timezone: input.timezone.as_str().into(),
            gap: input.gap,
            fold: input.fold,
        }
    }
}

impl Form {
    fn input(&self) -> Result<Input, String> {
        Ok(Input {
            time: self.time.trim().into(),
            timezone: TimeZoneId::new(self.timezone.trim()).map_err(|error| error.to_string())?,
            gap: self.gap,
            fold: self.fold,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Preview,
    Import,
    Complete,
}

#[derive(Component, Default)]
pub(super) struct State {
    form: Form,
    preview: Option<(Form, Preview)>,
    source_changed: bool,
    record: Option<String>,
    quantity: Option<nucleus::DecimalValue>,
    pending: Option<(String, Operation, Form)>,
    subscription: Option<String>,
    requested: bool,
    output: Option<Entity>,
    import: Option<Entity>,
    complete: Option<Entity>,
    gap: Option<Entity>,
    fold: Option<Entity>,
    notice: String,
}

#[derive(Component)]
struct Field {
    owner: Entity,
    index: usize,
}

#[derive(Resource, Default)]
pub(super) struct Subscriptions(
    HashMap<String, Entity>,
    HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>,
);

#[derive(Clone, Copy)]
pub(in crate::instinct) enum Command {
    Preview,
    Import,
    Complete,
    Gap,
    Fold,
    Karma,
}

fn stack(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    world.init_resource::<Subscriptions>();
    if world.get::<State>(owner).is_none() {
        world.entity_mut(owner).insert(State::default());
    }
    let form = world.get::<State>(owner).unwrap().form.clone();
    let panel = stack(world, parent);
    crate::edit_mode::label(world, panel, "Daily Cleaning Room", 18.0);
    crate::edit_mode::label(
        world,
        panel,
        "Each day sets one Need (-1). Completing it sets zero. Preview the Record, Frequency and Rule before importing.",
        14.0,
    );
    for (index, title, value) in [
        (0, "Local time (HH:MM)", form.time),
        (1, "Timezone (UTC or an IANA name)", form.timezone),
    ] {
        let line = stack(world, panel);
        crate::edit_mode::label(world, line, title, 13.0);
        let input = world
            .spawn(crate::sand::text_editor(
                &value,
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(line),
                Field { owner, index },
                Node {
                    width: percent(100),
                    min_height: px(30),
                    ..default()
                },
            ))
            .id();
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(if index == 0 { 5 } else { 255 });
    }
    let policies = row(world, panel);
    let gap = crate::castle_feed::button(world, policies, owner, "Clock gap", Command::Gap);
    let fold = crate::castle_feed::button(world, policies, owner, "Repeated time", Command::Fold);
    let buttons = row(world, panel);
    crate::castle_feed::button(
        world,
        buttons,
        owner,
        "Preview daily task",
        Command::Preview,
    );
    let import =
        crate::castle_feed::button(world, buttons, owner, "Import daily task", Command::Import);
    let complete = crate::castle_feed::button(
        world,
        buttons,
        owner,
        "Cleaning Room completed",
        Command::Complete,
    );
    let mut accessibility = accesskit::Node::new(accesskit::Role::CheckBox);
    accessibility.set_label("Cleaning Room completed");
    world
        .entity_mut(complete)
        .insert(bevy::a11y::AccessibilityNode::from(accessibility));
    crate::castle_feed::button(world, buttons, owner, "Open Karma", Command::Karma);
    let output = crate::edit_mode::label(world, panel, "", 13.0);
    let mut state = world.get_mut::<State>(owner).unwrap();
    state.output = Some(output);
    state.import = Some(import);
    state.complete = Some(complete);
    state.gap = Some(gap);
    state.fold = Some(fold);
    show(world, owner);
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let fields: Vec<_> = world
        .query::<(&Field, &EditableText)>()
        .iter(world)
        .filter(|(field, _)| field.owner == owner)
        .map(|(field, text)| (field.index, text.value().to_string()))
        .collect();
    let Some(mut state) = world.get_mut::<State>(owner) else {
        return;
    };
    for (index, value) in fields {
        if index == 0 {
            state.form.time = value;
        } else {
            state.form.timezone = value;
        }
    }
}

fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Importing is unavailable in the Laboratory".into());
    }
    crate::practice_cells::send(world, owner, message)
}

fn submit(world: &mut World, owner: Entity, action: engine::actions::Action, operation: Operation) {
    let id = nucleus::new_uid("habit-ui");
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            let mut state = world.get_mut::<State>(owner).unwrap();
            state.pending = Some((id, operation, state.form.clone()));
            state.notice = match operation {
                Operation::Preview => "Preparing the import preview…",
                Operation::Import => "Importing the daily task…",
                Operation::Complete => "Completing Cleaning Room…",
            }
            .into();
        }
        Err(error) => world.get_mut::<State>(owner).unwrap().notice = error,
    }
}

impl crate::actions::Action for Command {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        match self {
            Self::Preview => &[Operation::PreviewHabit],
            Self::Import => &[Operation::ImportHabit],
            Self::Complete => &[Operation::CompleteHabit],
            _ => &[],
        }
    }

    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        if matches!(self, Self::Gap | Self::Fold) {
            &[lince_interface::practice::Operation::PreviewHabit]
        } else {
            &[]
        }
    }

    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        capture(world, owner);
        let Some(state) = world.get::<State>(owner) else {
            return;
        };
        if state.pending.is_some() {
            return;
        }
        let result = match self {
            Self::Preview => state.form.input().map(|input| {
                submit(
                    world,
                    owner,
                    engine::actions::Action::PreviewKarmaHabit { input },
                    Operation::Preview,
                )
            }),
            Self::Import => {
                let preview = state
                    .preview
                    .as_ref()
                    .filter(|(form, preview)| form == &state.form && preview.conflicts.is_empty())
                    .map(|(_, preview)| preview.fingerprint.clone());
                match preview {
                    Some(expected_preview) => state.form.input().map(|input| {
                        submit(
                            world,
                            owner,
                            engine::actions::Action::ImportKarmaHabit {
                                input,
                                expected_preview,
                                request_id: nucleus::new_uid("habit-import"),
                            },
                            Operation::Import,
                        )
                    }),
                    None => Err("Preview the current settings before importing".into()),
                }
            }
            Self::Complete => {
                let record = state.record.clone();
                match record {
                    Some(target) => {
                        submit(
                            world,
                            owner,
                            engine::actions::Action::SetQuantityExact {
                                target,
                                amount: "0".into(),
                            },
                            Operation::Complete,
                        );
                        Ok(())
                    }
                    None => Err("Import the daily task first".into()),
                }
            }
            Self::Gap => {
                let gap = state.form.gap;
                world.get_mut::<State>(owner).unwrap().form.gap = match gap {
                    GapPolicy::ShiftForward => GapPolicy::Skip,
                    GapPolicy::Skip => GapPolicy::Pause,
                    GapPolicy::Pause => GapPolicy::ShiftForward,
                };
                Ok(())
            }
            Self::Fold => {
                let fold = state.form.fold;
                world.get_mut::<State>(owner).unwrap().form.fold = if fold == FoldPolicy::First {
                    FoldPolicy::Second
                } else {
                    FoldPolicy::First
                };
                Ok(())
            }
            Self::Karma => {
                let mut root = owner;
                while world.get::<crate::container::BoxRoot>(root).is_none() {
                    let Some(parent) = world.get::<ChildOf>(root).map(ChildOf::parent) else {
                        break;
                    };
                    root = parent;
                }
                let workspace = world
                    .get::<crate::workspace::Workspaces>(root)
                    .map_or(1, |spaces| spaces.active);
                let position = world
                    .get::<crate::canvas::CanvasView>(root)
                    .map_or(DVec2::ZERO, |view| view.center);
                let search = state
                    .preview
                    .as_ref()
                    .and_then(|(_, preview)| {
                        preview
                            .objects
                            .iter()
                            .find(|object| object.kind == Kind::Rule)
                    })
                    .map_or_else(String::new, |object| object.slug.clone());
                crate::karma_castle::spawn(
                    world,
                    root,
                    workspace,
                    position,
                    crate::karma_castle::KarmaCastle {
                        search,
                        ..Default::default()
                    },
                );
                Ok(())
            }
        };
        if let Err(error) = result {
            world.get_mut::<State>(owner).unwrap().notice = error;
        }
        show(world, owner);
    }
}

fn enabled(world: &mut World, entity: Option<Entity>, enabled: bool) {
    let Some(entity) = entity.filter(|entity| world.get_entity(*entity).is_ok()) else {
        return;
    };
    if enabled {
        world
            .entity_mut(entity)
            .remove::<bevy::ui::InteractionDisabled>();
    } else {
        world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
}

fn button_text(world: &mut World, entity: Option<Entity>, value: &str) {
    let children = entity
        .and_then(|entity| world.get::<Children>(entity))
        .map(|children| children.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    for child in children {
        if let Some(mut text) = world.get_mut::<Text>(child) {
            text.0 = value.into();
        }
    }
}

fn show(world: &mut World, owner: Entity) {
    let Some(state) = world.get::<State>(owner) else {
        return;
    };
    let mut lines = Vec::new();
    if let Some((form, preview)) = &state.preview {
        if form != &state.form || state.source_changed {
            lines.push("The preview is out of date. Preview again before importing.".into());
        }
        if preview.imported {
            lines.push(
                "Already imported. Existing settings, quantities and pauses are kept.".into(),
            );
        }
        let utc = chrono::DateTime::from_timestamp_millis(preview.first_at_ms)
            .map_or_else(|| "unavailable".into(), |date| date.to_rfc3339());
        lines.push(format!(
            "{}: {} in {} · {}",
            if preview.imported {
                "Initial occurrence"
            } else {
                "First occurrence"
            },
            preview.first_local,
            preview.input.timezone.as_str(),
            utc
        ));
        lines.extend(preview.objects.iter().map(|object| {
            format!(
                "{}: {} (@{}){}",
                object.kind.as_str(),
                object.name,
                object.slug,
                if object.exists {
                    " · exists"
                } else {
                    " · will be created"
                }
            )
        }));
        lines.extend(preview.conflicts.clone());
    }
    if let Some(quantity) = state.quantity {
        lines.push(format!("Current task quantity: {quantity}"));
    }
    if !state.notice.is_empty() {
        lines.push(state.notice.clone());
    }
    let output = state.output;
    let import = state.import;
    let complete = state.complete;
    let gap = state.gap;
    let fold = state.fold;
    let busy = state.pending.is_some();
    let can_import = !busy
        && !state.source_changed
        && state
            .preview
            .as_ref()
            .is_some_and(|(form, preview)| form == &state.form && preview.conflicts.is_empty());
    let done = state.quantity.is_some_and(|quantity| quantity.is_zero());
    let can_complete = !busy && state.record.is_some() && state.quantity.is_some() && !done;
    let gap_text = match state.form.gap {
        GapPolicy::ShiftForward => "Clock gap: shift forward",
        GapPolicy::Skip => "Clock gap: skip",
        GapPolicy::Pause => "Clock gap: pause",
    };
    let fold_text = if state.form.fold == FoldPolicy::Second {
        "Repeated time: second"
    } else {
        "Repeated time: first"
    };
    if let Some(mut text) = output.and_then(|output| world.get_mut::<Text>(output)) {
        text.0 = lines.join("\n");
    }
    enabled(world, import, can_import);
    enabled(world, complete, can_complete);
    button_text(world, gap, gap_text);
    button_text(world, fold, fold_text);
    button_text(
        world,
        complete,
        if done {
            "☑ Cleaning Room completed"
        } else {
            "☐ Cleaning Room completed"
        },
    );
    if let Some(mut node) =
        complete.and_then(|entity| world.get_mut::<bevy::a11y::AccessibilityNode>(entity))
    {
        node.set_toggled(if done {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }
}

fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    if let ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } = message {
        if world
            .get::<State>(owner)
            .is_some_and(|state| state.subscription.as_ref() == Some(id))
        {
            let quantity = rows.first().and_then(|row| match &row["quantity"] {
                serde_json::Value::String(value) => {
                    nucleus::DecimalValue::parse_inferred(value).ok()
                }
                serde_json::Value::Number(value) => {
                    nucleus::DecimalValue::parse_inferred(&value.to_string()).ok()
                }
                _ => None,
            });
            let mut state = world.get_mut::<State>(owner).unwrap();
            if state.quantity != quantity {
                state.source_changed = true;
            }
            state.quantity = quantity;
        }
        return;
    }
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return,
    };
    let Some((pending_id, operation, form)) = world
        .get::<State>(owner)
        .and_then(|state| state.pending.clone())
    else {
        return;
    };
    if &pending_id != id {
        return;
    }
    let mut state = world.get_mut::<State>(owner).unwrap();
    state.pending = None;
    let result = match message {
        ServerMessage::Error { message, .. } => Err(message.clone()),
        ServerMessage::ActionOk { data, .. } => match operation {
            Operation::Preview => data
                .clone()
                .ok_or("No import preview returned".into())
                .and_then(|data| {
                    serde_json::from_value::<Preview>(data).map_err(|error| error.to_string())
                })
                .map(|preview| {
                    state.record = preview
                        .objects
                        .iter()
                        .find(|object| object.kind == Kind::Record && object.exists)
                        .map(|object| object.uid.clone());
                    state.quantity = preview.current_quantity;
                    state.source_changed = false;
                    state.preview = Some((form, preview));
                    state.notice.clear();
                }),
            Operation::Import => data
                .clone()
                .ok_or("No import result returned".into())
                .and_then(|data| {
                    serde_json::from_value::<Imported>(data).map_err(|error| error.to_string())
                })
                .map(|imported| {
                    state.record = imported
                        .objects
                        .iter()
                        .find(|object| object.kind == Kind::Record)
                        .map(|object| object.uid.clone());
                    state.quantity = Some(imported.quantity_at_import);
                    state.source_changed = true;
                    if let Some((_, preview)) = &mut state.preview {
                        preview.imported = true;
                        preview.objects = imported.objects;
                    }
                    state.notice = "Daily task imported. Edit its ordinary Rule in Karma.".into();
                }),
            Operation::Complete => {
                state.quantity = Some(nucleus::DecimalValue::from_mantissa(0, 0).unwrap());
                state.source_changed = true;
                state.notice = "Cleaning Room completed.".into();
                Ok(())
            }
        },
        _ => Ok(()),
    };
    if let Err(error) = result {
        state.notice = error;
    }
}

pub(super) fn active(states: Query<(), With<State>>) -> bool {
    !states.is_empty()
}

pub(super) fn tick(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<State>>()
        .iter(world)
        .collect();
    for owner in owners {
        capture(world, owner);
        for message in &messages {
            receive(world, owner, message);
        }
        let record = world.get::<State>(owner).unwrap().record.clone();
        if let Some(record) = record {
            if !world.get::<State>(owner).unwrap().requested {
                let id = format!("habit-task-{}", owner.to_bits());
                let query = protein::Protein {
                    source: protein::Source::Record,
                    filter: vec![protein::Predicate::UidEq(record)],
                    fields: Some(vec!["uid".into(), "quantity".into()]),
                    include: Default::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: Some(1),
                };
                if send(
                    world,
                    owner,
                    ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: query,
                    },
                )
                .is_ok()
                {
                    if let Some(sender) = crate::practice_cells::sender(world, owner) {
                        world
                            .resource_mut::<Subscriptions>()
                            .1
                            .insert(id.clone(), sender);
                    }
                    world
                        .resource_mut::<Subscriptions>()
                        .0
                        .insert(id.clone(), owner);
                    let mut state = world.get_mut::<State>(owner).unwrap();
                    state.subscription = Some(id);
                    state.requested = true;
                }
            }
        }
        show(world, owner);
    }
    let abandoned: Vec<_> = world
        .resource::<Subscriptions>()
        .0
        .iter()
        .filter(|(_, owner)| world.get::<State>(**owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in abandoned {
        let sender = world.resource::<Subscriptions>().1.get(&id).cloned();
        if sender.is_none_or(|sender| {
            sender.is_closed()
                || sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_ok()
        }) {
            world.resource_mut::<Subscriptions>().0.remove(&id);
            world.resource_mut::<Subscriptions>().1.remove(&id);
        }
    }
}

pub(in crate::instinct) fn previewed(world: &World, owner: Entity) -> bool {
    world.get::<State>(owner).is_some_and(|state| {
        state.pending.is_none()
            && state
                .preview
                .as_ref()
                .is_some_and(|(form, preview)| form == &state.form && preview.conflicts.is_empty())
    })
}

pub(in crate::instinct) fn imported(world: &World, owner: Entity) -> Option<String> {
    world.get::<State>(owner)?.record.clone()
}

pub(in crate::instinct) fn completed(world: &World, owner: Entity) -> bool {
    world.get::<State>(owner).is_some_and(|state| {
        state.pending.is_none()
            && state
                .quantity
                .as_ref()
                .is_some_and(|quantity| quantity.to_string() == "0")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<crate::tokens::ThemeSettings>();
        let world = app.world_mut();
        let root = world
            .spawn((
                crate::container::BoxRoot,
                crate::workspace::Workspaces::default(),
            ))
            .id();
        let owner = world.spawn(ChildOf(root)).id();
        spawn(world, owner, owner);
        (app, owner)
    }

    fn preview() -> Preview {
        let frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
            nucleus::karma::Slug::new("cleaning-room.daily").unwrap(),
            "Cleaning Room daily".into(),
            &nucleus::karma::Cadence::every(nucleus::karma::CadenceStep {
                days: 1,
                ..Default::default()
            }),
            nucleus::karma::TimestampMs::from_millis(1_893_520_800_000).unwrap(),
        )
        .unwrap();
        Preview {
            fingerprint: "preview".into(),
            input: Input::default(),
            first_local: nucleus::karma::CivilDateTime::from_naive(
                chrono::DateTime::from_timestamp_millis(1_893_520_800_000)
                    .unwrap()
                    .naive_utc(),
            )
            .unwrap(),
            first_at_ms: 1_893_520_800_000,
            frequency,
            objects: vec![engine::karma_habits::Object {
                kind: Kind::Record,
                uid: "room".into(),
                name: "Cleaning Room".into(),
                slug: "cleaning-room".into(),
                exists: false,
            }],
            imported: false,
            current_quantity: None,
            conflicts: Vec::new(),
        }
    }

    fn reply(id: &str, data: serde_json::Value) -> ServerMessage {
        ServerMessage::ActionOk {
            id: id.into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(data),
        }
    }

    #[test]
    fn a_pending_preview_keeps_its_submitted_form_and_cannot_authorize_an_edited_form() {
        let (mut app, owner) = fixture();
        let world = app.world_mut();
        let form = world.get::<State>(owner).unwrap().form.clone();
        world.get_mut::<State>(owner).unwrap().pending =
            Some(("preview".into(), Operation::Preview, form.clone()));
        let field = world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| field.index == 0)
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("09:00");
        capture(world, owner);
        receive(
            world,
            owner,
            &reply("preview", serde_json::to_value(preview()).unwrap()),
        );
        show(world, owner);
        let state = world.get::<State>(owner).unwrap();
        assert_eq!(state.form.time, "09:00");
        assert!(state.preview.as_ref().unwrap().0 == form);
        assert!(
            world
                .get::<bevy::ui::InteractionDisabled>(state.import.unwrap())
                .is_some()
        );
        assert!(
            world
                .get::<Text>(state.output.unwrap())
                .unwrap()
                .0
                .contains("out of date")
        );
    }

    #[test]
    fn import_and_record_updates_preserve_the_task_and_refresh_the_completion_checkbox() {
        let (mut app, owner) = fixture();
        let world = app.world_mut();
        let form = world.get::<State>(owner).unwrap().form.clone();
        world.get_mut::<State>(owner).unwrap().preview = Some((form.clone(), preview()));
        world.get_mut::<State>(owner).unwrap().pending =
            Some(("import".into(), Operation::Import, form));
        let mut objects = preview().objects;
        objects[0].exists = true;
        receive(
            world,
            owner,
            &reply(
                "import",
                serde_json::to_value(Imported {
                    tutorial: engine::karma_habits::TUTORIAL.into(),
                    objects,
                    quantity_at_import: nucleus::DecimalValue::from_mantissa(0, -1).unwrap(),
                })
                .unwrap(),
            ),
        );
        world.get_mut::<State>(owner).unwrap().subscription = Some("task".into());
        show(world, owner);
        let complete = world.get::<State>(owner).unwrap().complete.unwrap();
        assert!(
            world
                .get::<bevy::ui::InteractionDisabled>(complete)
                .is_none()
        );
        assert_eq!(
            world
                .get::<bevy::a11y::AccessibilityNode>(complete)
                .unwrap()
                .toggled(),
            Some(accesskit::Toggled::False)
        );
        receive(
            world,
            owner,
            &ServerMessage::Update {
                id: "task".into(),
                rows: vec![serde_json::json!({"quantity":"0"})],
            },
        );
        show(world, owner);
        assert!(
            world
                .get::<bevy::ui::InteractionDisabled>(complete)
                .is_some()
        );
        assert_eq!(
            world
                .get::<bevy::a11y::AccessibilityNode>(complete)
                .unwrap()
                .toggled(),
            Some(accesskit::Toggled::True)
        );
        receive(
            world,
            owner,
            &ServerMessage::Update {
                id: "task".into(),
                rows: Vec::new(),
            },
        );
        show(world, owner);
        assert!(world.get::<State>(owner).unwrap().quantity.is_none());
        assert!(
            world
                .get::<bevy::ui::InteractionDisabled>(complete)
                .is_some()
        );
        assert!(world.get::<State>(owner).unwrap().source_changed);
    }

    #[test]
    fn the_habit_opens_the_ordinary_karma_editor_and_reports_a_missing_cell() {
        let (mut app, owner) = fixture();
        let world = app.world_mut();
        world.get_mut::<State>(owner).unwrap().preview = Some((Form::default(), preview()));
        Command::Karma.apply(world, owner);
        world.flush();
        assert_eq!(
            world
                .query::<&crate::karma_castle::KarmaCastle>()
                .iter(world)
                .count(),
            1
        );
        Command::Preview.apply(world, owner);
        assert!(
            world
                .get::<State>(owner)
                .unwrap()
                .notice
                .contains("No Cell connection")
        );
        assert!(world.get::<State>(owner).unwrap().pending.is_none());
    }

    #[test]
    fn the_instinct_karma_page_embeds_the_habit_and_preserves_its_form_across_navigation() {
        let (mut app, owner) = fixture();
        let world = app.world_mut();
        let root = world.get::<ChildOf>(owner).unwrap().parent();
        let reader = crate::instinct::spawn(
            world,
            root,
            1,
            DVec2::ZERO,
            crate::instinct::Instinct {
                page: Some("karma".into()),
                ..Default::default()
            },
        );
        world.flush();
        let field = world
            .query::<(Entity, &Field)>()
            .iter(world)
            .find(|(_, field)| field.owner == reader && field.index == 0)
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("09:00");
        crate::instinct::Command::Page("tool".into()).apply(world, reader);
        world.flush();
        assert_eq!(world.get::<State>(reader).unwrap().form.time, "09:00");
        crate::instinct::Command::Page("karma".into()).apply(world, reader);
        world.flush();
        let fields: Vec<_> = world
            .query::<(&Field, &EditableText)>()
            .iter(world)
            .filter(|(field, _)| field.owner == reader)
            .map(|(field, text)| (field.index, text.value().to_string()))
            .collect();
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&(0, "09:00".into())));
    }
}
