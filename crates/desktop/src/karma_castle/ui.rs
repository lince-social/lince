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
    options: Vec<(Entity, Command)>,
    selected: usize,
    dismissed: bool,
}

#[derive(Component)]
struct Search(Entity);

#[derive(Component)]
struct TableRow(Entity);

#[derive(Component)]
struct SelectionHeader(Entity);

#[derive(Component)]
struct RuleRow;

#[derive(Component)]
struct RowAction {
    row: Entity,
    owner: Entity,
}

pub(super) fn hover_actions(world: &mut World) {
    let mut hovered = HashSet::new();
    let mut hits: Vec<_> = world
        .get_resource::<bevy::picking::hover::HoverMap>()
        .map(|map| map.values().flat_map(|hits| hits.keys().copied()).collect())
        .unwrap_or_default();
    if let Some(focus) = world.get_resource::<InputFocus>().and_then(InputFocus::get) {
        hits.push(focus);
    }
    for hit in hits {
        let mut entity = Some(hit);
        while let Some(current) = entity {
            if world.get::<RuleRow>(current).is_some() {
                hovered.insert(current);
                break;
            }
            entity = world.get::<ChildOf>(current).map(ChildOf::parent);
        }
    }
    let buttons: Vec<_> = world
        .query::<(Entity, &RowAction)>()
        .iter(world)
        .map(|(entity, action)| (entity, action.row, action.owner))
        .collect();
    for (entity, row, owner) in buttons {
        let visible = hovered.contains(&row)
            && world
                .get::<View>(owner)
                .is_some_and(|view| view.pending.is_none());
        world.entity_mut(entity).insert(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if visible {
            world
                .entity_mut(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        } else {
            world
                .entity_mut(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
    }
}

fn reset_columns(world: &mut World, owner: Entity) {
    let rows: Vec<_> = world
        .query::<(Entity, &TableRow)>()
        .iter(world)
        .filter(|(_, row)| row.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    for row in rows {
        let mut tracks = vec![GridTrack::max_content(); 6];
        tracks[0] = GridTrack::px(29.0);
        world.get_mut::<Node>(row).unwrap().grid_template_columns = tracks;
    }
}

pub(super) fn fit_columns(world: &mut World) {
    let mut widths = HashMap::<Entity, [f32; 6]>::new();
    let rows: Vec<_> = world
        .query::<(Entity, &TableRow, &Children)>()
        .iter(world)
        .map(|(entity, row, children)| (entity, row.0, children.to_vec()))
        .collect();
    for (_, owner, children) in &rows {
        let columns = widths
            .entry(*owner)
            .or_insert([29.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        for (index, child) in children.iter().take(6).enumerate().skip(1) {
            if let Some(node) = world.get::<ComputedNode>(*child) {
                columns[index] = columns[index].max(node.size().x * node.inverse_scale_factor());
            }
        }
    }
    for (entity, owner, _) in rows {
        if let Some(widths) = widths.get(&owner)
            && widths.iter().all(|width| *width > 0.0)
        {
            let tracks: Vec<_> = widths.iter().map(|width| GridTrack::px(*width)).collect();
            let mut node = world.get_mut::<Node>(entity).unwrap();
            if node.grid_template_columns != tracks {
                node.grid_template_columns = tracks;
            }
        }
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
pub(crate) enum Command {
    Create,
    Simulate,
    Schedules,
    Commands,
    Select(String),
    SelectAll,
    DeleteSelected,
    PauseSelected,
    RequestPause(String, i64, bool),
    ConfirmPause,
    CancelPause,
    New,
    Cancel,
    Save,
    EditCell(String, usize),
    Delete(String),
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
            Self::Simulate => {
                super::preview_ui::toggle(world, owner);
                return;
            }
            Self::Schedules => {
                super::schedules_ui::toggle(world, owner);
                return;
            }
            Self::Commands => {
                super::commands_ui::toggle(world, owner);
                return;
            }
            Self::Select(uid) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                if !view.selected.remove(uid) {
                    view.selected.insert(uid.clone());
                }
            }
            Self::SelectAll => {
                let rules = visible_rules(world, owner);
                let mut view = world.get_mut::<View>(owner).unwrap();
                let all = rules.iter().all(|rule| view.selected.contains(&rule.uid));
                for rule in rules {
                    if all {
                        view.selected.remove(&rule.uid);
                    } else {
                        view.selected.insert(rule.uid);
                    }
                }
            }
            Self::DeleteSelected => {
                let rules = visible_rules(world, owner);
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.pausing.clear();
                view.deleting = rules
                    .into_iter()
                    .filter(|rule| view.selected.contains(&rule.uid))
                    .map(|rule| rule.uid)
                    .collect();
            }
            Self::PauseSelected => {
                let rules = visible_rules(world, owner);
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.deleting.clear();
                view.pausing = rules
                    .into_iter()
                    .filter(|rule| view.selected.contains(&rule.uid) && rule.state != "paused")
                    .map(|rule| (rule.uid, rule.revision, true))
                    .collect();
            }
            Self::RequestPause(uid, revision, paused) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.deleting.clear();
                view.pausing = vec![(uid.clone(), *revision, *paused)];
            }
            Self::ConfirmPause => {
                let next = {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pause_active = !view.pausing.is_empty();
                    if view.pause_active {
                        Some(view.pausing.remove(0))
                    } else {
                        None
                    }
                };
                if let Some((uid, revision, paused)) = next {
                    Self::Pause(uid, revision, paused).apply(world, owner);
                    return;
                }
            }
            Self::CancelPause => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.pausing.clear();
                view.pause_active = false;
            }
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
                world.get_mut::<View>(owner).unwrap().editing = None;
                status(world, owner, "");
            }
            Self::Save => {
                save(world, owner);
                return;
            }
            Self::EditCell(uid, index) => {
                if *index > 4 || world.get::<KarmaCastle>(owner).unwrap().draft.is_some() {
                    return;
                }
                let Some(rule) = world
                    .get::<View>(owner)
                    .unwrap()
                    .rules
                    .iter()
                    .find(|rule| &rule.uid == uid)
                    .cloned()
                else {
                    return;
                };
                let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                if !castle
                    .edits
                    .iter()
                    .any(|draft| draft.rule.as_ref() == Some(uid))
                {
                    castle.edits.push(Draft::from_rule(&rule));
                }
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.editing = Some((uid.clone(), *index));
                view.deleting.clear();
                view.pausing.clear();
            }
            Self::Delete(uid) => {
                if world
                    .get::<View>(owner)
                    .unwrap()
                    .rules
                    .iter()
                    .any(|rule| &rule.uid == uid)
                {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pausing.clear();
                    view.deleting = vec![uid.clone()];
                }
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
                    owner,
                    ClientMessage::Act {
                        id: id.clone(),
                        action,
                    },
                ) {
                    Ok(()) => {
                        world.get_mut::<View>(owner).unwrap().pending = Some(id);
                    }
                    Err(error) => {
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        view.pause_active = false;
                        view.pausing.clear();
                        status(world, owner, error);
                    }
                }
            }
        }
        if matches!(self, Self::New) {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.deleting.clear();
            view.pausing.clear();
            let form = world.get::<View>(owner).unwrap().form;
            let scroll = world.get::<ChildOf>(form).unwrap().parent();
            world.get_mut::<ScrollPosition>(scroll).unwrap().0 = Vec2::ZERO;
        }
        render_form(world, owner);
        render_list(world, owner);
        if let Self::EditCell(uid, index) = self {
            let row = world
                .get::<KarmaCastle>(owner)
                .unwrap()
                .edits
                .iter()
                .position(|draft| draft.rule.as_ref() == Some(uid))
                .map(|row| row + 1);
            let input = world
                .query::<(Entity, &Input)>()
                .iter(world)
                .find(|(_, input)| {
                    input.owner == owner && Some(input.row) == row && input.index == *index
                })
                .map(|(entity, _)| entity);
            if let Some(input) = input {
                world
                    .resource_mut::<InputFocus>()
                    .set(input, bevy::input_focus::FocusCause::Navigated);
            }
        }
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
    let mut owner = parent;
    while world.get::<KarmaCastle>(owner).is_none() {
        owner = world.get::<ChildOf>(owner).unwrap().parent();
    }
    world
        .spawn((
            TableRow(owner),
            Node {
                display: Display::Grid,
                width: Val::Auto,
                min_width: px(0),
                align_self: AlignSelf::Start,
                padding: UiRect::horizontal(px(22)),
                justify_content: JustifyContent::Start,
                flex_shrink: 0.0,
                grid_template_columns: vec![
                    GridTrack::px(29.0),
                    GridTrack::max_content(),
                    GridTrack::max_content(),
                    GridTrack::max_content(),
                    GridTrack::max_content(),
                    GridTrack::max_content(),
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
    node.width = Val::Auto;
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
    node.min_width = Val::Auto;
    node.flex_shrink = 1.0;
    entity
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

pub(super) fn headings(world: &mut World, parent: Entity) {
    let heading = grid(world, parent);
    let selection = cell(world, heading);
    let mut owner = parent;
    while world.get::<KarmaCastle>(owner).is_none() {
        owner = world.get::<ChildOf>(owner).unwrap().parent();
    }
    let checkbox = checkbox(
        world,
        selection,
        owner,
        Command::SelectAll,
        "Select all rules",
        false,
    );
    world.entity_mut(checkbox).insert(SelectionHeader(owner));
    for (index, title) in ["Name", "Slug", "Condition", "Threshold", "Consequence"]
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
        node.min_width = Val::Auto;
        node.flex_shrink = 1.0;
        if index > 1 {
            let explanation = [
                "The condition calculates a quantity from numbers, record quantities and frequencies. For example, @balance * freq(@weekly) uses the balance when the weekly frequency fires.",
                "The threshold decides whether the calculated quantity should trigger the consequence. Use >0 for positive values, >=10 for at least ten, !=0 for any nonzero value, or always.",
                "The consequence chooses the record to change. @target sets its quantity to the calculated value; @target: command(\"…\") runs a command on that record.",
            ][index - 2];
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

fn checkbox(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    label: &str,
    checked: bool,
) -> Entity {
    let entity = icon_button(
        world,
        parent,
        owner,
        if checked { Icon::Check } else { Icon::Square },
        label,
        command,
        true,
    );
    size_icon(world, entity, 16.0);
    accessible(world, entity, accesskit::Role::CheckBox, label);
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    entity
}

fn update_selection(world: &mut World, owner: Entity) {
    let rules = visible_rules(world, owner);
    let selected = &world.get::<View>(owner).unwrap().selected;
    let count = rules
        .iter()
        .filter(|rule| selected.contains(&rule.uid))
        .count();
    let all = !rules.is_empty() && count == rules.len();
    let headers: Vec<_> = world
        .query::<(Entity, &SelectionHeader)>()
        .iter(world)
        .filter(|(_, header)| header.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    for entity in headers {
        let glyph = world.get::<Children>(entity).unwrap()[0];
        let icon = if all {
            Icon::Check
        } else if count > 0 {
            Icon::Minus
        } else {
            Icon::Square
        };
        let image = crate::icons::image(world, icon).unwrap_or_default();
        world.entity_mut(glyph).insert(image);
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(entity)
            .unwrap()
            .set_toggled(if all {
                accesskit::Toggled::True
            } else if count > 0 {
                accesskit::Toggled::Mixed
            } else {
                accesskit::Toggled::False
            });
    }
}

fn rule_label(rule: &Rule) -> &str {
    if !rule.name.is_empty() {
        &rule.name
    } else if !rule.slug.is_empty() {
        &rule.slug
    } else {
        &rule.uid
    }
}

pub(super) fn render_tools(world: &mut World, owner: Entity) {
    let tools = world.get::<View>(owner).unwrap().tools;
    clear(world, tools);
    for (command, label) in [
        (Command::Schedules, "Scheduled changes"),
        (Command::Commands, "Commands"),
        (Command::Simulate, "Simulation"),
    ] {
        button(world, tools, owner, command, label);
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
    let (controls, busy) = (view.controls, view.pending.is_some());
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let creating = castle.draft.is_some();
    let editing = !castle.edits.is_empty();
    let selected = visible_rules(world, owner)
        .iter()
        .any(|rule| view.selected.contains(&rule.uid));
    let can_pause = visible_rules(world, owner)
        .iter()
        .any(|rule| view.selected.contains(&rule.uid) && rule.state != "paused");
    clear(world, controls);
    if selected && !creating && !editing {
        for (command, label) in [
            (Command::PauseSelected, "Pause selected"),
            (Command::DeleteSelected, "Delete selected"),
        ] {
            let disabled = busy || (matches!(command, Command::PauseSelected) && !can_pause);
            let entity = button(world, controls, owner, command, label);
            if disabled {
                world
                    .entity_mut(entity)
                    .insert(bevy::ui::InteractionDisabled);
            }
        }
    }
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
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    reset_columns(world, owner);
    let known: HashSet<_> = world
        .get::<View>(owner)
        .unwrap()
        .rules
        .iter()
        .map(|rule| rule.uid.clone())
        .collect();
    world
        .get_mut::<View>(owner)
        .unwrap()
        .selected
        .retain(|uid| known.contains(uid));
    let view = world.get::<View>(owner).unwrap();
    let (list, busy, editing) = (view.list, view.pending.is_some(), view.editing.clone());
    let rules = visible_rules(world, owner);
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let edits = castle.edits.clone();
    let creating = castle.draft.is_some();
    clear(world, list);
    update_selection(world, owner);
    for rule in rules {
        let cells = grid(world, list);
        world.entity_mut(cells).insert(RuleRow);
        let selection = cell(world, cells);
        let selected = world
            .get::<View>(owner)
            .unwrap()
            .selected
            .contains(&rule.uid);
        let checkbox = checkbox(
            world,
            selection,
            owner,
            Command::Select(rule.uid.clone()),
            &format!("Select {}", rule_label(&rule)),
            selected,
        );
        if busy {
            world
                .entity_mut(checkbox)
                .insert(bevy::ui::InteractionDisabled);
        }
        let edited = edits
            .iter()
            .position(|draft| draft.rule.as_ref() == Some(&rule.uid));
        let value = edited
            .map(|index| edits[index].clone())
            .unwrap_or_else(|| Draft::from_rule(&rule));
        for index in [3, 4, 0, 1, 2] {
            let cell = cell(world, cells);
            let active = editing.as_ref() == Some(&(rule.uid.clone(), index));
            let content = if index >= 3 {
                let identity = row(world, cell);
                let text = if index == 3 {
                    value.name.clone()
                } else if value.slug.is_empty() {
                    String::new()
                } else {
                    format!("@{}", value.slug)
                };
                {
                    let label = crate::edit_mode::label(world, identity, &text, 16.0);
                    world.entity_mut(label).insert(TextLayout::linebreak(
                        bevy::text::LineBreak::WordOrCharacter,
                    ));
                    let mut node = world.get_mut::<Node>(label).unwrap();
                    node.min_width = Val::Auto;
                    node.flex_shrink = 1.0;
                }
                identity
            } else {
                rich_text(
                    world,
                    cell,
                    owner,
                    &value.fields[index].text,
                    value.rule.as_deref(),
                    index,
                )
            };
            if active && let Some(edited) = edited {
                world.entity_mut(content).insert(Visibility::Hidden);
                let overlay = row(world, cell);
                *world.get_mut::<Node>(overlay).unwrap() = Node {
                    position_type: PositionType::Absolute,
                    left: px(6),
                    right: px(6),
                    top: px(6),
                    bottom: px(6),
                    min_width: px(0),
                    column_gap: px(8),
                    ..default()
                };
                if index >= 3 {
                    editor(
                        world,
                        overlay,
                        owner,
                        edited + 1,
                        index,
                        if index == 3 { &value.name } else { &value.slug },
                    );
                } else {
                    editor(
                        world,
                        overlay,
                        owner,
                        edited + 1,
                        index,
                        &value.fields[index].text,
                    );
                }
            } else if !busy && !creating {
                world.entity_mut(cell).insert((
                    crate::sand::button(0),
                    crate::sand::Borderless,
                    ActionButton::new(
                        owner,
                        crate::actions![Command::EditCell(rule.uid.clone(), index)],
                    ),
                ));
                accessible(
                    world,
                    cell,
                    accesskit::Role::Button,
                    &format!(
                        "Edit {} for {}",
                        ["condition", "threshold", "consequence", "name", "slug"][index],
                        rule_label(&rule)
                    ),
                );
            }
        }
        let paused = rule.state == "paused";
        for (left, icon, label, command) in [
            (
                true,
                if paused { Icon::Play } else { Icon::Stop },
                if paused {
                    "Resume rule".to_owned()
                } else {
                    "Pause rule".to_owned()
                },
                Command::RequestPause(rule.uid.clone(), rule.revision, !paused),
            ),
            (
                false,
                Icon::Close,
                format!("Delete {}", rule_label(&rule)),
                Command::Delete(rule.uid.clone()),
            ),
        ] {
            let entity = icon_button(world, cells, owner, icon, &label, command, !busy);
            size_icon(world, entity, 16.0);
            {
                let mut node = world.get_mut::<Node>(entity).unwrap();
                node.position_type = PositionType::Absolute;
                node.top = px(8);
                if left {
                    node.left = px(2);
                } else {
                    node.right = px(2);
                }
            }
            world.entity_mut(entity).insert((
                RowAction { row: cells, owner },
                Visibility::Hidden,
                bevy::ui::InteractionDisabled,
            ));
        }
    }
    render_controls(world, owner);
}

fn size_icon(world: &mut World, entity: Entity, size: f32) {
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = px(size);
    node.height = px(size);
    let glyph = world.get::<Children>(entity).unwrap()[0];
    let mut node = world.get_mut::<Node>(glyph).unwrap();
    node.width = px(size);
    node.height = px(size);
}

pub(super) fn render_form(world: &mut World, owner: Entity) {
    reset_columns(world, owner);
    let form = world.get::<View>(owner).unwrap().form;
    clear(world, form);
    let draft = world.get::<KarmaCastle>(owner).unwrap().draft.clone();
    if let Some(draft) = draft {
        let cells = grid(world, form);
        cell(world, cells);
        editor_cells(world, cells, owner, 0, &draft);
    }
    let view = world.get::<View>(owner).unwrap();
    let deleting = view
        .deleting
        .first()
        .and_then(|uid| view.rules.iter().find(|rule| &rule.uid == uid))
        .map(|rule| rule_label(rule).to_owned());
    if let Some(name) = deleting {
        let count = view.deleting.len();
        let message = if count == 1 {
            format!("Delete {name}?")
        } else {
            format!("Delete {count} selected rules?")
        };
        let line = row(world, form);
        crate::edit_mode::label(world, line, &message, 14.0);
        button(world, line, owner, Command::ConfirmDelete, "Delete");
        button(world, line, owner, Command::CancelDelete, "Cancel");
    }
    let view = world.get::<View>(owner).unwrap();
    if !view.pause_active && !view.pausing.is_empty() {
        let count = view.pausing.len();
        let verb = if view.pausing[0].2 { "Pause" } else { "Resume" };
        let name = view
            .rules
            .iter()
            .find(|rule| rule.uid == view.pausing[0].0)
            .map(rule_label)
            .unwrap_or("rule");
        let message = if count == 1 {
            format!("{verb} {name}?")
        } else {
            format!("{verb} {count} selected rules?")
        };
        let line = row(world, form);
        crate::edit_mode::label(world, line, &message, 14.0);
        button(world, line, owner, Command::ConfirmPause, verb);
        button(world, line, owner, Command::CancelPause, "Cancel");
    }
    render_controls(world, owner);
}

fn editor_cells(world: &mut World, cells: Entity, owner: Entity, row: usize, draft: &Draft) {
    let identity = cell(world, cells);
    editor(world, identity, owner, row, 3, &draft.name);
    let slug = cell(world, cells);
    editor(world, slug, owner, row, 4, &draft.slug);
    for (index, field) in draft.fields.iter().enumerate() {
        let cell = cell(world, cells);
        editor(world, cell, owner, row, index, &field.text);
    }
}

fn editor(world: &mut World, parent: Entity, owner: Entity, row: usize, index: usize, value: &str) {
    let host = stack(world, parent);
    world.get_mut::<Node>(host).unwrap().flex_shrink = 1.0;
    if row > 0 && index < 3 {
        world.get_mut::<Node>(host).unwrap().height = percent(100);
    }
    if index >= 3 {
        crate::edit_mode::label(
            world,
            host,
            if index == 3 {
                "Name (optional)"
            } else {
                "Slug (optional)"
            },
            12.0,
        );
    }
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
            height: if row == 0 || index >= 3 {
                Val::Auto
            } else {
                percent(100)
            },
            min_height: if row == 0 || index >= 3 {
                px(20)
            } else {
                Val::Auto
            },
            overflow: Overflow::clip(),
            ..default()
        },
        ChildOf(host),
        crate::sand::Borderless,
    ));
    let mut editor = world.get_mut::<EditableText>(entity).unwrap();
    editor.allow_newlines = false;
    editor.max_characters = Some(if index < 3 { 16_384 } else { 256 });
    editor.visible_lines = Some(1.0);
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
        node.border = UiRect::all(px(1));
        node.padding = UiRect::all(px(4));
        node.row_gap = px(2);
    }
    world.entity_mut(suggestions).insert((
        GlobalZIndex(30),
        ScrollPosition::default(),
        crate::token_style::background(crate::tokens::Token::Surface),
        crate::token_style::border(crate::tokens::Token::Accent),
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
        options: Vec::new(),
        selected: 0,
        dismissed: false,
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
        if text != input.observed || focused != input.focused {
            input.dismissed = false;
        }
        let dismissed = input.dismissed;
        input.options.clear();
        input.selected = 0;
        input.observed = text.clone();
        input.dirty = false;
        input.focused = focused;
        input.selection = selection.clone();
        if let Some(draft) = draft_mut(&mut world.get_mut::<KarmaCastle>(owner).unwrap(), row) {
            set_value(draft, index, text.clone());
        }
        clear(world, suggestions);
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
        if !focused
            || dismissed
            || index >= 3
            || world.get::<View>(owner).unwrap().pending.is_some()
        {
            continue;
        }
        let view = world.get::<View>(owner).unwrap();
        let prefix = text.get(..selection.end.min(text.len())).unwrap_or(&text);
        let mut choices = model::suggestions(
            RuleFieldKind::ALL[index],
            prefix,
            &view.rules,
            &view.records,
            &view.frequencies,
        );
        let tail = prefix
            .rsplit(|character: char| {
                character.is_whitespace()
                    || matches!(character, '(' | ')' | '*' | '/' | '+' | '=' | ',')
            })
            .next()
            .unwrap_or("")
            .to_lowercase();
        choices.extend(
            model::transfers::elements(
                RuleFieldKind::ALL[index],
                &view.transfers,
                view.acting_person.as_deref(),
            )
            .into_iter()
            .filter(|element| {
                element.to_lowercase().contains(&tail)
                    || model::transfers::label(element, &view.transfers)
                        .to_lowercase()
                        .contains(&tail)
            })
            .take(16)
            .map(Suggestion::Element),
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
                    model::transfers::label(
                        &model::element_label(&element, &world.get::<View>(owner).unwrap().records),
                        &world.get::<View>(owner).unwrap().transfers,
                    ),
                ),
            };
            let option = button(world, suggestions, owner, command.clone(), &label);
            world.get_mut::<Node>(option).unwrap().width = percent(100);
            world
                .get_mut::<Input>(entity)
                .unwrap()
                .options
                .push((option, command));
        }
        highlight(world, entity);
    }
}

fn highlight(world: &mut World, entity: Entity) {
    let input = world.get::<Input>(entity).unwrap();
    let selected = input.selected;
    let options: Vec<_> = input.options.iter().map(|(entity, _)| *entity).collect();
    for (index, option) in options.into_iter().enumerate() {
        world
            .entity_mut(option)
            .insert(crate::token_style::background(if index == selected {
                crate::tokens::Token::Accent
            } else {
                crate::tokens::Token::Surface
            }));
    }
}

pub(super) fn keys(world: &mut World) {
    let Some(focused) = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
    else {
        return;
    };
    let Some(input) = world.get::<Input>(focused) else {
        return;
    };
    if input.options.is_empty()
        || world
            .get::<View>(input.owner)
            .is_none_or(|view| view.pending.is_some())
    {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    if keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
    ]) {
        return;
    }
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    let escape = keys.just_pressed(KeyCode::Escape);
    let step = i32::from(keys.just_pressed(KeyCode::ArrowDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp));
    if !enter && !escape && step == 0 {
        return;
    }
    let Some(mut text) = world.get_mut::<EditableText>(focused) else {
        return;
    };
    if text.is_composing() || text.pending_paste.is_some() {
        return;
    }
    text.pending_edits.retain(|edit| {
        !(step != 0 && matches!(edit, bevy::text::TextEdit::Up(_) | bevy::text::TextEdit::Down(_))
            || enter && matches!(edit, bevy::text::TextEdit::Insert(value) if value.contains(['\n', '\r'])))
    });
    let mut input = world.get_mut::<Input>(focused).unwrap();
    if escape {
        input.dismissed = true;
        let suggestions = input.suggestions;
        input.options.clear();
        world.get_mut::<Node>(suggestions).unwrap().display = Display::None;
        return;
    }
    input.selected = (input.selected as i32 + step).rem_euclid(input.options.len() as i32) as usize;
    let owner = input.owner;
    let command = input.options[input.selected].1.clone();
    if enter {
        command.apply(world, owner);
    } else {
        highlight(world, focused);
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

fn rich_text(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    source: &str,
    rule_uid: Option<&str>,
    index: usize,
) -> Entity {
    let rule = world
        .get::<View>(owner)
        .and_then(|view| {
            view.rules
                .iter()
                .find(|rule| Some(rule.uid.as_str()) == rule_uid)
        })
        .cloned();
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
        node.min_width = Val::Auto;
        node.max_width = percent(100);
        if let Some(slug) = slug {
            let reading = model::reading_at(source, offset, offset + text.len());
            let reading = rule.as_ref().map_or(reading.clone(), |rule| {
                if index == 2 && !rule.record.is_empty() {
                    format!("@{}", rule.record)
                } else {
                    model::bound_reading(rule, &reading)
                }
            });
            let slug = rule
                .as_ref()
                .and_then(|rule| {
                    if index == 2 {
                        Some(rule.record.clone()).filter(|uid| !uid.is_empty())
                    } else {
                        rule.bindings
                            .iter()
                            .find(|binding| {
                                binding.authored == slug
                                    && reading.contains(binding.target.as_str())
                            })
                            .map(|binding| binding.target.as_str().to_owned())
                    }
                })
                .unwrap_or(slug);
            world.entity_mut(part).insert((
                Reading {
                    owner,
                    slug,
                    source: reading,
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
    line
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
                        owner,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_keep_editor_focus_and_accept_the_arrow_selected_option() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let owner = spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle::default(),
        );
        Command::New.apply(app.world_mut(), owner);
        let field = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .find(|(_, input)| input.owner == owner && input.index == 1)
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, bevy::input_focus::FocusCause::Pressed);
        inputs(app.world_mut());
        let input = app.world().get::<Input>(field).unwrap();
        assert!(input.options.len() > 1);
        assert_eq!(input.selected, 0);
        let popup = input.suggestions;
        assert_eq!(
            app.world().get::<Node>(popup).unwrap().border,
            UiRect::all(px(1))
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        keys(app.world_mut());
        assert_eq!(app.world().get::<Input>(field).unwrap().selected, 1);
        let selected = app.world().get::<Input>(field).unwrap().options[1].0;
        assert_eq!(
            app.world()
                .get::<crate::token_style::BackgroundToken>(selected)
                .unwrap()
                .0,
            crate::tokens::Token::Accent
        );
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        let expected = match &app.world().get::<Input>(field).unwrap().options[1].1 {
            Command::Insert(_, _, value, _) => value.clone(),
            _ => panic!("Expected a threshold suggestion"),
        };
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        keys(app.world_mut());
        assert_eq!(
            app.world()
                .get::<KarmaCastle>(owner)
                .unwrap()
                .draft
                .as_ref()
                .unwrap()
                .fields[1]
                .text,
            expected
        );
        let field = app
            .world_mut()
            .query::<(Entity, &Input)>()
            .iter(app.world())
            .find(|(_, input)| input.owner == owner && input.index == 1)
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, bevy::input_focus::FocusCause::Pressed);
        inputs(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        keys(app.world_mut());
        let input = app.world().get::<Input>(field).unwrap();
        assert!(input.options.is_empty());
        assert_eq!(
            app.world().get::<Node>(input.suggestions).unwrap().display,
            Display::None
        );
    }
}
