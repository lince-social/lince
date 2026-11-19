use super::*;
use crate::native::{
    actions::ActionButton,
    icons::{Icon, Tooltip},
};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    input_focus::InputFocus,
};
use bevy::text::EditableText;
use model::{FieldDraft, SharedField, Suggestion};
use nucleus::karma::rule_field::RuleFieldKind;

mod readings;
mod list;
mod panels;
pub(super) mod choices;

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
    choices: choices::State,
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
    let _hover = nucleus::performance::Span::new("karma-row-controls");
    world
        .run_system_cached(update_row_actions)
        .expect("update Karma row actions");
}

fn update_row_actions(
    world: &mut World,
    actions: &mut QueryState<(Entity, &'static RowAction)>,
    mut buttons: Local<Vec<(Entity, Entity, Entity)>>,
    mut hovered: Local<HashSet<Entity>>,
) {
    buttons.extend(
        actions
            .iter(world)
            .map(|(entity, action)| (entity, action.row, action.owner)),
    );
    nucleus::performance::add("karma-row-controls-visited", buttons.len() as u64);
    if buttons.is_empty() {
        *buttons = Vec::new();
        *hovered = HashSet::new();
        return;
    }
    hovered.clear();
    let focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let hits = world
        .get_resource::<bevy::picking::hover::HoverMap>()
        .into_iter()
        .flat_map(|map| map.values().flat_map(|hits| hits.keys().copied()))
        .chain(focus);
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
    for (entity, row, owner) in buttons.drain(..) {
        let visible = hovered.contains(&row)
            && world
                .get::<View>(owner)
                .is_some_and(|view| view.pending.is_none());
        let visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if world.get::<Visibility>(entity) != Some(&visibility) {
            world.entity_mut(entity).insert(visibility);
        }
        let disabled = world.get::<bevy::ui::InteractionDisabled>(entity).is_some();
        if visible && disabled {
            world
                .entity_mut(entity)
                .remove::<bevy::ui::InteractionDisabled>();
        } else if !visible && !disabled {
            world
                .entity_mut(entity)
                .insert(bevy::ui::InteractionDisabled);
        }
    }
    hovered.clear();
}

fn reset_columns(world: &mut World, owner: Entity) {
    world
        .run_system_cached_with(reset_column_tracks, owner)
        .expect("reset Karma columns");
}

fn reset_column_tracks(
    In(owner): In<Entity>,
    rows: Query<(Entity, &TableRow)>,
    mut nodes: Query<&mut Node, Allow<Disabled>>,
) {
    let tracks: [RepeatedGridTrack; 6] = std::array::from_fn(|index| {
        if index == 0 {
            GridTrack::px(29.0)
        } else {
            GridTrack::max_content()
        }
    });
    for (entity, row) in &rows {
        if row.0 != owner {
            continue;
        }
        let mut node = nodes.get_mut(entity).unwrap();
        if node.grid_template_columns.as_slice() != tracks.as_slice() {
            node.grid_template_columns = tracks.to_vec();
            nucleus::performance::add("karma-column-resets", 1);
        }
    }
}

pub(super) fn fit_columns(world: &mut World) {
    let _fit = nucleus::performance::Span::new("karma-column-fitting");
    world
        .run_system_cached(fit_column_tracks)
        .expect("fit Karma columns");
}

fn fit_column_tracks(
    world: &mut World,
    candidates: &mut QueryState<(Entity, &'static TableRow, &'static Children)>,
    mut widths: Local<HashMap<Entity, [f32; 6]>>,
    mut rows: Local<Vec<(Entity, Entity)>>,
) {
    widths.clear();
    rows.extend(candidates.iter(world).map(|(entity, row, children)| {
        let columns = widths
            .entry(row.0)
            .or_insert([29.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        for (index, child) in children.iter().take(6).enumerate().skip(1) {
            if let Some(node) = world.get::<ComputedNode>(child) {
                columns[index] = columns[index].max(node.size().x * node.inverse_scale_factor());
            }
        }
        (entity, row.0)
    }));
    let count = rows.len();
    nucleus::performance::add("karma-column-rows", count as u64);
    for (entity, owner) in rows.drain(..) {
        if let Some(widths) = widths.get(&owner)
            && widths.iter().all(|width| *width > 0.0)
        {
            let tracks: [RepeatedGridTrack; 6] = widths.map(GridTrack::px);
            if world
                .get::<Node>(entity)
                .unwrap()
                .grid_template_columns
                .as_slice()
                != tracks.as_slice()
            {
                world.get_mut::<Node>(entity).unwrap().grid_template_columns = tracks.to_vec();
            }
        }
    }
    widths.clear();
    if count == 0 {
        *rows = Vec::new();
        *widths = HashMap::new();
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
    hint: Option<readings::Hint<'static>>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Create,
    Simulate,
    Schedules,
    Commands,
    Ailuros,
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
                .get::<crate::native::workspace::Workspaces>(owner)
                .map_or(1, |spaces| spaces.active);
            let position = world
                .get::<crate::native::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(world, owner, workspace, position, KarmaCastle::default());
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
            Self::Ailuros => {
                super::ailuros_ui::toggle(world, owner);
                return;
            }
            Self::Select(uid) => {
                let mut view = world.get_mut::<View>(owner).unwrap();
                if !view.selected.remove(uid) {
                    view.selected.insert(uid.clone());
                }
            }
            Self::SelectAll => {
                let ids: Vec<_> = visible_rules(world, owner)
                    .into_iter()
                    .map(|rule| rule.uid.clone())
                    .collect();
                let mut view = world.get_mut::<View>(owner).unwrap();
                let all = ids.iter().all(|uid| view.selected.contains(uid));
                for uid in ids {
                    if all {
                        view.selected.remove(&uid);
                    } else {
                        view.selected.insert(uid);
                    }
                }
            }
            Self::DeleteSelected => {
                let selected = &world.get::<View>(owner).unwrap().selected;
                let deleting = visible_rules(world, owner)
                    .into_iter()
                    .filter(|rule| selected.contains(&rule.uid))
                    .map(|rule| rule.uid.clone())
                    .collect();
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.pausing.clear();
                view.deleting = deleting;
            }
            Self::PauseSelected => {
                let selected = &world.get::<View>(owner).unwrap().selected;
                let pausing = visible_rules(world, owner)
                    .into_iter()
                    .filter(|rule| selected.contains(&rule.uid) && rule.state != "paused")
                    .map(|rule| (rule.uid.clone(), rule.revision, true))
                    .collect();
                let mut view = world.get_mut::<View>(owner).unwrap();
                view.deleting.clear();
                view.pausing = pausing;
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
                else {
                    return;
                };
                let new_draft = (!world
                    .get::<KarmaCastle>(owner)
                    .unwrap()
                    .edits
                    .iter()
                    .any(|draft| draft.rule.as_ref() == Some(uid)))
                .then(|| Draft::from_rule(rule));
                if let Some(draft) = new_draft {
                    world
                        .get_mut::<KarmaCastle>(owner)
                        .unwrap()
                        .edits
                        .push(draft);
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
            crate::native::token_style::border(crate::native::tokens::Token::TableGrid),
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
        .insert(crate::native::token_style::border(
            crate::native::tokens::Token::TableGrid,
        ));
    entity
}

fn glyph(world: &mut World, parent: Entity, icon: Icon, size: f32) {
    world.spawn((
        crate::native::icons::image(world, icon).unwrap_or_default(),
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
            crate::native::sand::button(0),
            crate::native::sand::Borderless,
            ActionButton::new(owner, crate::native::actions![command]),
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
        crate::native::icons::InlineTooltip,
        crate::native::icons::TooltipIcon { source: entity },
    ));
    glyph(world, entity, icon, 22.0);
    if !enabled {
        world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
        let glyph = world.get::<Children>(entity).unwrap()[0];
        world
            .entity_mut(glyph)
            .insert(crate::native::token_style::TextToken(
                crate::native::tokens::Token::TableGrid,
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
            crate::native::sand::button(0),
            crate::native::sand::Borderless,
            ActionButton::new(owner, crate::native::actions![command]),
            Node {
                min_width: px(0),
                max_width: percent(100),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    button_label(world, entity, text);
    entity
}

fn button_label(world: &mut World, entity: Entity, text: &str) -> Entity {
    let label = crate::native::edit_mode::label(world, entity, text, 16.0);
    world.entity_mut(label).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    let mut node = world.get_mut::<Node>(label).unwrap();
    node.min_width = Val::Auto;
    node.flex_shrink = 1.0;
    label
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
        .spawn(crate::native::sand::text_editor(
            &value,
            world.resource::<crate::native::theme::Typography>(),
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
        let label = crate::native::edit_mode::label(world, line, title, 14.0);
        world.entity_mut(label).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
        let mut node = world.get_mut::<Node>(label).unwrap();
        node.min_width = Val::Auto;
        node.flex_shrink = 1.0;
        if index > 1 {
            let explanation = [
                "The condition calculates a quantity from numbers, record quantities and frequencies. For example, @balance * freq(@weekly) uses the balance when the weekly frequency fires. startup(@local-cell) is 1 when this Cell starts after boot and 0 otherwise.",
                "The threshold decides whether the calculated quantity should trigger the consequence. Use >0 for positive values, >=10 for at least ten, !=0 for any nonzero value, or always.",
                "The consequence chooses the record to change. @target sets its quantity to the calculated value; @target: command(\"…\") runs a command on that record.",
            ][index - 2];
            let info = world
                .spawn((
                    crate::native::sand::button(0),
                    crate::native::sand::Borderless,
                    crate::native::icons::InlineTooltip,
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
                .insert(crate::native::icons::TooltipIcon { source: info });
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

fn update_selection(world: &mut World, owner: Entity, total: usize, count: usize) {
    let toggled = if total > 0 && total == count {
        accesskit::Toggled::True
    } else if count > 0 {
        accesskit::Toggled::Mixed
    } else {
        accesskit::Toggled::False
    };
    let headers: Vec<_> = world
        .query::<(Entity, &SelectionHeader)>()
        .iter(world)
        .filter(|(_, header)| header.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    for entity in headers {
        update_checkbox(world, entity, toggled);
    }
}

fn update_checkbox(world: &mut World, entity: Entity, toggled: accesskit::Toggled) {
    let glyph = world.get::<Children>(entity).unwrap()[0];
    let icon = match toggled {
        accesskit::Toggled::True => Icon::Check,
        accesskit::Toggled::Mixed => Icon::Minus,
        accesskit::Toggled::False => Icon::Square,
    };
    let image = crate::native::icons::image(world, icon).unwrap_or_default();
    if world.get::<ImageNode>(glyph).is_none_or(|current| {
        current.image != image.image || current.texture_atlas != image.texture_atlas
    }) {
        world.entity_mut(glyph).insert(image);
    }
    let mut node = world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap();
    if node.toggled() != Some(toggled) {
        node.set_toggled(toggled);
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
        (Command::Ailuros, "Ailuros"),
        (Command::Simulate, "Simulation"),
    ] {
        button(world, tools, owner, command, label);
    }
}

fn visible_rules(world: &World, owner: Entity) -> Vec<&Rule> {
    let query = world
        .get::<KarmaCastle>(owner)
        .unwrap()
        .search
        .trim()
        .to_lowercase();
    matching_rules(&world.get::<View>(owner).unwrap().rules, &query)
}

fn matching_rules<'a>(rules: &'a [Rule], query: &str) -> Vec<&'a Rule> {
    rules
        .iter()
        .filter(|rule| matches_rule(rule, query))
        .collect()
}

fn matches_rule(rule: &Rule, query: &str) -> bool {
    query.is_empty()
        || format!("{} {}", rule.name, rule.slug)
            .to_lowercase()
            .contains(query)
        || rule
            .fields
            .iter()
            .any(|field| field.source.to_lowercase().contains(query))
}

pub(super) fn render_controls(world: &mut World, owner: Entity) {
    panels::controls(world, owner);
}

pub(super) fn render_list(world: &mut World, owner: Entity) {
    list::render(world, owner);
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
    panels::form(world, owner);
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
        crate::native::edit_mode::label(
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
        .spawn(crate::native::sand::text_editor(
            value,
            world.resource::<crate::native::theme::Typography>(),
            0,
        ))
        .id();
    let font = world
        .resource::<crate::native::theme::Typography>()
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
        crate::native::sand::Borderless,
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
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
        crate::native::token_style::border(crate::native::tokens::Token::Accent),
    ));
    crate::native::scroll_sand::attach(world, suggestions);
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
        choices: default(),
        selected: 0,
        dismissed: false,
    });
}

pub(super) fn capture(world: &mut World, owner: Entity) {
    let _capture = nucleus::performance::Span::new("karma-field-capture");
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Karma rule fields");
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut castles: Query<&mut KarmaCastle, Allow<Disabled>>,
) {
    for (input, text) in &inputs {
        if input.owner != owner {
            continue;
        }
        let mut castle = castles.get_mut(owner).unwrap();
        let Some(current) = draft(&castle, input.row) else {
            continue;
        };
        if text.value() == field_value(current, input.index) {
            continue;
        }
        set_value(
            draft_mut(&mut castle, input.row).unwrap(),
            input.index,
            &text.value().to_string(),
        );
        nucleus::performance::add("karma-fields-published", 1);
    }
}

fn field_value(draft: &Draft, index: usize) -> &str {
    match index {
        3 => &draft.name,
        4 => &draft.slug,
        _ => &draft.fields[index].text,
    }
}

fn set_value(draft: &mut Draft, index: usize, text: &str) {
    if field_value(draft, index) == text {
        return;
    }
    match index {
        3 => text.clone_into(&mut draft.name),
        4 => text.clone_into(&mut draft.slug),
        _ => {
            let field = &mut draft.fields[index];
            field.linked = None;
            text.clone_into(&mut field.text);
        }
    }
}

pub(super) fn inputs(world: &mut World) {
    world
        .run_system_cached(update_inputs)
        .expect("update Karma inputs");
}

type InputChange = (
    Entity,
    Entity,
    usize,
    usize,
    String,
    bool,
    std::ops::Range<usize>,
);

fn update_inputs(
    world: &mut World,
    search_query: &mut QueryState<(&'static Search, &'static EditableText)>,
    input_query: &mut QueryState<(Entity, &'static Input, &'static EditableText)>,
    mut searches: Local<Vec<(Entity, String)>>,
    mut inputs: Local<Vec<InputChange>>,
) {
    let _prepare = nucleus::performance::Span::new("karma-input-preparation");
    searches.extend(
        search_query
            .iter(world)
            .filter(|(search, text)| {
                world
                    .get::<KarmaCastle>(search.0)
                    .is_some_and(|castle| text.value() != castle.search.as_str())
            })
            .map(|(search, text)| (search.0, text.value().to_string())),
    );
    for (owner, value) in searches.drain(..) {
        capture(world, owner);
        world.get_mut::<KarmaCastle>(owner).unwrap().search = value;
        render_list(world, owner);
    }
    let focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    inputs.extend(input_query.iter(world).filter_map(|(entity, input, text)| {
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
                    text.value().to_string(),
                    focused,
                    selection,
                )
            })
    }));
    for (entity, owner, row, index, text, focused, selection) in inputs.drain(..) {
        let mut input = world.get_mut::<Input>(entity).unwrap();
        if text != input.observed || focused != input.focused {
            input.dismissed = false;
        }
        let dismissed = input.dismissed;
        input.selected = 0;
        if input.observed != text {
            input.observed.clone_from(&text);
        }
        input.dirty = false;
        input.focused = focused;
        input.selection = selection.clone();
        if draft(world.get::<KarmaCastle>(owner).unwrap(), row)
            .is_some_and(|draft| field_value(draft, index) != text)
        {
            let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
            set_value(draft_mut(&mut castle, row).unwrap(), index, &text);
            nucleus::performance::add("karma-fields-published", 1);
        }
        if !focused
            || dismissed
            || index >= 3
            || world.get::<View>(owner).unwrap().pending.is_some()
        {
            choices::draw(world, entity, &[]);
            continue;
        }
        let prefix = text.get(..selection.end.min(text.len())).unwrap_or(&text);
        let rows = choices::prepare(world, owner, row, index, prefix, &selection);
        choices::draw(world, entity, &rows);
        highlight(world, entity);
    }
}

fn highlight(world: &mut World, entity: Entity) {
    let input = world.get::<Input>(entity).unwrap();
    let selected = input.selected;
    let options: Vec<_> = input.options.iter().map(|(entity, _)| *entity).collect();
    for (index, option) in options.into_iter().enumerate() {
        let token = if index == selected {
            crate::native::tokens::Token::Accent
        } else {
            crate::native::tokens::Token::Surface
        };
        if world
            .get::<crate::native::token_style::BackgroundToken>(option)
            .is_none_or(|current| current.0 != token)
        {
            world
                .entity_mut(option)
                .insert(crate::native::token_style::background(token));
        }
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
        choices::draw(world, focused, &[]);
        return;
    }
    input.selected = (input.selected as i32 + step).rem_euclid(input.options.len() as i32) as usize;
    let owner = input.owner;
    if enter {
        let command = input.options[input.selected].1.clone();
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
    world
        .run_system_cached_with(dirty_links, owner)
        .expect("refresh Karma field links");
}

fn dirty_links(In(owner): In<Entity>, mut inputs: Query<&mut Input>) {
    for mut input in &mut inputs {
        if input.owner == owner && !input.dirty {
            input.dirty = true;
        }
    }
}

fn rich_text(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    source: &str,
    rule: &Rule,
    index: usize,
) -> Entity {
    let parts = readings::prepare(owner, source, Some(rule), index);
    let line = row(world, parent);
    let mut node = world.get_mut::<Node>(line).unwrap();
    node.flex_wrap = FlexWrap::Wrap;
    node.column_gap = px(0);
    for readings::Part { text, reading } in parts {
        let part = crate::native::edit_mode::label(world, line, &text, 14.0);
        world.entity_mut(part).insert(TextLayout::linebreak(
            bevy::text::LineBreak::WordOrCharacter,
        ));
        let mut node = world.get_mut::<Node>(part).unwrap();
        node.flex_shrink = 1.0;
        node.min_width = Val::Auto;
        node.max_width = percent(100);
        if let Some(reading) = reading {
            world.entity_mut(part).insert((
                reading,
                Tooltip("Loading reading…".into()),
                crate::native::token_style::text(crate::native::tokens::Token::Accent),
            ));
        }
    }
    line
}

pub(super) fn tick(
    world: &mut World,
    candidates: &mut QueryState<(Entity, &'static Reading)>,
    mut readings: Local<Vec<Entity>>,
    mut wake_at: Local<Option<std::time::Instant>>,
) {
    let _presentation = nucleus::performance::Span::new("karma-reading-presentation");
    let now = chrono::Utc::now().timestamp_millis();
    readings.extend(candidates.iter(world).map(|(entity, _)| entity));
    let count = readings.len();
    nucleus::performance::add("karma-reading-candidates", count as u64);
    let mut countdown = false;
    for entity in readings.drain(..) {
        let reading = world.get::<Reading>(entity).unwrap();
        let owner = reading.owner;
        let Some(view) = world.get::<View>(owner) else {
            continue;
        };
        let complex = reading.source.strip_prefix('@') != Some(reading.slug.as_str())
            && !reading.source.starts_with("freq(");
        let mut hint = readings::Hint::new(view, reading, complex, now);
        countdown |= reading.hovered && (hint.countdown() || complex);
        let mut sent = None;
        if complex
            && reading.hovered
            && !reading.pending
            && reading
                .requested_at
                .is_none_or(|at| now.saturating_sub(at) >= 1000)
        {
            let id = nucleus::new_uid("karma-reading");
            match send(
                world,
                owner,
                ClientMessage::Act {
                    id: id.clone(),
                    action: engine::actions::Action::PreviewKarmaReading {
                        source: reading.source.clone(),
                    },
                },
            ) {
                Ok(()) => sent = Some(id),
                Err(error) => hint.error = Some(error.into()),
            }
        }
        let entity_ref = world.entity(entity);
        let tooltip = entity_ref.get_ref::<Tooltip>().unwrap();
        let changed = reading.hint.as_ref() != Some(&hint);
        let prepared = if changed || tooltip.is_changed() {
            nucleus::performance::add("karma-reading-hints-prepared", 1);
            let text = hint.to_string();
            Some((
                changed.then(|| hint.into_owned()),
                (tooltip.0 != text).then_some(text),
            ))
        } else {
            None
        };
        if let Some(id) = sent {
            world.resource_mut::<Requests>().readings.insert(id, entity);
            let mut reading = world.get_mut::<Reading>(entity).unwrap();
            reading.requested_at = Some(now);
            reading.pending = true;
        }
        if let Some((hint, text)) = prepared {
            if let Some(hint) = hint {
                world.get_mut::<Reading>(entity).unwrap().hint = Some(hint);
            }
            if let Some(text) = text {
                world.get_mut::<Tooltip>(entity).unwrap().0 = text;
                nucleus::performance::add("karma-reading-hints-published", 1);
            }
        }
    }
    if count == 0 {
        *readings = Vec::new();
    }
    if countdown && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
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

pub(super) fn hover_on(
    event: On<crate::native::effect::SandHoveredOn>,
    mut readings: Query<&mut Reading>,
) {
    if let Ok(mut reading) = readings.get_mut(event.entity) {
        reading.hovered = true;
    }
}

pub(super) fn hover_off(
    event: On<crate::native::effect::SandHoveredOff>,
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
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::native::workspace::Workspaces::default())
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
                .get::<crate::native::token_style::BackgroundToken>(selected)
                .unwrap()
                .0,
            crate::native::tokens::Token::Accent
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
