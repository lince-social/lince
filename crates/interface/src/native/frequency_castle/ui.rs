use super::*;
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    text::EditableText,
};

mod form;
mod list;

const PAGE_SIZE: usize = 20;

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Component)]
pub(super) struct Countdown {
    owner: Entity,
    uid: String,
    schedule: String,
    presented: Option<(String, model::Next)>,
}

#[derive(Clone)]
pub(crate) enum Command {
    Create,
    New,
    Save,
    Cancel,
    Edit(String),
    Weekday(nucleus::karma::CivilWeekday),
    Delete(String),
    ConfirmDelete(String),
    CancelDelete,
    Page(usize),
}

impl Action for Command {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        if matches!(self, Self::Save) {
            &[lince_interface::practice::Operation::CreateFrequency]
        } else {
            &[]
        }
    }

    fn tutorial_supports(&self) -> &'static [lince_interface::practice::Operation] {
        if matches!(self, Self::Weekday(_) | Self::New | Self::Cancel) {
            &[lince_interface::practice::Operation::CreateFrequency]
        } else {
            &[]
        }
    }

    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::native::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(
                world,
                owner,
                workspace,
                position,
                FrequencyCastle::default(),
            );
            return;
        }
        if world
            .get::<View>(owner)
            .is_none_or(|view| view.pending.is_some())
            || crate::native::laboratory::suspended(world, owner)
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::Create => {}
            Self::New | Self::Edit(_) => {
                if world.get::<FrequencyCastle>(owner).unwrap().draft.is_some() {
                    status(world, owner, "Save or cancel this draft first");
                    return;
                }
                let draft = if let Self::Edit(uid) = self {
                    let Some(row) = find(world, owner, uid) else {
                        return;
                    };
                    Draft::edit(row)
                } else {
                    Draft::default()
                };
                world.get_mut::<FrequencyCastle>(owner).unwrap().draft = Some(draft);
            }
            Self::Save => {
                save(world, owner);
                return;
            }
            Self::Cancel => {
                world.get_mut::<FrequencyCastle>(owner).unwrap().draft = None;
            }
            Self::Weekday(day) => {
                if let Some(draft) = &mut world.get_mut::<FrequencyCastle>(owner).unwrap().draft {
                    if draft.weekdays.contains(day) {
                        draft.weekdays.retain(|selected| selected != day);
                    } else {
                        draft.weekdays.push(*day);
                        draft.weekdays.sort();
                    }
                }
            }
            Self::Delete(uid) => {
                world.get_mut::<View>(owner).unwrap().deleting = Some(uid.clone());
            }
            Self::CancelDelete => {
                world.get_mut::<View>(owner).unwrap().deleting = None;
            }
            Self::ConfirmDelete(uid) => {
                if world.get::<View>(owner).unwrap().deleting.as_ref() != Some(uid) {
                    return;
                }
                submit(
                    world,
                    owner,
                    engine::actions::Action::DeleteFrequency {
                        frequency: uid.clone(),
                    },
                    None,
                    "Frequency deleted",
                );
                return;
            }
            Self::Page(page) => {
                world.get_mut::<View>(owner).unwrap().page = *page;
            }
        }
        render_form(world, owner);
        render_list(world, owner);
    }
}

fn find<'a>(world: &'a World, owner: Entity, uid: &str) -> Option<&'a Frequency> {
    world.get::<View>(owner)?.rows.by_uid(uid)
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(6),
                row_gap: px(6),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn stack(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    title: &str,
    command: Command,
) -> Entity {
    crate::native::castle_feed::button(world, parent, owner, title, command)
}

pub(super) fn glyph(
    world: &mut World,
    parent: Entity,
    icon: crate::native::icons::Icon,
    size: f32,
) {
    let image = crate::native::icons::image(world, icon).unwrap_or_default();
    world.spawn((
        image,
        crate::native::token_style::TextToken(crate::native::tokens::Token::Ink),
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(parent),
    ));
}

pub(super) fn icon_button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    icon: crate::native::icons::Icon,
    title: &str,
    command: Command,
    size: f32,
) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::sand::Square,
            crate::native::sand::Borderless,
            Node {
                width: px(size),
                height: px(size),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            crate::native::actions::ActionButton::new(owner, crate::native::actions![command]),
            crate::native::icons::Tooltip(title.into()),
            ChildOf(parent),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    world.entity_mut(entity).insert((
        crate::native::icons::InlineTooltip,
        crate::native::icons::TooltipIcon { source: entity },
    ));
    glyph(world, entity, icon, size);
}

pub(super) fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    index: usize,
    title: &str,
    value: &str,
) -> Entity {
    if index != 4 {
        crate::native::edit_mode::label(world, parent, title, 13.0);
    }
    let entity = world
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert((
        Node {
            width: if index == 4 { px(230) } else { percent(100) },
            min_width: px(0),
            min_height: px(36),
            ..default()
        },
        ChildOf(parent),
        Input { owner, index },
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = false;
    editor.max_characters = Some(256);
    editor.visible_lines = Some(1.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(title);
    }
    entity
}

fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub(super) fn render_form(world: &mut World, owner: Entity) {
    form::render(world, owner);
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    list::render(world, owner);
}

pub(super) fn capture(world: &mut World, owner: Entity) -> bool {
    let _capture = nucleus::performance::Span::new("frequency-field-capture");
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Frequency fields")
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut castles: Query<&mut FrequencyCastle, Allow<Disabled>>,
) -> bool {
    let Ok(mut castle) = castles.get_mut(owner) else {
        return false;
    };
    let mut previous_search = None;
    for (input, text) in &inputs {
        if input.owner != owner {
            continue;
        }
        let expected = if input.index == 4 {
            Some(castle.search.as_str())
        } else {
            castle
                .draft
                .as_ref()
                .map(|draft| draft.fields[input.index].as_str())
        };
        let Some(expected) = expected else { continue };
        if text.value() == expected {
            continue;
        }
        if input.index == 4 {
            if previous_search.is_none() {
                previous_search = Some(std::mem::take(&mut castle.search));
            }
            crate::controls::copy_value(text, &mut castle.search);
        } else if let Some(draft) = &mut castle.draft {
            crate::controls::copy_value(text, &mut draft.fields[input.index]);
        }
        nucleus::performance::add("frequency-fields-published", 1);
    }
    previous_search.is_some_and(|previous| castle.search != previous)
}

pub(super) fn inputs(world: &mut World) {
    world
        .run_system_cached(update_inputs)
        .expect("update Frequency inputs");
}

fn update_inputs(
    world: &mut World,
    candidates: &mut QueryState<Entity, With<FrequencyCastle>>,
    mut owners: Local<Vec<Entity>>,
) {
    owners.extend(candidates.iter(world));
    let count = owners.len();
    for owner in owners.drain(..) {
        if capture(world, owner) {
            world.get_mut::<View>(owner).unwrap().page = 0;
            render_list(world, owner);
        }
    }
    if count == 0 {
        *owners = Vec::new();
    }
}

pub(super) fn refresh_next_date(world: &mut World, owner: Entity) {
    capture(world, owner);
    let Some(draft) = world
        .get::<FrequencyCastle>(owner)
        .and_then(|castle| castle.draft.as_ref())
    else {
        return;
    };
    if draft
        .original_fields
        .as_ref()
        .is_none_or(|fields| fields[3] != draft.fields[3])
    {
        return;
    }
    let Some(row) = draft.uid.as_ref().and_then(|uid| find(world, owner, uid)) else {
        return;
    };
    if Some(row.handle_revision) != draft.revision {
        return;
    }
    let date = model::edit_date(row);
    if date == draft.fields[3] {
        return;
    }
    let Some(draft) = world
        .get_mut::<FrequencyCastle>(owner)
        .unwrap()
        .into_inner()
        .draft
        .as_mut()
    else {
        return;
    };
    date.clone_into(&mut draft.fields[3]);
    if let Some(fields) = &mut draft.original_fields {
        date.clone_into(&mut fields[3]);
    }
    for (input, mut text) in world.query::<(&Input, &mut EditableText)>().iter_mut(world) {
        if input.owner == owner && input.index == 3 && text.value() != date.as_str() {
            text.editor.set_text(&date);
        }
    }
}

fn publish_countdown(world: &mut World, entity: Entity, next: model::Next) {
    let _publish = nucleus::performance::Span::new("frequency-countdown-publication");
    let countdown = world.get::<Countdown>(entity).unwrap();
    let schedule_changed = countdown
        .presented
        .as_ref()
        .is_none_or(|(schedule, _)| *schedule != countdown.schedule);
    let changed = schedule_changed
        || countdown
            .presented
            .as_ref()
            .is_none_or(|(_, presented)| *presented != next);
    let entity_ref = world.entity(entity);
    let text = entity_ref.get_ref::<Text>().unwrap();
    if !changed && !text.is_changed() {
        return;
    }
    nucleus::performance::add("frequency-countdowns-prepared", 1);
    let label = format!("{} | Next: {next}", countdown.schedule);
    let publish = text.0 != label;
    let schedule = schedule_changed.then(|| countdown.schedule.clone());
    if changed {
        let mut countdown = world.get_mut::<Countdown>(entity).unwrap();
        if let Some((previous_schedule, previous_next)) = &mut countdown.presented {
            if let Some(schedule) = schedule {
                *previous_schedule = schedule;
            }
            *previous_next = next;
        } else {
            countdown.presented = Some((schedule.expect("first countdown schedule"), next));
        }
    }
    if publish {
        world.get_mut::<Text>(entity).unwrap().0 = label;
        nucleus::performance::add("frequency-countdowns-published", 1);
    }
}

pub(super) fn tick(
    world: &mut World,
    candidates: &mut QueryState<(Entity, &'static Countdown)>,
    mut countdowns: Local<Vec<Entity>>,
    mut wake_at: Local<Option<std::time::Instant>>,
) {
    let _presentation = nucleus::performance::Span::new("frequency-countdown-presentation");
    let now = chrono::Utc::now().timestamp_millis();
    countdowns.extend(candidates.iter(world).map(|(entity, _)| entity));
    let count = countdowns.len();
    nucleus::performance::add("frequency-countdown-candidates", count as u64);
    let mut ticking = false;
    for entity in countdowns.drain(..) {
        let countdown = world.get::<Countdown>(entity).unwrap();
        if crate::native::laboratory::suspended(world, countdown.owner) {
            continue;
        }
        let Some(root) = world.get::<ChildOf>(countdown.owner).map(ChildOf::parent) else {
            continue;
        };
        if let (Some(spaces), Some(member)) = (
            world.get::<crate::native::workspace::Workspaces>(root),
            world.get::<WorkspaceMember>(countdown.owner),
        ) && spaces.active != member.0
        {
            continue;
        }
        let Some(frequency) = world
            .get::<View>(countdown.owner)
            .and_then(|view| view.rows.by_uid(&countdown.uid))
        else {
            continue;
        };
        let next = model::Next::new(frequency, now);
        ticking |= next.scheduled();
        publish_countdown(world, entity, next);
    }
    if count == 0 {
        *countdowns = Vec::new();
    }
    if ticking && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
        *wake_at = Some(std::time::Instant::now());
        if let Some(wake) = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                wake.ring();
            });
        }
    }
}
