use super::{
    Settings,
    configuration::{self, ApplicationOpening, LayoutSettings},
};
use crate::native::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::label,
};
use bevy::{
    input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup},
    prelude::*,
    text::EditableText,
    ui::InteractionDisabled,
};

#[derive(Resource, Default)]
pub(super) struct WidthEditor(Option<Modal>);

struct Modal {
    overlay: Entity,
    previous: Option<Entity>,
    origin: LayoutSettings,
    #[cfg(test)]
    list: Entity,
    width_list: Entity,
    height_list: Entity,
    entries: Vec<Entry>,
    heights: Vec<Entry>,
    default: Entity,
    move_step: Entity,
    resize_step: Entity,
    gap: Entity,
    opening_button: Entity,
    opening: ApplicationOpening,
    workspace_button: Entity,
    workspace_back_and_forth: bool,
    status: Entity,
    save: Entity,
    add: Entity,
    add_height: Entity,
    observed: Vec<(String, bool, bool)>,
    message: String,
}

struct Entry {
    row: Entity,
    input: Entity,
    number: Entity,
    up: Entity,
    down: Entity,
    remove: Entity,
}

#[derive(Component)]
struct Summary(Entity);

#[derive(Clone, Copy)]
enum Preset {
    Width,
    RowHeight,
}

impl Preset {
    fn label(self) -> &'static str {
        match self {
            Self::Width => "Width",
            Self::RowHeight => "Row height",
        }
    }
}

#[derive(Clone, Copy)]
enum Change {
    Open,
    Add(Preset),
    Remove(Entity),
    Move(Entity, bool),
    Reset(Preset),
    Opening,
    WorkspaceBackAndForth,
    Save,
    Cancel,
    Reload,
    Rebase,
}

#[cfg(test)]
fn active(world: &World) -> bool {
    world
        .get_resource::<WidthEditor>()
        .is_some_and(|editor| editor.0.is_some())
}

pub(crate) fn ratio(value: &str) -> Result<[u16; 2], String> {
    fn integer(value: &str) -> Option<u64> {
        (!value.is_empty() && value.len() <= 8 && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse().ok())
            .flatten()
    }
    let invalid = || {
        "Use a positive fraction such as 1/3 or a percentage such as 50%, no greater than 100%."
            .to_string()
    };
    let value = value.trim();
    if value.len() > 32 {
        return Err(invalid());
    }
    let percent = value.ends_with('%');
    let value = value
        .strip_suffix('%')
        .unwrap_or(value)
        .trim()
        .replace(',', ".");
    let (mut numerator, mut denominator) =
        if let Some((numerator, denominator)) = value.split_once('/') {
            (
                integer(numerator.trim()).ok_or_else(invalid)?,
                integer(denominator.trim()).ok_or_else(invalid)?,
            )
        } else if let Some((whole, fraction)) = value.split_once('.') {
            if fraction.is_empty() || fraction.len() > 6 {
                return Err(invalid());
            }
            let denominator = 10_u64.pow(fraction.len() as u32);
            let whole = if whole.is_empty() {
                0
            } else {
                integer(whole).ok_or_else(invalid)?
            };
            (
                whole * denominator + integer(fraction).ok_or_else(invalid)?,
                denominator,
            )
        } else {
            (integer(&value).ok_or_else(invalid)?, 1)
        };
    if percent {
        denominator *= 100;
    }
    if numerator == 0 || denominator == 0 || numerator > denominator {
        return Err(invalid());
    }
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    numerator /= a;
    denominator /= a;
    Ok([
        u16::try_from(numerator).map_err(|_| invalid())?,
        u16::try_from(denominator).map_err(|_| {
            "This percentage is too precise; use a fraction such as 1/3.".to_string()
        })?,
    ])
}

fn text(ratio: [u16; 2]) -> String {
    let numerator = u32::from(ratio[0]) * 100;
    let denominator = u32::from(ratio[1]);
    if numerator % denominator == 0 {
        format!("{}%", numerator / denominator)
    } else {
        format!("{}/{}", ratio[0], ratio[1])
    }
}

fn opening_text(opening: ApplicationOpening) -> &'static str {
    match opening {
        ApplicationOpening::Normal => "Normal",
        ApplicationOpening::Maximized => "Maximized",
    }
}

fn workspace_text(enabled: bool) -> String {
    format!("Back-and-forth workspace switching: {}", if enabled { "On" } else { "Off" })
}

fn column(world: &mut World, parent: Entity) -> Entity {
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

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                column_gap: px(6),
                row_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn button(
    world: &mut World,
    parent: Entity,
    target: Entity,
    action: Change,
    caption: &str,
) -> Entity {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            super::KeyboardModalTrigger,
            Node {
                padding: UiRect::axes(px(9), px(6)),
                flex_shrink: 0.0,
                min_width: px(0),
                max_width: percent(100),
                ..default()
            },
            ActionButton::new(target, crate::native::actions![action]),
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(caption);
    let label = label(world, entity, caption, 14.0);
    world.entity_mut(label).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    entity
}

fn field(world: &mut World, parent: Entity, value: &str, caption: &str) -> Entity {
    let input = crate::native::sand::single_line_editor(
        value,
        world.resource::<crate::native::theme::Typography>(),
        0,
        24,
    );
    let entity = world
        .spawn((
            input,
            ChildOf(parent),
            bevy::a11y::AccessibilityNode::from(accesskit::Node::new(accesskit::Role::TextInput)),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(caption);
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .max_characters = Some(24);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.width = px(132);
    node.max_width = percent(100);
    node.flex_shrink = 0.0;
    entity
}

fn entry(world: &mut World, parent: Entity, value: &str, kind: Preset) -> Entry {
    let item = column(world, parent);
    let caption = format!("{} preset", kind.label());
    let number = label(world, item, &caption, 14.0);
    let controls = row(world, item);
    let input = field(world, controls, value, &caption);
    let up = button(world, controls, item, Change::Move(item, false), "Up");
    let down = button(world, controls, item, Change::Move(item, true), "Down");
    let remove = button(world, controls, item, Change::Remove(item), "Remove");
    Entry {
        row: item,
        input,
        number,
        up,
        down,
        remove,
    }
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    label(world, parent, "Window layout", 20.0);
    let summary = label(world, parent, "", 14.0);
    world.entity_mut(summary).insert(Summary(root));
    button(world, parent, root, Change::Open, "Edit window layout");
    refresh(world);
}

fn focus(world: &mut World, entity: Option<Entity>) {
    if let Some(entity) = entity.filter(|entity| world.get_entity(*entity).is_ok()) {
        world
            .resource_mut::<InputFocus>()
            .set(entity, FocusCause::Navigated);
    } else {
        world.resource_mut::<InputFocus>().clear();
    }
}

fn open(world: &mut World, root: Entity) {
    if super::keyboard_owned(world) {
        return;
    }
    world.init_resource::<WidthEditor>();
    let previous = world.resource::<InputFocus>().get();
    let origin = world
        .get_resource::<LayoutSettings>()
        .cloned()
        .unwrap_or_default();
    let overlay = world
        .spawn((
            crate::native::inspection::InspectionExcluded,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(100),
            TabGroup::modal(),
            ChildOf(root),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::native::actions![Change::Cancel],
            )]),
        ))
        .id();
    let panel = column(world, overlay);
    world.entity_mut(panel).insert((
        Node {
            width: px(640),
            max_width: percent(96),
            height: percent(90),
            padding: UiRect::all(px(14)),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    label(world, panel, "Window layout", 22.0);
    label(
        world,
        panel,
        "Cycle in this order. Use 50% or 1/2; thirds stay exact as 1/3 and 2/3. Changes apply after Save.",
        14.0,
    );
    let list = column(world, panel);
    world.entity_mut(list).insert((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            flex_grow: 1.0,
            min_height: px(0),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        crate::native::edit_mode::FocusScroll,
    ));
    world
        .entity_mut(list)
        .observe(|mut event: On<Pointer<Scroll>>| event.propagate(false));
    label(world, list, "Workspace navigation", 16.0);
    let workspace_back_and_forth = origin.workspace_back_and_forth;
    let workspace_button = button(world, list, overlay, Change::WorkspaceBackAndForth, &workspace_text(workspace_back_and_forth));
    {
        let mut node = world.get_mut::<Node>(workspace_button).unwrap();
        node.width = percent(100);
        node.max_width = percent(100);
        node.min_width = px(0);
    }
    let caption = world.get::<Children>(workspace_button).unwrap()[0];
    world.entity_mut(caption).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    {
        let mut node = world.get_mut::<Node>(caption).unwrap();
        node.flex_shrink = 1.0;
    }
    label(world, list, "When enabled, selecting the current numbered workspace returns to the previous workspace selected in this view. Background activation stays unchanged. History is kept for this session only.", 14.0);
    label(world, list, "Horizontal width presets", 16.0);
    let width_list = column(world, list);
    let entries: Vec<_> = origin
        .width_presets
        .iter()
        .map(|ratio| entry(world, width_list, &text(*ratio), Preset::Width))
        .collect();
    let options = row(world, list);
    let add = button(
        world,
        options,
        overlay,
        Change::Add(Preset::Width),
        "Add preset",
    );
    button(
        world,
        options,
        overlay,
        Change::Reset(Preset::Width),
        "Reset widths",
    );
    label(world, list, "Tiled row height presets", 16.0);
    label(
        world,
        list,
        "A row shares its height. Saving this list leaves current windows unchanged; the row-height command applies the next preset.",
        14.0,
    );
    let height_list = column(world, list);
    let heights = origin
        .height_presets
        .iter()
        .map(|ratio| entry(world, height_list, &text(*ratio), Preset::RowHeight))
        .collect();
    let options = row(world, list);
    let add_height = button(
        world,
        options,
        overlay,
        Change::Add(Preset::RowHeight),
        "Add row height preset",
    );
    button(
        world,
        options,
        overlay,
        Change::Reset(Preset::RowHeight),
        "Reset row heights",
    );
    label(world, list, "Default opening width", 16.0);
    let default = field(
        world,
        list,
        &text(origin.default_width),
        "Default opening width",
    );
    label(world, list, "Free / floating movement step", 16.0);
    let move_step = field(
        world,
        list,
        &origin.move_step.to_string(),
        "Free movement step",
    );
    label(world, list, "Resize step", 16.0);
    let resize_step = field(
        world,
        list,
        &origin.resize_step.to_string(),
        "Resize step (logical pixels)",
    );
    label(
        world,
        list,
        "Grow/shrink commands resize one width or a tiled row's height. Free and floating items resize around their center; the step is measured at the current view scale.",
        14.0,
    );
    label(world, list, "Window gaps", 16.0);
    let gap = field(
        world,
        list,
        &origin.gaps.to_string(),
        "Window gaps (0 to 256)",
    );
    world.get_mut::<EditableText>(gap).unwrap().max_characters = Some(3);
    let opening = origin.application_opening;
    let opening_button = button(
        world,
        list,
        overlay,
        Change::Opening,
        &format!("New application windows: {}", opening_text(opening)),
    );
    label(
        world,
        list,
        "Zero removes gaps. Opening state applies to new application windows in Park; authored Sands, dialogs and restored placements keep their own state. Maximized windows restore to their normal width.",
        14.0,
    );
    let status = crate::native::layout::desktop::input_settings::feedback(world, panel);
    let footer = row(world, panel);
    let save = button(world, footer, overlay, Change::Save, "Save window layout");
    button(world, footer, overlay, Change::Reload, "Reload active layout values");
    button(world, footer, overlay, Change::Rebase, "Review draft against file");
    button(
        world,
        footer,
        overlay,
        Change::Cancel,
        "Cancel layout draft",
    );
    let first = entries.first().map(|entry| entry.input);
    world.resource_mut::<WidthEditor>().0 = Some(Modal {
        overlay,
        previous,
        origin,
        #[cfg(test)]
        list,
        width_list,
        height_list,
        entries,
        heights,
        default,
        move_step,
        resize_step,
        gap,
        opening_button,
        opening,
        workspace_button,
        workspace_back_and_forth,
        status,
        save,
        add,
        add_height,
        observed: Vec::new(),
        message: String::new(),
    });
    super::own_keyboard(world, overlay);
    focus(world, first);
    refresh(world);
}

fn candidate(world: &World, modal: &Modal) -> Result<LayoutSettings, String> {
    let value = |entity| -> Result<String, String> {
        let text = world
            .get::<EditableText>(entity)
            .ok_or("A layout field is unavailable.")?;
        if text.is_composing() || !text.pending_edits.is_empty() || text.pending_paste.is_some() {
            return Err("Finish typing before saving the window layout.".into());
        }
        Ok(text.value().to_string())
    };
    let mut layout = modal.origin.clone();
    layout.width_presets = modal
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            ratio(&value(entry.input)?).map_err(|error| format!("Preset {}: {error}", index + 1))
        })
        .collect::<Result<_, _>>()?;
    layout.height_presets = modal
        .heights
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            ratio(&value(entry.input)?)
                .map_err(|error| format!("Row height preset {}: {error}", index + 1))
        })
        .collect::<Result<_, _>>()?;
    layout.default_width =
        ratio(&value(modal.default)?).map_err(|error| format!("Default width: {error}"))?;
    layout.move_step = value(modal.move_step)?
        .trim()
        .parse()
        .map_err(|_| "Use a whole movement step from 1 to 10000 canvas units.".to_owned())?;
    layout.resize_step = value(modal.resize_step)?
        .trim()
        .parse()
        .map_err(|_| "Use a whole resize step from 1 to 10000 logical pixels.".to_owned())?;
    layout.gaps = value(modal.gap)?
        .trim()
        .parse()
        .map_err(|_| "Use a whole gap from 0 to 256.".to_owned())?;
    layout.application_opening = modal.opening;
    layout.workspace_back_and_forth = modal.workspace_back_and_forth;
    layout.validate()?;
    Ok(layout)
}

fn disabled(world: &mut World, entity: Entity, disabled: bool) {
    if world.get::<InteractionDisabled>(entity).is_some() == disabled {
        return;
    }
    if disabled {
        world.entity_mut(entity).insert(InteractionDisabled);
    } else {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    }
}

fn observed(world: &World, modal: &Modal) -> Vec<(String, bool, bool)> {
    let mut values: Vec<_> = modal.entries.iter().chain(&modal.heights).map(|entry| entry.input)
        .chain([modal.default, modal.move_step, modal.resize_step, modal.gap])
        .map(|entity| world.get::<EditableText>(entity).map(|text| (text.value().to_string(), text.is_composing(), !text.pending_edits.is_empty() || text.pending_paste.is_some())).unwrap_or_default()).collect();
    values.push((opening_text(modal.opening).into(), false, false));
    values.push((modal.workspace_back_and_forth.to_string(), false, false));
    values
}

pub(super) fn refresh(world: &mut World) {
    let layout = world
        .get_resource::<LayoutSettings>()
        .cloned()
        .unwrap_or_default();
    let summaries: Vec<_> = world
        .query::<(Entity, &Summary)>()
        .iter(world)
        .map(|(entity, summary)| (entity, summary.0))
        .collect();
    let value = format!(
        "Width cycle: {} · Row height cycle: {} · Resize step: {} px · Default width: {} · Gaps: {} · New applications: {} · Back-and-forth: {}",
        layout
            .width_presets
            .iter()
            .map(|ratio| text(*ratio))
            .collect::<Vec<_>>()
            .join(" → "),
        layout
            .height_presets
            .iter()
            .map(|ratio| text(*ratio))
            .collect::<Vec<_>>()
            .join(" → "),
        layout.resize_step,
        text(layout.default_width),
        layout.gaps,
        opening_text(layout.application_opening),
        if layout.workspace_back_and_forth { "On" } else { "Off" }
    );
    for (entity, root) in summaries {
        if world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|mode| !mode.enabled)
        {
            continue;
        }
        let focused = world
            .get::<crate::native::workspace::Workspaces>(root)
            .and_then(|spaces| {
                spaces
                    .entries
                    .iter()
                    .find(|space| space.id == spaces.active)
            })
            .and_then(|space| {
                let id = space.desktop.focused()?;
                let item = space.desktop.items().iter().find(|item| item.id == id)?;
                let ratio = if space.desktop.mode() == crate::host::WorkspaceLayout::Tiled
                    && !item.floating
                {
                    space
                        .desktop
                        .item_width(id)
                        .and_then(crate::layout::desktop::Width::ratio)
                } else {
                    item.free_width.map(|width| width.ratio)
                };
                Some((id, ratio))
            });
        let suffix = focused
            .and_then(|(id, intent)| {
                world
                    .query::<(
                        &crate::native::layout::desktop::Managed,
                        &crate::native::canvas::CanvasItem,
                        &ChildOf,
                    )>()
                    .iter(world)
                    .find(|(managed, _, parent)| managed.id == id && parent.parent() == root)
                    .map(|(_, item, _)| {
                        format!(
                            "\nFocused width: {:.0} logical px · {}",
                            item.size.x,
                            intent
                                .map(|ratio| format!("preset {}", text(ratio)))
                                .unwrap_or_else(|| "manual size".into())
                        )
                    })
            })
            .unwrap_or_default();
        let value = format!("{value}{suffix}");
        if let Some(mut summary) = world.get_mut::<Text>(entity)
            && summary.0 != value
        {
            summary.0 = value.clone();
        }
    }
    let Some(mut modal) = world
        .get_resource_mut::<WidthEditor>()
        .and_then(|mut editor| editor.0.take())
    else {
        return;
    };
    if world.get_entity(modal.overlay).is_err() {
        super::release_keyboard(world, modal.overlay);
        focus(world, modal.previous);
        return;
    }
    let observed = observed(world, &modal);
    if observed != modal.observed {
        let values_changed = observed.len() != modal.observed.len() || observed.iter().zip(&modal.observed).any(|(new, old)| new.0 != old.0);
        modal.observed = observed;
        if values_changed { modal.message.clear(); }
    }
    let result = candidate(world, &modal);
    let stale = modal.origin != layout;
    let message = result
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| if stale { "Active layout settings changed. Reload active layout values or Review the draft against the file.".into() } else { modal.message.clone() });
    if let Some(mut status) = world.get_mut::<Text>(modal.status)
        && status.0 != message
    {
        status.0 = message;
    }
    disabled(world, modal.save, result.is_err() || stale);
    disabled(world, modal.add, modal.entries.len() == 32);
    disabled(world, modal.add_height, modal.heights.len() == 32);
    for (kind, entries) in [
        (Preset::Width, &modal.entries),
        (Preset::RowHeight, &modal.heights),
    ] {
        for (index, entry) in entries.iter().enumerate() {
            let value = format!("{} preset {}", kind.label(), index + 1);
            if let Some(mut number) = world.get_mut::<Text>(entry.number)
                && number.0 != value
            {
                number.0 = value.clone();
            }
            accessibility_label(world, entry.input, &value);
            for (entity, action) in [
                (entry.up, "Move up"),
                (entry.down, "Move down"),
                (entry.remove, "Remove"),
            ] {
                accessibility_label(
                    world,
                    entity,
                    &format!(
                        "{action} {} preset {}",
                        kind.label().to_lowercase(),
                        index + 1
                    ),
                );
            }
            disabled(world, entry.up, index == 0);
            disabled(world, entry.down, index + 1 == entries.len());
            disabled(world, entry.remove, entries.len() == 1);
        }
    }
    world.resource_mut::<WidthEditor>().0 = Some(modal);
}

fn accessibility_label(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity)
        && node.label() != Some(value)
    {
        node.set_label(value);
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if matches!(self, Self::Open) {
            open(world, target);
            return;
        }
        if let Some(owner) = world.get_resource::<super::KeyboardModalOwner>().and_then(|owner| owner.0)
            && world.get_resource::<WidthEditor>().and_then(|editor| editor.0.as_ref()).is_some_and(|modal| modal.overlay != owner) { return; }
        let Some(mut modal) = world
            .get_resource_mut::<WidthEditor>()
            .and_then(|mut editor| editor.0.take())
        else {
            return;
        };
        let mut close = false;
        let mut changed = false;
        let mut rebase = false;
        match *self {
            Self::WorkspaceBackAndForth => {
                modal.workspace_back_and_forth = !modal.workspace_back_and_forth;
                let caption = workspace_text(modal.workspace_back_and_forth);
                let label = world.get::<Children>(modal.workspace_button).unwrap()[0];
                if let Some(mut text) = world.get_mut::<Text>(label) {
                    text.0 = caption.clone();
                }
                accessibility_label(world, modal.workspace_button, &caption);
            }
            Self::Opening => {
                modal.opening = match modal.opening {
                    ApplicationOpening::Normal => ApplicationOpening::Maximized,
                    ApplicationOpening::Maximized => ApplicationOpening::Normal,
                };
                let caption = world.get::<Children>(modal.opening_button).unwrap()[0];
                if let Some(mut text) = world.get_mut::<Text>(caption) {
                    text.0 = format!("New application windows: {}", opening_text(modal.opening));
                }
                accessibility_label(
                    world,
                    modal.opening_button,
                    &format!("New application windows: {}", opening_text(modal.opening)),
                );
            }
            Self::Add(kind)
                if match kind {
                    Preset::Width => modal.entries.len(),
                    Preset::RowHeight => modal.heights.len(),
                } < 32 =>
            {
                let (parent, entries) = match kind {
                    Preset::Width => (modal.width_list, &mut modal.entries),
                    Preset::RowHeight => (modal.height_list, &mut modal.heights),
                };
                let item = entry(world, parent, "", kind);
                focus(world, Some(item.input));
                entries.push(item);
                changed = true;
            }
            Self::Remove(row) => {
                let entries = if modal.entries.iter().any(|entry| entry.row == row) {
                    &mut modal.entries
                } else {
                    &mut modal.heights
                };
                if entries.len() > 1
                    && let Some(index) = entries.iter().position(|entry| entry.row == row)
                {
                    let entry = entries.remove(index);
                    world.despawn(entry.row);
                    focus(
                        world,
                        entries
                            .get(index.min(entries.len() - 1))
                            .map(|entry| entry.input),
                    );
                    changed = true;
                }
            }
            Self::Move(row, down) => {
                let entries = if modal.entries.iter().any(|entry| entry.row == row) {
                    &mut modal.entries
                } else {
                    &mut modal.heights
                };
                if let Some(index) = entries.iter().position(|entry| entry.row == row) {
                    let target = if down {
                        (index + 1 < entries.len()).then_some(index + 1)
                    } else {
                        index.checked_sub(1)
                    };
                    if let Some(target) = target {
                        entries.swap(index, target);
                        changed = true;
                    }
                }
            }
            Self::Reset(kind) => {
                let (parent, entries) = match kind {
                    Preset::Width => (modal.width_list, &mut modal.entries),
                    Preset::RowHeight => (modal.height_list, &mut modal.heights),
                };
                for entry in entries.drain(..) {
                    world.despawn(entry.row);
                }
                let defaults = LayoutSettings::default();
                let presets = match kind {
                    Preset::Width => &defaults.width_presets,
                    Preset::RowHeight => &defaults.height_presets,
                };
                *entries = presets
                    .iter()
                    .map(|ratio| entry(world, parent, &text(*ratio), kind))
                    .collect();
                let first = entries.first().map(|entry| entry.input);
                if matches!(kind, Preset::Width) {
                    world
                        .get_mut::<EditableText>(modal.default)
                        .unwrap()
                        .editor
                        .set_text(&text(defaults.default_width));
                    world
                        .get_mut::<EditableText>(modal.move_step)
                        .unwrap()
                        .editor
                        .set_text(&defaults.move_step.to_string());
                }
                focus(world, first);
                changed = true;
            }
            Self::Reload => {
                let fields = modal.entries.iter().chain(&modal.heights).map(|entry| entry.input).chain([modal.default, modal.move_step, modal.resize_step, modal.gap]);
                let result = configuration::review::settled(world, fields).and_then(|_| {
                    let active = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
                    load_draft(world, &mut modal, active.clone(), active)
                });
                modal.message = result.map(|_| "Active layout values loaded. Unsaved layout changes were discarded.".into()).unwrap_or_else(|error| error);
            }
            Self::Rebase => rebase = true,
            Self::Save => {
                let active = world
                    .get_resource::<LayoutSettings>()
                    .cloned()
                    .unwrap_or_default();
                let result = if active != modal.origin {
                    Err(
                        "Active layout settings changed. Reload active layout values or Review the draft against the file before saving."
                            .into(),
                    )
                } else {
                    candidate(world, &modal).and_then(|layout| {
                        configuration::commit(world, world.resource::<Settings>().clone(), layout)
                    })
                };
                match result {
                    Ok(message) => {
                        crate::native::notifications::report(world, "Window layout", &message);
                        close = true;
                    }
                    Err(error) => modal.message = error,
                }
            }
            Self::Cancel => close = true,
            _ => {}
        }
        if changed {
            for (parent, entries) in [
                (modal.width_list, &modal.entries),
                (modal.height_list, &modal.heights),
            ] {
                let children: Vec<_> = entries.iter().map(|entry| entry.row).collect();
                world.entity_mut(parent).replace_children(&children);
            }
        }
        if close {
            super::release_keyboard(world, modal.overlay);
            world.despawn(modal.overlay);
            focus(world, modal.previous);
        } else {
            let overlay = modal.overlay;
            world.resource_mut::<WidthEditor>().0 = Some(modal);
            if rebase && let Err(error) = configuration::review::open(world, overlay, "window layout", review_snapshot, review_apply) {
                world.resource_mut::<WidthEditor>().0.as_mut().unwrap().message = error;
            }
        }
        refresh(world);
    }
}

fn load_draft(world: &mut World, modal: &mut Modal, active: LayoutSettings, draft: LayoutSettings) -> Result<(), String> {
    draft.validate()?;
    for entity in [modal.default, modal.move_step, modal.resize_step, modal.gap] {
        if world.get::<EditableText>(entity).is_none() { return Err("A layout field is unavailable; the draft is kept.".into()); }
    }
    for entity in [modal.width_list, modal.height_list] {
        if world.get::<Node>(entity).is_none() { return Err("The layout editor is unavailable; the draft is kept.".into()); }
    }
    for entry in modal.entries.drain(..).chain(modal.heights.drain(..)) { world.despawn(entry.row); }
    modal.entries = draft.width_presets.iter().map(|ratio| entry(world, modal.width_list, &text(*ratio), Preset::Width)).collect();
    modal.heights = draft.height_presets.iter().map(|ratio| entry(world, modal.height_list, &text(*ratio), Preset::RowHeight)).collect();
    for (entity, value) in [(modal.default, text(draft.default_width)), (modal.move_step, draft.move_step.to_string()), (modal.resize_step, draft.resize_step.to_string()), (modal.gap, draft.gaps.to_string())] {
        world.get_mut::<EditableText>(entity).ok_or("A layout field is unavailable.")?.editor.set_text(&value);
    }
    modal.opening = draft.application_opening;
    modal.workspace_back_and_forth = draft.workspace_back_and_forth;
    for (button, value) in [(modal.opening_button, format!("New application windows: {}", opening_text(modal.opening))), (modal.workspace_button, workspace_text(modal.workspace_back_and_forth))] {
        if let Some(label) = world.get::<Children>(button).and_then(|children| children.first()).copied()
            && let Some(mut text) = world.get_mut::<Text>(label) { text.0 = value.clone(); }
        accessibility_label(world, button, &value);
    }
    modal.origin = active;
    modal.observed = observed(world, modal);
    focus(world, Some(modal.save));
    Ok(())
}

fn review_snapshot(world: &World, owner: Entity) -> Result<configuration::review::Snapshot, String> {
    let modal = world.get_resource::<WidthEditor>().and_then(|editor| editor.0.as_ref()).filter(|modal| modal.overlay == owner).ok_or("The layout editor is unavailable.")?;
    let active = world.get_resource::<LayoutSettings>().cloned().unwrap_or_default();
    configuration::review::Snapshot::new(&modal.origin, &candidate(world, modal)?, &active)
}

fn review_apply(world: &mut World, owner: Entity, active: serde_json::Value, draft: serde_json::Value) -> Result<(), String> {
    let active: LayoutSettings = serde_json::from_value(active).map_err(|error| error.to_string())?;
    let draft: LayoutSettings = serde_json::from_value(draft).map_err(|error| error.to_string())?;
    draft.validate()?;
    let Some(mut modal) = world.resource_mut::<WidthEditor>().0.take() else { return Err("The layout editor is unavailable.".into()); };
    if modal.overlay != owner { world.resource_mut::<WidthEditor>().0 = Some(modal); return Err("The original layout editor is unavailable.".into()); }
    let result = load_draft(world, &mut modal, active, draft);
    if result.is_ok() { modal.message = "Review applied to the draft. Save window layout to activate it.".into(); }
    world.resource_mut::<WidthEditor>().0 = Some(modal);
    result
}

#[cfg(test)]
mod tests;
