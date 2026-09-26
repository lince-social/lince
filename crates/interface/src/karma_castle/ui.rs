use super::*;
use crate::{
    actions::ActionButton,
    icons::{Icon, Tooltip},
};
use bevy::input_focus::InputFocus;
use bevy::text::EditableText;
use model::{FieldDraft, SharedField, Suggestion};
use nucleus::karma::rule_field::RuleFieldKind;

#[derive(Component)]
struct Input {
    owner: Entity,
    row: usize,
    index: usize,
    suggestions: Entity,
    observed: String,
    focused: bool,
    selection: std::ops::Range<usize>,
    dirty: bool,
}

#[derive(Component)]
struct Search(Entity);

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
    Select(String),
    SelectAll,
    EditSelected,
    CopySelected,
    DeleteSelected,
    ConfirmDelete,
    CancelDelete,
    Link(usize, usize, SharedField),
    Insert(usize, usize, String, std::ops::Range<usize>),
    Pause(String, i64, bool),
}

fn draft(castle: &KarmaCastle, row: usize) -> Option<&Draft> {
    if row == 0 {
        castle.draft.as_ref()
    } else {
        castle.edits.get(row - 1)
    }
}

fn draft_mut(castle: &mut KarmaCastle, row: usize) -> Option<&mut Draft> {
    if row == 0 {
        castle.draft.as_mut()
    } else {
        castle.edits.get_mut(row - 1)
    }
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
            || crate::laboratory::suspended(world, owner)
        {
            return;
        }
        capture(world, owner);
        match self {
            Self::Create => {}
            Self::New => {
                let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                if castle.draft.is_some() || !castle.edits.is_empty() {
                    return;
                }
                castle.draft = Some(Draft::default());
            }
            Self::Cancel => {
                let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                castle.draft = None;
                castle.edits.clear();
                status(world, owner, "");
            }
            Self::Save => {
                save(world, owner);
                return;
            }
            Self::Select(uid) => {
                if !world.get::<KarmaCastle>(owner).unwrap().edits.is_empty() {
                    return;
                }
                let mut view = world.get_mut::<View>(owner).unwrap();
                if !view.selection.remove(uid) {
                    view.selection.insert(uid.clone());
                }
                view.deleting.clear();
            }
            Self::SelectAll => {
                if !world.get::<KarmaCastle>(owner).unwrap().edits.is_empty() {
                    return;
                }
                let ids = visible_rules(world, owner)
                    .into_iter()
                    .map(|rule| rule.uid)
                    .collect::<Vec<_>>();
                let mut view = world.get_mut::<View>(owner).unwrap();
                let all = ids.iter().all(|uid| view.selection.contains(uid));
                for uid in ids {
                    if all {
                        view.selection.remove(&uid);
                    } else {
                        view.selection.insert(uid);
                    }
                }
                view.deleting.clear();
            }
            Self::EditSelected => {
                let castle = world.get::<KarmaCastle>(owner).unwrap();
                if castle.draft.is_some() || !castle.edits.is_empty() {
                    return;
                }
                let view = world.get::<View>(owner).unwrap();
                let edits = view
                    .rules
                    .iter()
                    .filter(|rule| view.selection.contains(&rule.uid))
                    .map(Draft::from_rule)
                    .collect();
                world.get_mut::<KarmaCastle>(owner).unwrap().edits = edits;
            }
            Self::CopySelected => {
                let view = world.get::<View>(owner).unwrap();
                let castle = world.get::<KarmaCastle>(owner).unwrap();
                if view.selection.len() != 1 || castle.draft.is_some() || !castle.edits.is_empty() {
                    return;
                }
                let Some(rule) = view
                    .rules
                    .iter()
                    .find(|rule| view.selection.contains(&rule.uid))
                else {
                    return;
                };
                let mut copied = Draft::from_rule(rule);
                copied.rule = None;
                copied.revision = None;
                copied.name = format!("{} copy", copied.name);
                copied.name = copied.name.chars().take(128).collect();
                let base: String = copied.slug.chars().take(200).collect();
                copied.slug = format!("{base}-copy");
                let mut suffix = 2;
                while view.rules.iter().any(|rule| rule.slug == copied.slug) {
                    copied.slug = format!("{base}-copy-{suffix}");
                    suffix += 1;
                }
                for field in &mut copied.fields {
                    field.linked = None;
                }
                world.get_mut::<KarmaCastle>(owner).unwrap().draft = Some(copied);
            }
            Self::DeleteSelected => {
                let castle = world.get::<KarmaCastle>(owner).unwrap();
                if castle.draft.is_some() || !castle.edits.is_empty() {
                    return;
                }
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.deleting = view
                    .rules
                    .iter()
                    .filter(|rule| view.selection.contains(&rule.uid))
                    .map(|rule| rule.uid.clone())
                    .collect();
            }
            Self::ConfirmDelete => {
                delete_next(world, owner);
                return;
            }
            Self::CancelDelete => world.get_mut::<View>(owner).unwrap().deleting.clear(),
            Self::Link(row, index, field) => {
                if let Some(draft) =
                    draft_mut(&mut world.get_mut::<KarmaCastle>(owner).unwrap(), *row)
                {
                    draft.fields[*index] = FieldDraft {
                        text: field.source.clone(),
                        linked: Some(field.clone()),
                    };
                }
            }
            Self::Insert(row, index, element, selection) => {
                if let Some(draft) =
                    draft_mut(&mut world.get_mut::<KarmaCastle>(owner).unwrap(), *row)
                {
                    let field = &mut draft.fields[*index];
                    field.text = model::insert_at(
                        &field.text,
                        selection.clone(),
                        element,
                        RuleFieldKind::ALL[*index],
                    );
                    field.linked = None;
                }
                let entity = world
                    .query::<(Entity, &Input)>()
                    .iter(world)
                    .find(|(_, input)| {
                        input.owner == owner && input.row == *row && input.index == *index
                    })
                    .map(|(entity, _)| entity);
                if let Some(entity) = entity {
                    let text = draft(world.get::<KarmaCastle>(owner).unwrap(), *row)
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
                    }
                    Err(error) => status(world, owner, error),
                }
            }
        }
        if matches!(self, Self::New | Self::EditSelected | Self::CopySelected) {
            world.get_mut::<View>(owner).unwrap().deleting.clear();
        }
        if matches!(self, Self::New | Self::CopySelected) {
            let form = world.get::<View>(owner).unwrap().form;
            let scroll = world.get::<ChildOf>(form).unwrap().parent();
            world.get_mut::<ScrollPosition>(scroll).unwrap().0 = Vec2::ZERO;
        }
        render_form(world, owner);
        render_list(world, owner);
    }
}

pub(super) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                column_gap: px(8),
                align_items: AlignItems::Center,
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
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(0),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn grid(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                display: Display::Grid,
                width: percent(100),
                min_width: px(0),
                flex_shrink: 0.0,
                grid_template_columns: vec![
                    GridTrack::px(28.0),
                    GridTrack::flex(2.0),
                    GridTrack::flex(3.0),
                    GridTrack::flex(1.5),
                    GridTrack::flex(3.0),
                ],
                border: UiRect::bottom(px(1)),
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::TableGrid),
            ChildOf(parent),
        ))
        .id()
}

fn cell(world: &mut World, parent: Entity) -> Entity {
    let entity = stack(world, parent);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.padding = UiRect::all(px(6));
    node.border = UiRect::right(px(1));
    world
        .entity_mut(entity)
        .insert(crate::token_style::border(crate::tokens::Token::TableGrid));
    entity
}

fn glyph(world: &mut World, parent: Entity, icon: Icon, size: f32) {
    world.spawn((
        crate::icons::image(world, icon).unwrap_or_default(),
        crate::token_style::TextToken(crate::tokens::Token::Ink),
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

fn icon_button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    icon: Icon,
    hint: &str,
    command: Command,
    enabled: bool,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            crate::sand::Borderless,
            ActionButton::new(owner, crate::actions![command]),
            Tooltip(hint.into()),
            Node {
                width: px(22),
                height: px(22),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    accessible(world, entity, accesskit::Role::Button, hint);
    world.entity_mut(entity).insert((
        crate::icons::InlineTooltip,
        crate::icons::TooltipIcon { source: entity },
    ));
    glyph(world, entity, icon, 22.0);
    if !enabled {
        world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
        let glyph = world.get::<Children>(entity).unwrap()[0];
        world
            .entity_mut(glyph)
            .insert(crate::token_style::TextToken(
                crate::tokens::Token::TableGrid,
            ));
    }
    entity
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    text: &str,
) -> Entity {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            crate::sand::Borderless,
            ActionButton::new(owner, crate::actions![command]),
            Node {
                min_width: px(0),
                max_width: percent(100),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let label = crate::edit_mode::label(world, entity, text, 16.0);
    world.entity_mut(label).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    let mut node = world.get_mut::<Node>(label).unwrap();
    node.min_width = px(0);
    node.flex_shrink = 1.0;
    entity
}

fn checkbox(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    checked: bool,
    command: Command,
    label: &str,
    enabled: bool,
) {
    let entity = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            crate::sand::Borderless,
            ActionButton::new(owner, crate::actions![command]),
            Node {
                width: px(16),
                height: px(16),
                margin: UiRect::all(px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::Ink),
            ChildOf(parent),
        ))
        .id();
    accessible(world, entity, accesskit::Role::CheckBox, label);
    let mut accessible = world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap();
    accessible.set_toggled(if checked {
        accesskit::Toggled::True
    } else {
        accesskit::Toggled::False
    });
    if checked {
        glyph(world, entity, Icon::Check, 14.0);
    }
    if !enabled {
        world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
}

fn accessible(world: &mut World, entity: Entity, role: accesskit::Role, label: &str) {
    let mut node = accesskit::Node::new(role);
    node.set_label(label);
    world
        .entity_mut(entity)
        .insert(bevy::a11y::AccessibilityNode(node));
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

pub(super) fn search(world: &mut World, parent: Entity, owner: Entity) {
    let group = row(world, parent);
    {
        let mut node = world.get_mut::<Node>(group).unwrap();
        node.width = Val::Auto;
        node.margin = UiRect::left(Val::Auto);
        node.flex_shrink = 1.0;
    }
    glyph(world, group, Icon::Search, 17.0);
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
            width: px(200),
            min_width: px(0),
            min_height: px(30),
            ..default()
        },
        ChildOf(group),
        Search(owner),
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = false;
    editor.max_characters = Some(256);
    editor.visible_lines = Some(1.0);
    accessible(world, entity, accesskit::Role::TextInput, "Filter rules");
}

#[derive(Component)]
struct SelectionHeading;

pub(super) fn headings(world: &mut World, parent: Entity) {
    let heading = grid(world, parent);
    let selector = stack(world, heading);
    world.entity_mut(selector).insert(SelectionHeading);
    for (index, title) in ["Name / Slug", "Condition", "Threshold", "Consequence"]
        .into_iter()
        .enumerate()
    {
        let cell = cell(world, heading);
        let line = row(world, cell);
        let label = crate::edit_mode::label(world, line, title, 14.0);
        world.entity_mut(label).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
        let mut node = world.get_mut::<Node>(label).unwrap();
        node.min_width = px(0);
        node.flex_shrink = 1.0;
        if index > 0 {
            let explanation = [
                "The condition calculates a quantity from numbers, record quantities and frequencies. For example, @balance * freq(@weekly) uses the balance when the weekly frequency fires.",
                "The threshold decides whether the calculated quantity should trigger the consequence. Use >0 for positive values, >=10 for at least ten, !=0 for any nonzero value, or always.",
                "The consequence chooses the record to change. @target sets its quantity to the calculated value; @target: command(\"…\") runs a command on that record.",
            ][index - 1];
            let info = world
                .spawn((
                    crate::sand::button(0),
                    crate::sand::Borderless,
                    crate::icons::InlineTooltip,
                    Tooltip(explanation.into()),
                    Node {
                        width: px(14),
                        height: px(14),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ChildOf(line),
                ))
                .id();
            world
                .entity_mut(info)
                .insert(crate::icons::TooltipIcon { source: info });
            accessible(
                world,
                info,
                accesskit::Role::Button,
                &format!("About {title}"),
            );
            glyph(world, info, Icon::Info, 14.0);
        }
    }
}

fn visible_rules(world: &World, owner: Entity) -> Vec<Rule> {
    let query = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .search
        .trim()
        .to_lowercase();
    world
        .get::<View>(owner)
        .unwrap()
        .rules
        .iter()
        .filter(|rule| {
            format!("{} {}", rule.name, rule.slug)
                .to_lowercase()
                .contains(&query)
                || rule
                    .fields
                    .iter()
                    .any(|field| field.source.to_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}

pub(super) fn render_controls(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let (controls, selected, busy) = (view.controls, view.selection.len(), view.pending.is_some());
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let creating = castle.draft.is_some();
    let editing = !castle.edits.is_empty();
    clear(world, controls);
    if creating || editing {
        for (command, title) in [
            (Command::Save, if creating { "Create" } else { "Save" }),
            (Command::Cancel, "Cancel"),
        ] {
            let entity = button(world, controls, owner, command, title);
            if busy {
                world
                    .entity_mut(entity)
                    .insert(bevy::ui::InteractionDisabled);
            }
        }
    } else {
        icon_button(
            world,
            controls,
            owner,
            Icon::Plus,
            "Create a rule",
            Command::New,
            !busy,
        );
    }
    let enabled = !busy && !creating && !editing;
    icon_button(
        world,
        controls,
        owner,
        Icon::Pencil,
        "Edit selected rules",
        Command::EditSelected,
        enabled && selected > 0,
    );
    icon_button(
        world,
        controls,
        owner,
        Icon::Copy,
        "Copy selected rule",
        Command::CopySelected,
        enabled && selected == 1,
    );
    icon_button(
        world,
        controls,
        owner,
        Icon::Close,
        "Delete selected rules",
        Command::DeleteSelected,
        enabled && selected > 0,
    );
    let selector = world
        .query_filtered::<Entity, With<SelectionHeading>>()
        .iter(world)
        .find(|entity| descendant(world, Some(*entity), owner));
    if let Some(selector) = selector {
        clear(world, selector);
        let rules = visible_rules(world, owner);
        let checked = !rules.is_empty()
            && rules.iter().all(|rule| {
                world
                    .get::<View>(owner)
                    .unwrap()
                    .selection
                    .contains(&rule.uid)
            });
        checkbox(
            world,
            selector,
            owner,
            checked,
            Command::SelectAll,
            "Select all visible rules",
            !busy && !editing && !rules.is_empty(),
        );
    }
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    let view = world.get::<View>(owner).unwrap();
    let (list, selected, busy) = (view.list, view.selection.clone(), view.pending.is_some());
    let rules = visible_rules(world, owner);
    let edits = world.get::<KarmaCastle>(owner).unwrap().edits.clone();
    clear(world, list);
    for rule in rules {
        let cells = grid(world, list);
        checkbox(
            world,
            cells,
            owner,
            selected.contains(&rule.uid),
            Command::Select(rule.uid.clone()),
            &format!("Select {}", rule.name),
            !busy && edits.is_empty(),
        );
        if let Some(index) = edits
            .iter()
            .position(|draft| draft.rule.as_ref() == Some(&rule.uid))
        {
            editor_cells(world, cells, owner, index + 1, &edits[index]);
        } else {
            let identity = cell(world, cells);
            let line = row(world, identity);
            for text in [rule.name.as_str(), &format!("@{}", rule.slug)] {
                let label = crate::edit_mode::label(world, line, text, 16.0);
                world.entity_mut(label).insert(TextLayout::linebreak(
                    bevy::text::LineBreak::WordOrCharacter,
                ));
                let mut node = world.get_mut::<Node>(label).unwrap();
                node.min_width = px(0);
                node.flex_shrink = 1.0;
            }
            let paused = rule.state == "paused";
            let toggle = icon_button(
                world,
                line,
                owner,
                if paused { Icon::Play } else { Icon::Stop },
                if paused { "Resume rule" } else { "Pause rule" },
                Command::Pause(rule.uid.clone(), rule.revision, !paused),
                !busy,
            );
            world.get_mut::<Node>(toggle).unwrap().width = px(16);
            world.get_mut::<Node>(toggle).unwrap().height = px(16);
            let glyph = world.get::<Children>(toggle).unwrap()[0];
            world.get_mut::<Node>(glyph).unwrap().width = px(16);
            world.get_mut::<Node>(glyph).unwrap().height = px(16);
            for kind in RuleFieldKind::ALL {
                let cell = cell(world, cells);
                if let Some(field) = rule.fields.iter().find(|field| field.kind == kind) {
                    rich_text(world, cell, owner, &field.source);
                }
            }
        }
    }
    render_controls(world, owner);
}

pub(super) fn render_form(world: &mut World, owner: Entity) {
    let form = world.get::<View>(owner).unwrap().form;
    clear(world, form);
    let draft = world.get::<KarmaCastle>(owner).unwrap().draft.clone();
    if let Some(draft) = draft {
        let cells = grid(world, form);
        cell(world, cells);
        editor_cells(world, cells, owner, 0, &draft);
    }
    let deleting = world.get::<View>(owner).unwrap().deleting.len();
    if deleting > 0 {
        let line = row(world, form);
        crate::edit_mode::label(
            world,
            line,
            &format!("Delete {deleting} selected rule(s)?"),
            14.0,
        );
        button(world, line, owner, Command::ConfirmDelete, "Delete");
        button(world, line, owner, Command::CancelDelete, "Cancel");
    }
    render_controls(world, owner);
}

fn editor_cells(world: &mut World, cells: Entity, owner: Entity, row: usize, draft: &Draft) {
    let identity = cell(world, cells);
    let line = self::row(world, identity);
    editor(world, line, owner, row, 3, &draft.name);
    editor(world, line, owner, row, 4, &draft.slug);
    for (index, field) in draft.fields.iter().enumerate() {
        let cell = cell(world, cells);
        editor(world, cell, owner, row, index, &field.text);
    }
}

fn editor(world: &mut World, parent: Entity, owner: Entity, row: usize, index: usize, value: &str) {
    let host = stack(world, parent);
    world.get_mut::<Node>(host).unwrap().flex_shrink = 1.0;
    let entity = world
        .spawn(crate::sand::text_editor(
            value,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    let font = world
        .resource::<crate::theme::Typography>()
        .text(if index < 3 { 14.0 } else { 16.0 });
    world.entity_mut(entity).insert((
        font,
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
    ));
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_width: px(0),
            min_height: px(36),
            ..default()
        },
        ChildOf(host),
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = false;
    editor.max_characters = Some(if index < 3 { 16_384 } else { 256 });
    editor.visible_lines = Some(2.0);
    accessible(
        world,
        entity,
        accesskit::Role::TextInput,
        ["Condition", "Threshold", "Consequence", "Name", "Slug"][index],
    );
    if world.get::<View>(owner).unwrap().pending.is_some() {
        world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
    let suggestions = stack(world, host);
    {
        let mut node = world.get_mut::<Node>(suggestions).unwrap();
        node.position_type = PositionType::Absolute;
        node.top = percent(100);
        node.left = px(0);
        node.min_width = px(180);
        node.max_height = px(220);
        node.overflow = Overflow::scroll_y();
        node.display = Display::None;
    }
    world.entity_mut(suggestions).insert((
        GlobalZIndex(30),
        ScrollPosition::default(),
        crate::token_style::background(crate::tokens::Token::Surface),
    ));
    crate::scroll_sand::attach(world, suggestions);
    world.entity_mut(entity).insert(Input {
        owner,
        row,
        index,
        suggestions,
        observed: value.into(),
        focused: false,
        selection: 0..0,
        dirty: true,
    });
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.row, input.index, text.value().to_string()))
        .collect();
    let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
    for (row, index, text) in values {
        if let Some(draft) = draft_mut(&mut castle, row) {
            set_value(draft, index, text);
        }
    }
}

fn set_value(draft: &mut Draft, index: usize, text: String) {
    match index {
        3 => draft.name = text,
        4 => draft.slug = text,
        _ => {
            let field = &mut draft.fields[index];
            if field.text != text {
                field.linked = None;
                field.text = text;
            }
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
        capture(world, owner);
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
                        input.row,
                        input.index,
                        input.suggestions,
                        text.value().to_string(),
                        focused,
                        selection,
                    )
                })
        })
        .collect();
    for (entity, owner, row, index, suggestions, text, focused, selection) in inputs {
        let mut input = world.get_mut::<Input>(entity).unwrap();
        input.observed = text.clone();
        input.dirty = false;
        input.focused = focused;
        input.selection = selection.clone();
        if let Some(draft) = draft_mut(&mut world.get_mut::<KarmaCastle>(owner).unwrap(), row) {
            set_value(draft, index, text.clone());
        }
        clear(world, suggestions);
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
        if !focused || index >= 3 || world.get::<View>(owner).unwrap().pending.is_some() {
            continue;
        }
        let view = world.get::<View>(owner).unwrap();
        let prefix = text.get(..selection.end.min(text.len())).unwrap_or(&text);
        let choices = model::suggestions(
            RuleFieldKind::ALL[index],
            prefix,
            &view.rules,
            &view.records,
            &view.frequencies,
        );
        if !choices.is_empty() {
            world.get_mut::<Node>(suggestions).unwrap().display = Display::Flex;
        }
        for choice in choices {
            let (command, label) = match choice {
                Suggestion::Shared(field) => {
                    let label = format!("Link · {}", field.source);
                    (Command::Link(row, index, field), label)
                }
                Suggestion::Element(element) => (
                    Command::Insert(row, index, element.clone(), selection.clone()),
                    element,
                ),
            };
            button(world, suggestions, owner, command, &label);
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
    for input in world
        .query::<&mut Input>()
        .iter_mut(world)
        .filter(|input| input.owner == owner)
    {
        input.into_inner().dirty = true;
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
        world.entity_mut(part).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
        let mut node = world.get_mut::<Node>(part).unwrap();
        node.flex_shrink = 1.0;
        node.min_width = px(0);
        node.max_width = percent(100);
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
