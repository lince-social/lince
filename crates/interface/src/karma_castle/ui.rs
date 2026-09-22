use super::*;
use crate::{actions::ActionButton, icons::Tooltip};
use bevy::input_focus::InputFocus;
use bevy::text::EditableText;
use model::{FieldDraft, SharedField, Suggestion};
use nucleus::karma::rule_field::RuleFieldKind;

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
    suggestions: Entity,
    preview: Entity,
    observed: String,
    focused: bool,
    selection: std::ops::Range<usize>,
    dirty: bool,
}

#[derive(Component)]
struct Search(Entity);

pub(super) fn search(world: &mut World, parent: Entity, owner: Entity) {
    let value = world.get::<KarmaCastle>(owner).unwrap().search.clone();
    let entity = world
        .spawn(crate::sand::text_editor(
            &value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    world.entity_mut(entity).insert((
        Node {
            width: px(230),
            min_width: px(0),
            min_height: px(36),
            margin: UiRect::left(Val::Auto),
            ..default()
        },
        ChildOf(parent),
        Search(owner),
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = false;
    editor.max_characters = Some(256);
    editor.visible_lines = Some(1.0);
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label("Filter rules");
    }
}

#[derive(Component)]
pub(super) struct Reading {
    owner: Entity,
    slug: String,
    source: String,
    hovered: bool,
    requested_at: Option<i64>,
    pending: bool,
    result: Option<String>,
}

#[derive(Clone)]
pub(super) enum Command {
    Create,
    New,
    Cancel,
    Save,
    Link(usize, SharedField),
    Insert(usize, String, std::ops::Range<usize>),
    Unlink(usize),
    Edit(SharedField),
    Copy(String),
    Detach(String, usize),
    Pause(String, i64, bool),
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
            spawn(world, owner, workspace, position, KarmaCastle::default());
            return;
        }
        if world
            .get::<View>(owner)
            .is_none_or(|view| view.pending.is_some())
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::Create => {}
            Self::New => {
                if world.get::<KarmaCastle>(owner).unwrap().draft.is_some() {
                    status(world, owner, "Save or cancel the current draft first");
                    return;
                }
                world.get_mut::<KarmaCastle>(owner).unwrap().draft = Some(Draft::default());
            }
            Self::Cancel => {
                let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                castle.draft = castle.suspended.take();
            }
            Self::Save => {
                save(world, owner);
                return;
            }
            Self::Link(index, field) => {
                if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
                    draft.fields[*index] = FieldDraft {
                        text: field.source.clone(),
                        linked: Some(field.clone()),
                    };
                }
            }
            Self::Insert(index, element, selection) => {
                if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
                    let field = &mut draft.fields[*index];
                    field.text = model::insert_at(
                        &field.text,
                        selection.clone(),
                        element,
                        RuleFieldKind::ALL[*index],
                    );
                }
                let input = world
                    .query::<(Entity, &Input)>()
                    .iter(world)
                    .find(|(_, input)| input.owner == owner && input.index == *index)
                    .map(|(entity, _)| entity);
                if let Some(entity) = input {
                    let text = world
                        .get::<KarmaCastle>(owner)
                        .unwrap()
                        .draft
                        .as_ref()
                        .unwrap()
                        .fields[*index]
                        .text
                        .clone();
                    world
                        .get_mut::<EditableText>(entity)
                        .unwrap()
                        .editor
                        .set_text(&text);
                    world
                        .resource_mut::<InputFocus>()
                        .set(entity, bevy::input_focus::FocusCause::Navigated);
                }
                return;
            }
            Self::Unlink(index) => {
                if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
                    draft.fields[*index] = FieldDraft::default();
                }
            }
            Self::Edit(field) => {
                let index = RuleFieldKind::ALL
                    .iter()
                    .position(|kind| *kind == field.kind)
                    .unwrap();
                if world
                    .get::<KarmaCastle>(owner)
                    .unwrap()
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.editing.is_some())
                {
                    status(
                        world,
                        owner,
                        "Save or cancel this draft before editing the shared original",
                    );
                    return;
                }
                let mut draft = Draft {
                    editing: Some(field.clone()),
                    ..Default::default()
                };
                draft.fields[index].text = field.source.clone();
                let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                castle.suspended = castle.draft.take();
                castle.draft = Some(draft);
            }
            Self::Copy(text) => {
                let copied = world
                    .get_resource_mut::<bevy::clipboard::Clipboard>()
                    .is_some_and(|mut clipboard| clipboard.set_text(text.clone()).is_ok());
                status(
                    world,
                    owner,
                    if copied {
                        "Copied. Use × to start a fresh field, then paste."
                    } else {
                        "Clipboard unavailable. Select and copy the text in the editor."
                    },
                );
                return;
            }
            Self::Detach(uid, index) => {
                if world.get::<KarmaCastle>(owner).unwrap().draft.is_some() {
                    status(world, owner, "Save or cancel the current draft first");
                    return;
                }
                let Some(rule) = world
                    .get::<View>(owner)
                    .unwrap()
                    .rules
                    .iter()
                    .find(|rule| &rule.uid == uid)
                else {
                    return;
                };
                let mut draft = Draft::from_rule(rule);
                draft.fields[*index] = FieldDraft::default();
                world.get_mut::<KarmaCastle>(owner).unwrap().draft = Some(draft);
            }
            Self::Pause(uid, revision, paused) => {
                let id = nucleus::new_uid("karma-pause");
                let action = engine::actions::Action::SetRecurrencePaused {
                    recurrence: uid.clone(),
                    expected_revision: *revision,
                    request_id: id.clone(),
                    paused: *paused,
                };
                match send(
                    world,
                    ClientMessage::Act {
                        id: id.clone(),
                        action,
                    },
                ) {
                    Ok(()) => {
                        world.get_mut::<View>(owner).unwrap().pending = Some(id);
                        status(world, owner, "Saving…");
                    }
                    Err(error) => status(world, owner, error),
                }
                return;
            }
        }
        render_form(world, owner);
        refresh_links(world, owner);
    }
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(8),
                align_items: AlignItems::Start,
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
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(super) fn column(world: &mut World, parent: Entity) -> Entity {
    let entity = stack(world, parent);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = percent(33.333);
    node.min_width = px(0);
    node.flex_shrink = 1.0;
    entity
}

pub(super) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    text: &str,
    hint: &str,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            ActionButton::new(owner, crate::actions![command]),
            Tooltip(hint.into()),
            Node {
                padding: UiRect::axes(px(5), px(3)),
                min_width: px(24),
                max_width: percent(100),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    if let Some(mut accessible) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessible.set_label(hint);
    }
    let label = crate::edit_mode::label(world, entity, text, 12.0);
    world.get_mut::<Node>(label).unwrap().flex_shrink = 1.0;
    entity
}

fn clear(world: &mut World, entity: Entity) {
    let children: Vec<_> = world
        .get::<Children>(entity)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

fn rich_text(world: &mut World, parent: Entity, owner: Entity, source: &str) {
    let line = row(world, parent);
    let mut node = world.get_mut::<Node>(line).unwrap();
    node.flex_wrap = FlexWrap::Wrap;
    node.column_gap = px(0);
    let mut offset = 0;
    for (text, slug) in model::fragments(source) {
        let part = crate::edit_mode::label(world, line, &text, 14.0);
        world.get_mut::<Node>(part).unwrap().flex_shrink = 1.0;
        if let Some(slug) = slug {
            world.entity_mut(part).insert((
                Reading {
                    owner,
                    slug,
                    source: model::reading_at(source, offset, offset + text.len()),
                    hovered: false,
                    requested_at: None,
                    pending: false,
                    result: None,
                },
                Tooltip("Loading reading…".into()),
                crate::token_style::text(crate::tokens::Token::Accent),
            ));
        }
        offset += text.len();
    }
}

fn field_controls(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    field: &SharedField,
    unlink: Command,
) {
    let controls = row(world, parent);
    button(
        world,
        controls,
        owner,
        Command::Edit(field.clone()),
        "Edit",
        "Edit the shared original. Every linked rule will change.",
    );
    button(
        world,
        controls,
        owner,
        Command::Copy(field.source.clone()),
        "Copy",
        "Copy the text without its shared identity",
    );
    button(
        world,
        controls,
        owner,
        unlink,
        "×",
        "Stop using this shared field. Changes take effect only after Save.",
    );
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let query = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .search
        .trim()
        .to_lowercase();
    let (list, rules) = (
        view.list,
        view.rules
            .iter()
            .filter(|rule| {
                rule.fields
                    .iter()
                    .any(|field| field.source.to_lowercase().contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>(),
    );
    clear(world, list);
    for rule in rules {
        let block = stack(world, list);
        let cells = row(world, block);
        for (index, kind) in RuleFieldKind::ALL.into_iter().enumerate() {
            let cell = column(world, cells);
            if let Some(field) = rule.fields.iter().find(|field| field.kind == kind) {
                rich_text(world, cell, owner, &field.source);
                field_controls(
                    world,
                    cell,
                    owner,
                    field,
                    Command::Detach(rule.uid.clone(), index),
                );
            }
        }
        let paused = rule.state == "paused";
        button(
            world,
            block,
            owner,
            Command::Pause(rule.uid, rule.revision, !paused),
            if paused { "Resume" } else { "Pause" },
            "Pause or resume this rule",
        );
    }
}

pub(super) fn render_form(world: &mut World, owner: Entity) {
    let form = world.get::<View>(owner).unwrap().form;
    clear(world, form);
    let Some(draft) = world.get::<KarmaCastle>(owner).unwrap().draft.clone() else {
        return;
    };
    if let Some(field) = &draft.editing {
        let uses = world
            .get::<View>(owner)
            .unwrap()
            .rules
            .iter()
            .filter(|rule| rule.fields.iter().any(|item| item.uid == field.uid))
            .count();
        crate::edit_mode::label(
            world,
            form,
            &format!(
                "Editing shared {} · Save changes all {uses} linked rules",
                field.kind.as_str()
            ),
            14.0,
        );
    }
    let cells = row(world, form);
    for (index, value) in draft.fields.iter().enumerate() {
        let cell = column(world, cells);
        if draft
            .editing
            .as_ref()
            .is_some_and(|field| field.kind != RuleFieldKind::ALL[index])
        {
            continue;
        }
        if let Some(field) = &value.linked {
            rich_text(world, cell, owner, &field.source);
            field_controls(world, cell, owner, field, Command::Unlink(index));
        } else {
            let entity = world
                .spawn(crate::sand::text_editor(
                    &value.text,
                    world.resource::<crate::theme::Typography>(),
                    0,
                ))
                .id();
            world.entity_mut(entity).insert((
                Node {
                    width: percent(100),
                    min_width: px(0),
                    min_height: px(36),
                    ..default()
                },
                Tooltip(
                    [
                        "Condition: @record * freq(@frequency)",
                        "Threshold: !=0, >0, >=10, ==0, or always",
                        "Consequence: @record saves the calculated quantity; @record: command(\"…\") runs a command",
                    ][index]
                        .into(),
                ),
                ChildOf(cell),
            ));
            let mut editor = world.get_mut::<EditableText>(entity).unwrap();
            editor.allow_newlines = false;
            editor.max_characters = Some(16_384);
            editor.visible_lines = Some(2.0);
            if let Some(mut accessible) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
                accessible.set_label(["Condition", "Threshold", "Consequence"][index]);
            }
            let preview = stack(world, cell);
            let suggestions = stack(world, cell);
            world.entity_mut(entity).insert(Input {
                owner,
                index,
                suggestions,
                preview,
                observed: "\0".into(),
                focused: false,
                selection: 0..0,
                dirty: false,
            });
        }
    }
    let controls = row(world, form);
    button(
        world,
        controls,
        owner,
        Command::Save,
        "Save",
        "Validate and save this rule",
    );
    button(
        world,
        controls,
        owner,
        Command::Cancel,
        "Cancel",
        "Keep the saved rules unchanged",
    );
    crate::edit_mode::label(
        world,
        form,
        "Condition computes the quantity · Threshold tests it · Consequence chooses the Record to change",
        11.0,
    );
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
        for (index, text) in values {
            draft.fields[index].text = text;
        }
    }
}

pub(super) fn inputs(world: &mut World) {
    let searches: Vec<_> = world
        .query::<(&Search, &EditableText)>()
        .iter(world)
        .filter(|(search, text)| {
            world
                .get::<KarmaCastle>(search.0)
                .is_some_and(|castle| text.value() != castle.search.as_str())
        })
        .map(|(search, text)| (search.0, text.value().to_string()))
        .collect();
    for (owner, value) in searches {
        world.get_mut::<KarmaCastle>(owner).unwrap().search = value;
        render_list(world, owner);
    }
    let focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    let inputs: Vec<_> = world
        .query::<(Entity, &Input, &EditableText)>()
        .iter(world)
        .filter_map(|(entity, input, text)| {
            let focused = focus == Some(entity) || descendant(world, focus, input.suggestions);
            let selection = text.editor.raw_selection().text_range();
            (text.value() != input.observed.as_str()
                || input.dirty
                || input.focused != focused
                || input.selection != selection)
                .then(|| {
                    (
                        entity,
                        input.owner,
                        input.index,
                        input.suggestions,
                        input.preview,
                        text.value().to_string(),
                        focused,
                        selection,
                    )
                })
        })
        .collect();
    for (entity, owner, index, suggestions, preview, text, focused, selection) in inputs {
        let mut input = world.get_mut::<Input>(entity).unwrap();
        let text_changed = input.observed != text;
        input.observed = text.clone();
        input.dirty = false;
        input.focused = focused;
        input.selection = selection.clone();
        if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
            draft.fields[index].text = text.clone();
        }
        if text_changed {
            clear(world, preview);
            rich_text(world, preview, owner, &text);
        }
        clear(world, suggestions);
        if !focused {
            continue;
        }
        let view = world.get::<View>(owner).unwrap();
        let choices = model::suggestions(
            RuleFieldKind::ALL[index],
            &text[..selection.end.min(text.len())],
            &view.rules,
            &view.records,
            &view.frequencies,
        );
        let shared_edit = world
            .get::<KarmaCastle>(owner)
            .unwrap()
            .draft
            .as_ref()
            .is_some_and(|draft| draft.editing.is_some());
        for choice in choices {
            match choice {
                Suggestion::Shared(field) if !shared_edit => {
                    let label = format!("Link · {}", field.source);
                    button(
                        world,
                        suggestions,
                        owner,
                        Command::Link(index, field),
                        &label,
                        "Reuse this shared field by ID",
                    );
                }
                Suggestion::Element(element) => {
                    button(
                        world,
                        suggestions,
                        owner,
                        Command::Insert(index, element.clone(), selection.clone()),
                        &element,
                        "Insert this element",
                    );
                }
                _ => {}
            }
        }
    }
}

fn descendant(world: &World, mut entity: Option<Entity>, ancestor: Entity) -> bool {
    while let Some(current) = entity {
        if current == ancestor {
            return true;
        }
        entity = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    false
}

pub(super) fn refresh_links(world: &mut World, owner: Entity) {
    let fields: HashMap<_, _> = world
        .get::<View>(owner)
        .unwrap()
        .rules
        .iter()
        .flat_map(|rule| &rule.fields)
        .map(|field| (field.uid.clone(), field.clone()))
        .collect();
    let mut changed = false;
    if let Some(draft) = &mut world.get_mut::<KarmaCastle>(owner).unwrap().draft {
        for value in &mut draft.fields {
            if let Some(linked) = &value.linked
                && let Some(latest) = fields.get(&linked.uid)
                && linked != latest
            {
                value.text = latest.source.clone();
                value.linked = Some(latest.clone());
                changed = true;
            }
        }
    }
    if changed {
        capture(world, owner);
        render_form(world, owner);
    }
    let inputs: Vec<_> = world
        .query::<(Entity, &Input)>()
        .iter(world)
        .filter(|(_, input)| input.owner == owner)
        .map(|(entity, _)| entity)
        .collect();
    for entity in inputs {
        world.get_mut::<Input>(entity).unwrap().dirty = true;
    }
}

pub(super) fn tick(world: &mut World, mut wake_at: Local<Option<std::time::Instant>>) {
    let now = chrono::Utc::now().timestamp_millis();
    let readings: Vec<_> = world
        .query::<(Entity, &Reading)>()
        .iter(world)
        .map(|(entity, reading)| {
            (
                entity,
                reading.owner,
                reading.slug.clone(),
                reading.source.clone(),
                reading.hovered,
                reading.pending,
                reading.requested_at,
                reading.result.clone(),
            )
        })
        .collect();
    let mut countdown = false;
    for (entity, owner, slug, source, hovered, pending, requested_at, result) in readings {
        let Some(view) = world.get::<View>(owner) else {
            continue;
        };
        let mut hint = if let Some(frequency) = view
            .frequency_lookup
            .get(&slug)
            .map(|index| &view.frequencies[*index])
        {
            countdown |= hovered && frequency["next_at_ms"].is_i64();
            model::frequency_hint(frequency, now)
        } else if let Some(record) = view
            .record_lookup
            .get(&slug)
            .map(|index| &view.records[*index])
        {
            format!(
                "{}\nCurrent quantity: {}",
                record["head"].as_str().unwrap_or(&slug),
                record["quantity"].as_str().unwrap_or("unavailable")
            )
        } else {
            "Record unavailable or not readable".into()
        };
        if source != format!("@{slug}") && !source.starts_with("freq(") {
            if let Some(result) = result {
                hint.push_str(&format!("\n{source}: {result}"));
            }
            if hovered {
                countdown = true;
                if !pending && requested_at.is_none_or(|at| now.saturating_sub(at) >= 1000) {
                    let id = nucleus::new_uid("karma-reading");
                    match send(
                        world,
                        ClientMessage::Act {
                            id: id.clone(),
                            action: engine::actions::Action::PreviewKarmaReading { source },
                        },
                    ) {
                        Ok(()) => {
                            world.resource_mut::<Requests>().readings.insert(id, entity);
                            let mut reading = world.get_mut::<Reading>(entity).unwrap();
                            reading.requested_at = Some(now);
                            reading.pending = true;
                        }
                        Err(error) => hint.push_str(&format!("\n{error}")),
                    }
                }
            }
        }
        if world.get::<Tooltip>(entity).unwrap().0 != hint {
            world.get_mut::<Tooltip>(entity).unwrap().0 = hint;
        }
    }
    if countdown && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
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

pub(super) fn hover_on(event: On<crate::effect::SandHoveredOn>, mut readings: Query<&mut Reading>) {
    if let Ok(mut reading) = readings.get_mut(event.entity) {
        reading.hovered = true;
    }
}

pub(super) fn hover_off(
    event: On<crate::effect::SandHoveredOff>,
    mut readings: Query<&mut Reading>,
) {
    if let Ok(mut reading) = readings.get_mut(event.entity) {
        reading.hovered = false;
    }
}

pub(super) fn reading_reply(world: &mut World, entity: Entity, result: String) {
    if let Some(mut reading) = world.get_mut::<Reading>(entity) {
        reading.pending = false;
        reading.result = Some(result);
    }
}
