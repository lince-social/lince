use super::*;
use bevy::text::EditableText;
use model::Unit;

const PAGE_SIZE: usize = 20;

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Component)]
struct Countdown {
    owner: Entity,
    uid: String,
}

#[derive(Clone)]
pub(super) enum Command {
    Create,
    New,
    Save,
    Cancel,
    Edit(String),
    Unit(Unit),
    Delete(String),
    ConfirmDelete(String),
    CancelDelete,
    Page(usize),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Create) {
            let workspace = world
                .get::<crate::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
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
            || crate::laboratory::suspended(world, owner)
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
                    Draft::edit(&row)
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
            Self::Unit(unit) => {
                if let Some(draft) = &mut world.get_mut::<FrequencyCastle>(owner).unwrap().draft {
                    draft.unit = *unit;
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

fn find(world: &World, owner: Entity, uid: &str) -> Option<Frequency> {
    world
        .get::<View>(owner)?
        .rows
        .iter()
        .find(|row| row.uid == uid)
        .cloned()
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
) {
    crate::castle_feed::button(world, parent, owner, title, command);
}

pub(super) fn input(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    index: usize,
    title: &str,
    value: &str,
) {
    if index != 4 {
        crate::edit_mode::label(world, parent, title, 13.0);
    }
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert((
        Node {
            width: if index == 4 { px(230) } else { percent(100) },
            margin: if index == 4 {
                UiRect::left(Val::Auto)
            } else {
                UiRect::ZERO
            },
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
    let form = world.get::<View>(owner).unwrap().form;
    clear(world, form);
    let Some(draft) = world.get::<FrequencyCastle>(owner).unwrap().draft.clone() else {
        return;
    };
    crate::edit_mode::label(
        world,
        form,
        if draft.uid.is_some() {
            "Edit frequency · existing rules keep the same reference"
        } else {
            "New frequency"
        },
        16.0,
    );
    for (index, title) in [(0, "Slug"), (1, "Name")] {
        input(world, form, owner, index, title, &draft.fields[index]);
    }
    if draft.cadence_editable {
        input(world, form, owner, 2, "Every", &draft.fields[2]);
        let units = row(world, form);
        for unit in Unit::ALL {
            let title = if unit == draft.unit {
                format!("[{}]", unit.label())
            } else {
                unit.label().into()
            };
            button(world, units, owner, &title, Command::Unit(unit));
        }
        let title = match draft.original.as_ref().map(|original| &original.cadence) {
            Some(nucleus::karma::FrequencyCadenceAst::Calendar { timezone, .. }) => {
                format!("Next date · {} local time", timezone.as_str())
            }
            _ => "Next date · date and time with offset".into(),
        };
        input(world, form, owner, 3, &title, &draft.fields[3]);
    } else {
        crate::edit_mode::label(
            world,
            form,
            "This calendar or parameter-based schedule is kept unchanged when renaming.",
            12.0,
        );
    }
    let controls = row(world, form);
    button(world, controls, owner, "Save", Command::Save);
    button(world, controls, owner, "Cancel", Command::Cancel);
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let (list, deleting, page) = (view.list, view.deleting.clone(), view.page);
    let query = world
        .get::<FrequencyCastle>(owner)
        .unwrap()
        .search
        .trim()
        .to_lowercase();
    let rows: Vec<_> = view
        .rows
        .iter()
        .filter(|row| {
            format!("{} {} {}", row.slug, row.definition.purpose, row.status)
                .to_lowercase()
                .contains(&query)
        })
        .cloned()
        .collect();
    let page = page.min(rows.len().saturating_sub(1) / PAGE_SIZE);
    world.get_mut::<View>(owner).unwrap().page = page;
    clear(world, list);
    if rows.is_empty() {
        crate::edit_mode::label(
            world,
            list,
            "No frequencies found. Use + to create one.",
            14.0,
        );
    }
    let now = chrono::Utc::now().timestamp_millis();
    for frequency in rows.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE) {
        let block = stack(world, list);
        crate::edit_mode::label(
            world,
            block,
            &format!("@{} · {}", frequency.slug, frequency.definition.purpose),
            17.0,
        );
        let unapplied = frequency
            .active_revision_hash
            .as_ref()
            .is_some_and(|active| active != &frequency.head_revision_hash);
        crate::edit_mode::label(
            world,
            block,
            &format!(
                "{} · Saved: {}{}",
                frequency.status,
                model::schedule(frequency),
                if unapplied {
                    " · saved changes not yet running"
                } else {
                    ""
                }
            ),
            13.0,
        );
        let countdown = crate::edit_mode::label(world, block, &model::next(frequency, now), 13.0);
        world.entity_mut(countdown).insert(Countdown {
            owner,
            uid: frequency.uid.clone(),
        });
        let controls = row(world, block);
        button(
            world,
            controls,
            owner,
            "Edit",
            Command::Edit(frequency.uid.clone()),
        );
        button(
            world,
            controls,
            owner,
            "Delete",
            Command::Delete(frequency.uid.clone()),
        );
        if deleting.as_ref() == Some(&frequency.uid) {
            crate::edit_mode::label(
                world,
                block,
                "Delete this frequency? Frequencies used by rules, Signals, or Programs cannot be deleted.",
                13.0,
            );
            let confirm = row(world, block);
            button(
                world,
                confirm,
                owner,
                "Confirm delete",
                Command::ConfirmDelete(frequency.uid.clone()),
            );
            button(
                world,
                confirm,
                owner,
                "Keep frequency",
                Command::CancelDelete,
            );
        }
    }
    if rows.len() > PAGE_SIZE {
        let pages = row(world, list);
        if page > 0 {
            button(world, pages, owner, "Previous", Command::Page(page - 1));
        }
        crate::edit_mode::label(
            world,
            pages,
            &format!("Page {} / {}", page + 1, rows.len().div_ceil(PAGE_SIZE)),
            13.0,
        );
        if (page + 1) * PAGE_SIZE < rows.len() {
            button(world, pages, owner, "Next", Command::Page(page + 1));
        }
    }
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let inputs: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    let Some(mut castle) = world.get_mut::<FrequencyCastle>(owner) else {
        return;
    };
    for (index, value) in inputs {
        if index == 4 {
            castle.search = value;
        } else if let Some(draft) = &mut castle.draft {
            draft.fields[index] = value;
        }
    }
}

pub(super) fn inputs(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<FrequencyCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        let previous = world.get::<FrequencyCastle>(owner).unwrap().search.clone();
        capture(world, owner);
        if previous != world.get::<FrequencyCastle>(owner).unwrap().search {
            world.get_mut::<View>(owner).unwrap().page = 0;
            render_list(world, owner);
        }
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
    let date = Draft::edit(&row).fields[3].clone();
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
    draft.fields[3] = date.clone();
    if let Some(fields) = &mut draft.original_fields {
        fields[3] = date.clone();
    }
    for (input, mut text) in world.query::<(&Input, &mut EditableText)>().iter_mut(world) {
        if input.owner == owner && input.index == 3 {
            text.editor.set_text(&date);
        }
    }
}

pub(super) fn tick(world: &mut World, mut wake_at: Local<Option<std::time::Instant>>) {
    let now = chrono::Utc::now().timestamp_millis();
    let updates: Vec<_> = world
        .query::<(Entity, &Countdown)>()
        .iter(world)
        .filter_map(|(entity, countdown)| {
            if crate::laboratory::suspended(world, countdown.owner) {
                return None;
            }
            let root = world.get::<ChildOf>(countdown.owner)?.parent();
            if let (Some(spaces), Some(member)) = (
                world.get::<crate::workspace::Workspaces>(root),
                world.get::<WorkspaceMember>(countdown.owner),
            ) && spaces.active != member.0
            {
                return None;
            }
            let frequency = world
                .get::<View>(countdown.owner)?
                .rows
                .iter()
                .find(|row| row.uid == countdown.uid)?;
            Some((
                entity,
                model::next(frequency, now),
                frequency.next_at_ms.is_some(),
            ))
        })
        .collect();
    let ticking = updates.iter().any(|(_, _, next)| *next);
    for (entity, label, _) in updates {
        if world.get::<Text>(entity).unwrap().0 != label {
            world.get_mut::<Text>(entity).unwrap().0 = label;
        }
    }
    if ticking && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
        *wake_at = Some(std::time::Instant::now());
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                wake.ring();
            });
        }
    }
}
