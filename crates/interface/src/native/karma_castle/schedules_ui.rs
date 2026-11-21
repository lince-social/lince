// non-lean-yet
// Scope: mod choices, mod form, mod outcomes, mod presentation.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 144, 206, 208,
//   224, 237, 239, 287, 297, 503, 520.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (95, 400).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   117, 118, 119, 128, 129, 130, 253, 263, 278, 297, 304, 333, 344, 346, 347, 456, 468, 478, 492,
//   493.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (65, 77, 78, 125, 149, 174, 179, 187, 195, 201, 249, 259, 274, 293, 301, 330, 340,
//   343, 345, 377, 433, 434, 445, 448, 458, 475, 492, 493, 497).

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    text::EditableText,
};
use model::schedules::{DateKind, Draft};
use nucleus::karma::rule_field::RuleFieldKind;

use super::*;

mod choices;
mod form;
mod outcomes;
mod presentation;

#[derive(Component)]
pub(super) struct ScheduleView {
    panel: Entity,
    rows: Arc<Vec<Arc<Value>>>,
    dirty: bool,
    pending: Option<(String, Operation)>,
    dates: [Arc<Vec<Value>>; 2],
}

impl replies::Pending for ScheduleView {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.0.as_str())
    }
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
    world.entity_mut(panel).insert(Name::new("Karma schedules"));
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.entity_mut(owner).insert(ScheduleView {
        panel,
        rows: Arc::default(),
        dirty: true,
        pending: None,
        dates: Default::default(),
    });
    render(world, owner);
}

pub(super) fn toggle(world: &mut World, owner: Entity) {
    let panel = world.get::<ScheduleView>(owner).unwrap().panel;
    let mut node = world.get_mut::<Node>(panel).unwrap();
    node.display = if node.display == Display::None {
        Display::Flex
    } else {
        Display::None
    };
}

pub(super) fn dirty(world: &mut World, owner: Entity) {
    if let Some(mut view) = world.get_mut::<ScheduleView>(owner) {
        view.dirty = true;
    }
}

fn capture(world: &mut World, owner: Entity) {
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Karma schedule fields");
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut castles: Query<&mut KarmaCastle, Allow<Disabled>>,
) {
    let _capture = nucleus::performance::Span::new("karma-schedule-field-capture");
    let Ok(mut castle) = castles.get_mut(owner) else {
        return;
    };
    for (input, text) in &inputs {
        if input.owner != owner {
            continue;
        }
        let Some(draft) = &castle.schedule else {
            return;
        };
        let index = input.index;
        let current = match index {
            0 => &draft.name,
            1 | 4 => &draft.dates[(index - 1) / 3].value,
            2 | 5 => &draft.dates[(index - 2) / 3].timezone,
            3 | 6 => &draft.consequences[(index - 3) / 3],
            _ => continue,
        };
        if text.value() == current.as_str() {
            continue;
        }
        let draft = castle.schedule.as_mut().unwrap();
        let target = match index {
            0 => &mut draft.name,
            1 | 4 => &mut draft.dates[(index - 1) / 3].value,
            2 | 5 => &mut draft.dates[(index - 2) / 3].timezone,
            3 | 6 => &mut draft.consequences[(index - 3) / 3],
            _ => unreachable!(),
        };
        crate::controls::copy_value(text, target);
        nucleus::performance::add("karma-schedule-fields-published", 1);
    }
}

fn submit(world: &mut World, owner: Entity, action: engine::actions::Action, operation: Operation) {
    let id = nucleus::new_uid("schedule-ui");
    match send(
        world,
        owner,
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

impl crate::native::actions::Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner)
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
                let Some(draft) = world.get::<KarmaCastle>(owner).unwrap().schedule.as_ref() else {
                    return;
                };
                let action = draft.inputs().map(|boundaries| {
                    engine::actions::Action::SaveKarmaSchedule {
                        schedule: draft.uid.clone(),
                        expected_revision: draft.revision,
                        name: draft.name.clone(),
                        boundaries,
                        request_id: nucleus::new_uid("schedule-save"),
                    }
                });
                match action {
                    Ok(action) => submit(world, owner, action, Operation::Write),
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
                let mut view = world.get_mut::<ScheduleView>(owner).unwrap();
                if !view.dates[*index].is_empty() {
                    view.dates[*index] = Arc::default();
                }
            }
        }
        render(world, owner);
    }
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) -> Entity {
    crate::native::castle_feed::button(world, parent, owner, label, command)
}

fn input(world: &mut World, parent: Entity, owner: Entity, index: usize, label: &str, value: &str) -> Entity {
    crate::native::edit_mode::label(world, parent, label, 13.0);
    let entity = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
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
    entity
}

fn render(world: &mut World, owner: Entity) {
    presentation::render(world, owner);
}

pub(super) fn maintain(world: &mut World) {
    world
        .run_system_cached(maintain_schedules)
        .expect("maintain Karma schedules");
}

fn maintain_schedules(
    world: &mut World,
    editors: &mut QueryState<Entity, With<ScheduleView>>,
    candidates: &mut QueryState<(Entity, &'static ScheduleView)>,
    mut owners: Local<Vec<Entity>>,
) {
    let _maintain = nucleus::performance::Span::new("karma-schedule-maintenance");
    owners.extend(editors.iter(world));
    let count = owners.len();
    for owner in owners.drain(..) {
        capture(world, owner);
    }
    owners.extend(
        candidates
            .iter(world)
            .filter(|(_, view)| view.dirty && view.pending.is_none())
            .map(|(owner, _)| owner),
    );
    for owner in owners.drain(..) {
        if crate::native::practice_cells::sender(world, owner).is_none()
            || crate::native::laboratory::blocked(world, owner)
        {
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
    if count == 0 {
        *owners = Vec::new();
    }
}

pub(super) fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
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
                    .map_or(&[][..], Vec::as_slice);
                if !incoming::publish_rows(
                    &mut world.get_mut::<ScheduleView>(owner).unwrap().rows, rows,
                ) {
                    return;
                }
                nucleus::performance::add("karma-schedule-rows-published", rows.len() as u64);
            } else if let Operation::Resolve(index) = operation {
                capture(world, owner);
                let choices = data
                    .as_ref()
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                if choices.len() == 1 {
                    if let Ok(input) = nucleus::karma::scheduled_change::DateInput::deserialize(
                        &choices[0]["date"],
                    ) {
                        if !world
                            .get::<KarmaCastle>(owner)
                            .unwrap()
                            .schedule
                            .as_ref()
                            .is_some_and(|draft| same_local(&draft.dates[index], &input))
                        {
                            status(world, owner, "The date changed; resolve it again");
                            return;
                        }
                        Command::SelectDate(index, input).apply(world, owner);
                    }
                    status(
                        world,
                        owner,
                        "Local date resolved with the installed timezone rules",
                    );
                    return;
                }
                if world.get::<ScheduleView>(owner).unwrap().dates[index].as_slice() != choices {
                    world.get_mut::<ScheduleView>(owner).unwrap().dates[index] = Arc::new(choices.to_vec());
                }
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
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::native::workspace::Workspaces::default())
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
                vec![serde_json::json!({"uid":transfer,"head":"Trade","karma_state":state})].into();
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
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::native::workspace::Workspaces::default())
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
