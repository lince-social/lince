use super::*;
use crate::{
    host::display::{DisplayAction, DisplayOutcome, DisplayRequest, DisplaySnapshot},
    i18n::Token as Tr,
    native::{
        actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
        edit_mode::label,
    },
};
use bevy::{
    input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup},
    text::EditableText,
    ui::InteractionDisabled,
};
use std::time::{Duration, Instant};

mod model;
use model::Draft;

#[derive(Resource, Default)]
struct Editor {
    modal: Option<Modal>,
    next_request: u64,
    root: Option<Entity>,
}

struct Modal {
    overlay: Entity,
    previous: Option<Entity>,
    draft: Draft,
    selected: OutputId,
    reference: Option<OutputId>,
    board: Entity,
    rectangles: BTreeMap<OutputId, (Entity, Entity)>,
    fields: [Entity; 3],
    modes: Entity,
    mode_buttons: Vec<Entity>,
    enabled: (Entity, Entity),
    rotation: (Entity, Entity),
    adaptive_sync: (Entity, Entity),
    adaptive_sync_status: Entity,
    color: (Entity, Entity),
    color_status: Entity,
    title: Entity,
    info: Entity,
    status: Entity,
    keep: Entity,
    revert: Entity,
    test: Entity,
    apply: Entity,
    reload: Entity,
    identify: Entity,
    editing: Vec<Entity>,
    reference_label: Entity,
    pending: Option<(u64, bool)>,
    response: Option<crate::host::display::DisplayResponse>,
    message: String,
    diagram: Diagram,
    drag: Option<(OutputId, [i32; 2], Diagram)>,
    countdown: Option<u64>,
}

#[derive(Clone, Copy, Default)]
struct Diagram {
    origin: Vec2,
    scale: f32,
}

#[derive(Component)]
struct Summary;

#[derive(Component)]
struct NeedsReference;

#[derive(Resource, Default)]
struct IdentificationView {
    signature: Option<(OutputId, Instant, Option<Handle<Image>>, Entity, String)>,
    entities: Vec<Entity>,
}

#[derive(Clone, Copy)]
enum Change {
    Open,
    Close,
    Reload,
    Select(OutputId),
    Enable,
    Mode(usize),
    Rotate,
    AdaptiveSync,
    Color,
    Reference,
    Align(u8),
    Test,
    Apply,
    Keep,
    Revert,
    Identify,
    DragStart(OutputId),
    Drag(OutputId, Vec2),
    DragEnd,
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
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: px(6),
                row_gap: px(4),
                flex_shrink: 0.0,
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
) -> (Entity, Entity) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::shortcuts::KeyboardModalTrigger,
            Node {
                padding: UiRect::axes(px(9), px(6)),
                flex_shrink: 0.0,
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
    (entity, label(world, entity, caption, 14.0))
}

fn field(world: &mut World, parent: Entity, caption: &str) -> Entity {
    let owner = column(world, parent);
    world.get_mut::<Node>(owner).unwrap().width = px(100);
    label(world, owner, caption, 14.0);
    let input = crate::native::sand::single_line_editor(
        "",
        world.resource::<crate::native::theme::Typography>(),
        0,
        16,
    );
    let entity = world
        .spawn((
            input,
            ChildOf(owner),
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
        .max_characters = Some(16);
    world.get_mut::<Node>(entity).unwrap().width = percent(100);
    entity
}

fn text(world: &mut World, entity: Entity, value: &str) {
    if let Some(mut text) = world.get_mut::<Text>(entity)
        && text.0 != value
    {
        text.0 = value.into();
    }
    let parent = world
        .get::<ChildOf>(entity)
        .map(ChildOf::parent)
        .filter(|parent| world.get::<ActionButton>(*parent).is_some());
    if let Some(parent) = parent
        && let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(parent)
        && node.label() != Some(value)
    {
        node.set_label(value);
    }
}

fn disable(world: &mut World, entity: Entity, disabled: bool) {
    if world.get::<InteractionDisabled>(entity).is_some() == disabled {
        return;
    }
    if disabled {
        world.entity_mut(entity).insert(InteractionDisabled);
    } else {
        world.entity_mut(entity).remove::<InteractionDisabled>();
    }
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    label(world, parent, Tr::ParkMonitors.text(), 20.0);
    let summary = label(world, parent, "", 14.0);
    world.entity_mut(summary).insert(Summary);
    button(
        world,
        parent,
        root,
        Change::Open,
        Tr::ParkArrangeMonitors.text(),
    );
}

fn read_fields(world: &World, modal: &mut Modal) -> Result<(), String> {
    let values = modal.fields.map(|entity| {
        world
            .get::<EditableText>(entity)
            .map(|text| text.value().to_string().trim().to_string())
            .unwrap_or_default()
    });
    let x: i32 = values[0]
        .parse()
        .map_err(|_| "Use whole logical coordinates for X and Y.".to_owned())?;
    let y: i32 = values[1]
        .parse()
        .map_err(|_| "Use whole logical coordinates for X and Y.".to_owned())?;
    let scale: f64 = values[2]
        .replace(',', ".")
        .parse()
        .map_err(|_| "Use a scale from 0.5 to 4, such as 1.25.".to_owned())?;
    if x.abs_diff(0) > 16384
        || y.abs_diff(0) > 16384
        || !scale.is_finite()
        || !(0.5..=4.0).contains(&scale)
        || (scale * 120.0 - (scale * 120.0).round()).abs() > 0.000001
    {
        return Err(
            "Use coordinates from -16384 to 16384 and scale 0.5–4 in steps of 1/120.".into(),
        );
    }
    if let Some(settings) = modal.draft.settings.get_mut(&modal.selected) {
        settings.position = [x, y];
        settings.scale = scale;
    }
    Ok(())
}

fn write_fields(world: &mut World, modal: &Modal) {
    if let Some(settings) = modal.draft.settings.get(&modal.selected) {
        let values = [
            settings.position[0].to_string(),
            settings.position[1].to_string(),
            settings.scale.to_string(),
        ];
        for (entity, value) in modal.fields.into_iter().zip(values) {
            if let Some(mut text) = world.get_mut::<EditableText>(entity) {
                text.editor.set_text(&value);
            }
        }
    }
}

fn fill_modes(world: &mut World, modal: &mut Modal) {
    for button in modal.mode_buttons.drain(..) {
        if world.get_entity(button).is_ok() {
            world.despawn(button);
        }
    }
    if let Some(head) = modal
        .draft
        .origin
        .heads
        .iter()
        .find(|head| head.id == modal.selected)
    {
        for (index, mode) in head.modes.iter().enumerate() {
            let caption = format!(
                "{} × {} · {:.3} Hz{}",
                mode.size[0],
                mode.size[1],
                f64::from(mode.refresh_mhz) / 1000.0,
                if mode.preferred { " ★" } else { "" }
            );
            modal.mode_buttons.push(
                button(
                    world,
                    modal.modes,
                    modal.overlay,
                    Change::Mode(index),
                    &caption,
                )
                .0,
            );
        }
    }
}

fn send(world: &mut World, action: DisplayAction) -> Result<u64, String> {
    world.init_resource::<Editor>();
    let latest = world
        .get_resource::<DisplaySnapshot>()
        .map(|snapshot| snapshot.last_request)
        .unwrap_or(0);
    let id = world
        .resource::<Editor>()
        .next_request
        .max(latest)
        .checked_add(1)
        .ok_or_else(|| "Display request IDs are exhausted.".to_owned())?;
    let sender = world
        .get_resource::<HostRequests>()
        .ok_or_else(|| Tr::ParkMonitorHost.text().to_owned())?;
    sender
        .0
        .try_send(crate::host::HostRequest::Display(DisplayRequest {
            id,
            action,
        }))
        .map_err(|_| "The display backend is busy; retry this action.".to_owned())?;
    world.resource_mut::<Editor>().next_request = id;
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(id)
}

fn close(world: &mut World, modal: Modal) {
    let preview = world
        .get_resource::<DisplaySnapshot>()
        .and_then(|snapshot| snapshot.preview.clone());
    if let Some(preview) = preview.filter(|preview| !preview.reverting) {
        let _ = send(
            world,
            DisplayAction::Revert {
                token: preview.token,
            },
        );
    } else if let Some((request, _)) = modal.pending {
        let _ = send(world, DisplayAction::Cancel { request });
    }
    crate::native::shortcuts::release_keyboard(world, modal.overlay);
    if world.get_entity(modal.overlay).is_ok() {
        world.despawn(modal.overlay);
    }
    let previous = modal
        .previous
        .filter(|entity| world.get_entity(*entity).is_ok());
    if let Some(previous) = previous {
        world
            .resource_mut::<InputFocus>()
            .set(previous, FocusCause::Navigated);
    } else {
        world.resource_mut::<InputFocus>().clear();
    }
}

fn open(world: &mut World, root: Entity) {
    if crate::native::shortcuts::keyboard_owned(world) {
        return;
    }
    let Some(snapshot) = world
        .get_resource::<DisplaySnapshot>()
        .cloned()
        .filter(|snapshot| !snapshot.heads.is_empty())
    else {
        crate::native::notifications::report(
            world,
            Tr::ParkMonitors.text(),
            Tr::ParkMonitorHost.text(),
        );
        return;
    };
    world.init_resource::<Editor>();
    world.init_resource::<InputFocus>();
    let previous = world.resource::<InputFocus>().get();
    let overlay = world
        .spawn((
            Name::new("desktop.monitors"),
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
                crate::native::actions![Change::Close],
            )]),
        ))
        .id();
    let panel = column(world, overlay);
    world.entity_mut(panel).insert((
        Node {
            width: px(780),
            max_width: percent(96),
            height: percent(92),
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
    ));
    label(world, panel, Tr::ParkMonitors.text(), 22.0);
    let body = column(world, panel);
    world.entity_mut(body).insert((
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_height: px(0),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition::default(),
        crate::native::edit_mode::FocusScroll,
    ));
    world.entity_mut(body).observe(
        |mut event: On<Pointer<Scroll>>, mut positions: Query<&mut ScrollPosition>| {
            if let Ok(mut position) = positions.get_mut(event.entity) {
                position.0.y -= event.y
                    * if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                        24.0
                    } else {
                        1.0
                    };
                event.propagate(false);
            }
        },
    );
    label(world, body, Tr::ParkMonitorDrag.text(), 14.0);
    let board = world
        .spawn((
            Name::new("monitor.diagram"),
            Node {
                width: percent(100),
                height: px(220),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.025, 0.025, 0.03)),
            ChildOf(body),
        ))
        .id();
    let mut rectangles = BTreeMap::new();
    for head in &snapshot.heads {
        let (entity, caption) = button(world, board, overlay, Change::Select(head.id), &head.name);
        world.entity_mut(entity).insert((
            Name::new(format!("monitor.diagram.{}", head.name)),
            crate::native::sand::FixedGeometry,
        ));
        let id = head.id;
        world.entity_mut(entity).observe(
            move |mut event: On<Pointer<DragStart>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| Change::DragStart(id).apply(world, root));
                event.propagate(false);
            },
        );
        world.entity_mut(entity).observe(
            move |mut event: On<Pointer<Drag>>, mut commands: Commands| {
                let distance = event.distance;
                commands
                    .queue(move |world: &mut World| Change::Drag(id, distance).apply(world, root));
                event.propagate(false);
            },
        );
        world.entity_mut(entity).observe(
            move |mut event: On<Pointer<DragEnd>>, mut commands: Commands| {
                commands.queue(move |world: &mut World| Change::DragEnd.apply(world, root));
                event.propagate(false);
            },
        );
        rectangles.insert(id, (entity, caption));
    }
    let selectors = row(world, body);
    let mut editing = Vec::new();
    for head in &snapshot.heads {
        editing.push(
            button(
                world,
                selectors,
                overlay,
                Change::Select(head.id),
                &head.name,
            )
            .0,
        );
    }
    let title = label(world, body, "", 18.0);
    let actions = row(world, body);
    let enabled = button(world, actions, overlay, Change::Enable, Tr::Enabled.text());
    editing.push(enabled.0);
    let identify = button(
        world,
        actions,
        overlay,
        Change::Identify,
        Tr::ParkMonitorIdentify.text(),
    )
    .0;
    let fields_row = row(world, body);
    let fields = [
        field(world, fields_row, "X"),
        field(world, fields_row, "Y"),
        field(world, fields_row, Tr::ParkMonitorScale.text()),
    ];
    editing.extend(fields);
    let rotation = button(world, body, overlay, Change::Rotate, "Rotation: 0° · change");
    editing.push(rotation.0);
    let adaptive_sync = button(world, body, overlay, Change::AdaptiveSync, "Adaptive sync: Off · change");
    editing.push(adaptive_sync.0);
    let adaptive_sync_status = label(world, body, "Adaptive-sync capability has not been reported.", 14.0);
    let color = button(world, body, overlay, Change::Color, "Color: SDR sRGB · change");
    editing.push(color.0);
    let color_status = label(world, body, "Color capability has not been reported.", 14.0);
    let alignment = row(world, body);
    let reference_button = button(
        world,
        alignment,
        overlay,
        Change::Reference,
        Tr::ParkMonitorReference.text(),
    );
    editing.push(reference_button.0);
    world.entity_mut(reference_button.0).insert(NeedsReference);
    for (direction, caption) in [
        Tr::ParkMonitorLeft,
        Tr::ParkMonitorRight,
        Tr::ParkMonitorAbove,
        Tr::ParkMonitorBelow,
    ]
    .into_iter()
    .enumerate()
    {
        let control = button(
            world,
            alignment,
            overlay,
            Change::Align(direction as u8),
            caption.text(),
        )
        .0;
        editing.push(control);
        world.entity_mut(control).insert(NeedsReference);
    }
    label(world, body, Tr::ParkMonitorModes.text(), 16.0);
    let modes = column(world, body);
    let info = label(world, body, "", 14.0);
    let status = label(world, panel, "", 14.0);
    let footer = row(world, panel);
    let test = button(
        world,
        footer,
        overlay,
        Change::Test,
        Tr::ParkMonitorTest.text(),
    )
    .0;
    let apply = button(world, footer, overlay, Change::Apply, Tr::Apply.text()).0;
    let keep = button(
        world,
        footer,
        overlay,
        Change::Keep,
        Tr::ParkMonitorKeep.text(),
    )
    .0;
    let revert = button(
        world,
        footer,
        overlay,
        Change::Revert,
        Tr::ParkMonitorRevert.text(),
    )
    .0;
    let reload = button(world, footer, overlay, Change::Reload, Tr::Refresh.text()).0;
    button(world, footer, overlay, Change::Close, Tr::Close.text());
    let selected = snapshot
        .heads
        .iter()
        .find(|head| head.current.is_some())
        .unwrap_or(&snapshot.heads[0])
        .id;
    let reference = snapshot
        .heads
        .iter()
        .find(|head| head.id != selected)
        .map(|head| head.id);
    let mut modal = Modal {
        overlay,
        previous,
        draft: Draft::new(snapshot),
        selected,
        reference,
        board,
        rectangles,
        fields,
        modes,
        mode_buttons: Vec::new(),
        enabled,
        rotation,
        adaptive_sync,
        adaptive_sync_status,
        color,
        color_status,
        title,
        info,
        status,
        keep,
        revert,
        test,
        apply,
        reload,
        identify,
        editing,
        reference_label: reference_button.1,
        pending: None,
        response: None,
        message: String::new(),
        diagram: Diagram::default(),
        drag: None,
        countdown: None,
    };
    write_fields(world, &modal);
    fill_modes(world, &mut modal);
    crate::native::shortcuts::own_keyboard(world, overlay);
    world
        .resource_mut::<InputFocus>()
        .set(modal.rectangles[&selected].0, FocusCause::Navigated);
    let mut editor = world.resource_mut::<Editor>();
    editor.root = Some(root);
    editor.modal = Some(modal);
}

impl Action for Change {
    fn apply(&self, world: &mut World, target: Entity) {
        if matches!(self, Self::Open) {
            open(world, target);
            return;
        }
        let Some(mut modal) = world
            .get_resource_mut::<Editor>()
            .and_then(|mut editor| editor.modal.take())
        else {
            return;
        };
        if matches!(self, Self::Close) {
            close(world, modal);
            return;
        }
        if matches!(self, Self::Reload) {
            let root = world.get::<ChildOf>(modal.overlay).map(ChildOf::parent);
            if modal.pending.is_none()
                && world
                    .get_resource::<DisplaySnapshot>()
                    .is_some_and(|snapshot| snapshot.preview.is_none())
            {
                close(world, modal);
                if let Some(root) = root {
                    open(world, root);
                }
            } else {
                world.resource_mut::<Editor>().modal = Some(modal);
            }
            return;
        }
        if modal.pending.is_some() {
            world.resource_mut::<Editor>().modal = Some(modal);
            return;
        }
        let snapshot = world
            .get_resource::<DisplaySnapshot>()
            .cloned()
            .unwrap_or_default();
        let read = matches!(
            self,
            Self::Select(_)
                | Self::Enable
                | Self::Mode(_)
                | Self::Rotate
                | Self::AdaptiveSync
                | Self::Color
                | Self::Align(_)
                | Self::Reference
                | Self::Test
                | Self::Apply
                | Self::DragStart(_)
        );
        if (read || matches!(self, Self::Drag(_, _) | Self::DragEnd))
            && (snapshot.preview.is_some() || snapshot.revision != modal.draft.origin.revision)
        {
            modal.drag = None;
            world.resource_mut::<Editor>().modal = Some(modal);
            return;
        }
        if read && let Err(error) = read_fields(world, &mut modal) {
            modal.message = error;
            world.resource_mut::<Editor>().modal = Some(modal);
            return;
        }
        modal.message.clear();
        match *self {
            Self::Select(id) | Self::DragStart(id) => {
                if modal.draft.settings.contains_key(&id) {
                    modal.selected = id;
                    modal.reference = modal
                        .reference
                        .filter(|reference| *reference != id)
                        .or_else(|| {
                            modal
                                .draft
                                .settings
                                .keys()
                                .copied()
                                .find(|reference| *reference != id)
                        });
                    write_fields(world, &modal);
                    fill_modes(world, &mut modal);
                    if matches!(self, Self::DragStart(_)) {
                        modal.drag = Some((id, modal.draft.settings[&id].position, modal.diagram));
                    }
                }
            }
            Self::Enable => {
                if !modal.draft.enabled.remove(&modal.selected) {
                    modal.draft.enabled.insert(modal.selected);
                }
            }
            Self::Mode(index) => {
                if let Some(mode) = modal
                    .draft
                    .origin
                    .heads
                    .iter()
                    .find(|head| head.id == modal.selected)
                    .and_then(|head| head.modes.get(index))
                    .cloned()
                    && let Some(settings) = modal.draft.settings.get_mut(&modal.selected)
                {
                    settings.mode = mode;
                }
            }
            Self::Rotate => {
                use crate::host::mapping::QuarterTurn;
                if let Some(settings) = modal.draft.settings.get_mut(&modal.selected) {
                    settings.rotation = match settings.rotation { QuarterTurn::Normal => QuarterTurn::Clockwise90,
                        QuarterTurn::Clockwise90 => QuarterTurn::UpsideDown, QuarterTurn::UpsideDown => QuarterTurn::Clockwise270,
                        QuarterTurn::Clockwise270 => QuarterTurn::Normal };
                }
            }
            Self::AdaptiveSync => {
                if let Some(settings) = modal.draft.settings.get_mut(&modal.selected) {
                    if settings.adaptive_sync { settings.adaptive_sync = false; }
                    else if snapshot.adaptive_sync.get(&modal.selected).is_some_and(|status| status.support.available()) {
                        settings.adaptive_sync = true;
                    } else {
                        modal.message = snapshot.adaptive_sync.get(&modal.selected).and_then(|status| status.reason.clone())
                            .unwrap_or_else(|| "This output has not reported adaptive-sync support.".into());
                    }
                }
            }
            Self::Color => {
                use crate::host::color::ColorMode;
                if let Some(settings) = modal.draft.settings.get_mut(&modal.selected) {
                    if settings.color == ColorMode::Hdr10 { settings.color = ColorMode::Srgb; }
                    else if snapshot.color.get(&modal.selected).is_some_and(|status| status.hdr10) { settings.color = ColorMode::Hdr10; }
                    else { modal.message = snapshot.color.get(&modal.selected).and_then(|status| status.reason.clone())
                        .unwrap_or_else(|| "This output has not reported a complete HDR10 pipeline.".into()); }
                }
            }
            Self::Reference => {
                let ids: Vec<_> = modal
                    .draft
                    .settings
                    .keys()
                    .copied()
                    .filter(|id| *id != modal.selected)
                    .collect();
                modal.reference = if ids.is_empty() {
                    None
                } else {
                    Some(
                        ids[(ids
                            .iter()
                            .position(|id| Some(*id) == modal.reference)
                            .unwrap_or(0)
                            + 1)
                            % ids.len()],
                    )
                };
            }
            Self::Align(direction) => {
                if let Some(reference) = modal
                    .reference
                    .filter(|reference| *reference != modal.selected)
                {
                    modal.draft.align(modal.selected, reference, direction);
                    write_fields(world, &modal);
                }
            }
            Self::Drag(id, distance) => {
                if let Some((owner, origin, diagram)) = modal.drag
                    && owner == id
                    && diagram.scale > 0.0
                {
                    if let Some(settings) = modal.draft.settings.get_mut(&id) {
                        settings.position = std::array::from_fn(|axis| {
                            (origin[axis] as f32 + distance[axis] / diagram.scale)
                                .round()
                                .clamp(-16384.0, 16384.0) as i32
                        });
                    }
                    modal.draft.snap(id, 8.0 / diagram.scale);
                    write_fields(world, &modal);
                }
            }
            Self::DragEnd => {
                modal.drag = None;
                if let Err(error) = modal.draft.normalize() {
                    modal.message = error;
                }
                write_fields(world, &modal);
            }
            Self::Test | Self::Apply => {
                match modal
                    .draft
                    .normalize()
                    .and_then(|_| modal.draft.normalized())
                {
                    Ok(heads) => {
                        write_fields(world, &modal);
                        let action = if matches!(self, Self::Apply) {
                            DisplayAction::Preview {
                                revision: modal.draft.origin.revision,
                                heads,
                            }
                        } else {
                            DisplayAction::Test {
                                revision: modal.draft.origin.revision,
                                heads,
                            }
                        };
                        match send(world, action) {
                            Ok(id) => modal.pending = Some((id, false)),
                            Err(error) => modal.message = error,
                        }
                    }
                    Err(error) => modal.message = error,
                }
            }
            Self::Keep | Self::Revert => {
                if let Some(preview) = snapshot.preview {
                    let action = if matches!(self, Self::Keep) {
                        DisplayAction::Keep {
                            token: preview.token,
                        }
                    } else {
                        DisplayAction::Revert {
                            token: preview.token,
                        }
                    };
                    match send(world, action) {
                        Ok(id) => modal.pending = Some((id, true)),
                        Err(error) => modal.message = error,
                    }
                }
            }
            Self::Identify => {
                if modal.pending.is_none()
                    && let Err(error) = send(
                        world,
                        DisplayAction::Identify {
                            output: modal.selected,
                        },
                    )
                {
                    modal.message = error;
                }
            }
            Self::Open | Self::Close | Self::Reload => {}
        }
        world.resource_mut::<Editor>().modal = Some(modal);
    }
}

fn diagram(world: &mut World, modal: &mut Modal) {
    let size = world
        .get::<ComputedNode>(modal.board)
        .map(|node| node.size() * node.inverse_scale_factor())
        .unwrap_or(Vec2::new(600.0, 220.0));
    if modal.drag.is_none() {
        let mut minimum = Vec2::splat(f32::MAX);
        let mut maximum = Vec2::splat(f32::MIN);
        for (id, settings) in &modal.draft.settings {
            let origin = Vec2::from_array(settings.position.map(|position| position as f32));
            minimum = minimum.min(origin);
            maximum = maximum.max(origin + Vec2::from_array(modal.draft.size(*id)));
        }
        let span = (maximum - minimum).max(Vec2::ONE);
        let scale = ((size.x - 32.0).max(1.0) / span.x).min((size.y - 32.0).max(1.0) / span.y);
        modal.diagram = Diagram {
            origin: (size - span * scale) * 0.5 - minimum * scale,
            scale,
        };
    }
    for (id, (entity, caption)) in &modal.rectangles {
        let Some(settings) = modal.draft.settings.get(id) else {
            continue;
        };
        let origin = Vec2::from_array(settings.position.map(|position| position as f32))
            * modal.diagram.scale
            + modal.diagram.origin;
        let extent = Vec2::from_array(modal.draft.size(*id)) * modal.diagram.scale;
        let width = px(extent.x.max(20.0));
        let height = px(extent.y.max(20.0));
        let border_width = UiRect::all(px(if *id == modal.selected { 3.0 } else { 1.0 }));
        if let Some(mut node) = world.get_mut::<Node>(*entity)
            && (node.position_type != PositionType::Absolute
                || node.left != px(origin.x)
                || node.top != px(origin.y)
                || node.width != width
                || node.height != height
                || node.overflow != Overflow::clip()
                || node.border != border_width)
        {
            node.position_type = PositionType::Absolute;
            node.left = px(origin.x);
            node.top = px(origin.y);
            node.width = width;
            node.height = height;
            node.overflow = Overflow::clip();
            node.border = border_width;
        }
        let background = BackgroundColor(if modal.draft.enabled.contains(id) {
            Color::srgb(0.11, 0.13, 0.22)
        } else {
            Color::srgb(0.06, 0.06, 0.07)
        });
        let border = BorderColor::all(Color::srgb(0.6, 0.5, 0.95));
        if world.get::<BackgroundColor>(*entity) != Some(&background) {
            world.entity_mut(*entity).insert(background);
        }
        if world.get::<BorderColor>(*entity) != Some(&border) {
            world.entity_mut(*entity).insert(border);
        }
        let name = modal
            .draft
            .origin
            .heads
            .iter()
            .find(|head| head.id == *id)
            .map(|head| head.name.as_str())
            .unwrap_or("");
        text(
            world,
            *caption,
            &format!(
                "{name}{}",
                if modal.draft.enabled.contains(id) {
                    ""
                } else {
                    " · off"
                }
            ),
        );
    }
}

pub(crate) fn identify(world: &mut World) {
    let source = world
        .get_resource::<DisplaySnapshot>()
        .and_then(|snapshot| {
            let identification = snapshot.identification.as_ref()?;
            if identification.deadline <= Instant::now() {
                return None;
            }
            let head = snapshot
                .heads
                .iter()
                .find(|head| head.id == identification.output && head.current.is_some())?
                .clone();
            Some((identification.clone(), head))
        });
    let root = world
        .get_resource::<Editor>()
        .and_then(|editor| editor.root)
        .filter(|root| world.get_entity(*root).is_ok());
    let wanted = source.zip(root).and_then(|((identification, head), root)| {
        let focused = world
            .get_resource::<crate::native::output_views::OutputViews>()
            .map(|views| views.focused == identification.output)
            .unwrap_or(true);
        let target = if focused {
            None
        } else {
            Some(
                world
                    .get_resource::<crate::native::topology::output_views::Targets>()?
                    .0
                    .get(&identification.output)?
                    .clone(),
            )
        };
        let mode = head.modes.get(head.current?)?;
        let caption = format!(
            "{} · {}\n{} × {} · {}×",
            identification.output.0, head.name, mode.size[0], mode.size[1], head.scale
        );
        Some((
            (
                identification.output,
                identification.deadline,
                target,
                root,
                caption,
            ),
            head,
        ))
    });
    world.init_resource::<IdentificationView>();
    let signature = wanted.as_ref().map(|(signature, _)| signature.clone());
    if world.resource::<IdentificationView>().signature == signature {
        return;
    }
    let previous = std::mem::take(&mut world.resource_mut::<IdentificationView>().entities);
    for entity in previous {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    world.resource_mut::<IdentificationView>().signature = signature;
    let Some(((_, _, target, root, caption), head)) = wanted else {
        return;
    };
    let node = Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Start,
        padding: UiRect::top(px(32)),
        ..default()
    };
    let overlay = world
        .spawn((
            Name::new(format!("monitor.identify.{}", head.name)),
            node,
            GlobalZIndex(120),
            Pickable::IGNORE,
        ))
        .id();
    let mut entities = vec![overlay];
    if let Some(target) = target {
        let camera = world
            .spawn((
                Camera2d,
                Camera {
                    order: 2,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                bevy::camera::RenderTarget::Image(bevy::camera::ImageRenderTarget {
                    handle: target,
                    scale_factor: head.scale as f32,
                }),
                bevy::camera::visibility::RenderLayers::none(),
            ))
            .id();
        world.entity_mut(overlay).insert(UiTargetCamera(camera));
        entities.push(camera);
    } else {
        world.entity_mut(overlay).insert(ChildOf(root));
    }
    let caption = label(world, overlay, &caption, 24.0);
    world.entity_mut(caption).insert((
        Pickable::IGNORE,
        BackgroundColor(Color::srgba(0.03, 0.03, 0.05, 0.95)),
    ));
    world.resource_mut::<IdentificationView>().entities = entities;
}

pub(crate) fn update(world: &mut World) {
    let snapshot = world.get_resource::<DisplaySnapshot>().cloned();
    let summaries: Vec<_> = world
        .query_filtered::<Entity, With<Summary>>()
        .iter(world)
        .collect();
    let summary = snapshot
        .as_ref()
        .map(|snapshot| {
            format!(
                "{} connected · {} enabled{}",
                snapshot.heads.len(),
                snapshot
                    .heads
                    .iter()
                    .filter(|head| head.current.is_some())
                    .count(),
                if snapshot.virtual_bounds.is_some() {
                    " · nested virtual outputs"
                } else {
                    ""
                }
            )
        })
        .unwrap_or_else(|| Tr::ParkMonitorHost.text().into());
    for entity in summaries {
        text(world, entity, &summary);
    }
    let Some(mut modal) = world
        .get_resource_mut::<Editor>()
        .and_then(|mut editor| editor.modal.take())
    else {
        return;
    };
    if world.get_entity(modal.overlay).is_err() {
        close(world, modal);
        return;
    }
    let snapshot = snapshot.unwrap_or_default();
    if snapshot.response != modal.response {
        modal.response = snapshot.response.clone();
        if let Some(response) = &snapshot.response
            && let Some((pending, resync)) = modal.pending
            && response.request == pending
            && response.outcome != DisplayOutcome::Pending
        {
            modal.pending = None;
            modal.message = match &response.outcome {
                DisplayOutcome::Tested => {
                    "The backend test passed; no changes were applied.".into()
                }
                DisplayOutcome::Kept => "Monitor settings saved.".into(),
                DisplayOutcome::Reverted => "Previous monitor settings restored.".into(),
                DisplayOutcome::Failed(reason) => reason.clone(),
                _ => String::new(),
            };
            if resync
                && matches!(
                    response.outcome,
                    DisplayOutcome::Kept | DisplayOutcome::Reverted
                )
                && snapshot
                    .heads
                    .iter()
                    .map(|head| (head.id, &head.name, &head.modes))
                    .eq(modal
                        .draft
                        .origin
                        .heads
                        .iter()
                        .map(|head| (head.id, &head.name, &head.modes)))
            {
                modal.draft = Draft::new(snapshot.clone());
                write_fields(world, &modal);
                fill_modes(world, &mut modal);
            }
        }
    }
    let error = read_fields(world, &mut modal)
        .err()
        .or_else(|| modal.draft.normalized().err());
    let stale = modal.draft.origin.revision != snapshot.revision
        && snapshot.preview.is_none()
        && modal.pending.is_none();
    let busy = modal.pending.is_some() || snapshot.preview.is_some();
    if busy || stale {
        modal.drag = None;
    }
    let head = modal
        .draft
        .origin
        .heads
        .iter()
        .find(|head| head.id == modal.selected);
    text(
        world,
        modal.title,
        head.map(|head| head.name.as_str()).unwrap_or(""),
    );
    text(
        world,
        modal.enabled.1,
        if modal.draft.enabled.contains(&modal.selected) {
            Tr::Enabled.text()
        } else {
            Tr::Disabled.text()
        },
    );
    let rotation = modal.draft.settings.get(&modal.selected).map(|settings| settings.rotation).unwrap_or_default();
    let rotation = match rotation { crate::host::mapping::QuarterTurn::Normal => "0°",
        crate::host::mapping::QuarterTurn::Clockwise90 => "90° clockwise", crate::host::mapping::QuarterTurn::UpsideDown => "180°",
        crate::host::mapping::QuarterTurn::Clockwise270 => "270° clockwise" };
    text(world, modal.rotation.1, &format!("Rotation: {rotation} · change"));
    let requested = modal.draft.settings.get(&modal.selected).is_some_and(|settings| settings.adaptive_sync);
    text(world, modal.adaptive_sync.1, if requested { "Adaptive sync: On · change" } else { "Adaptive sync: Off · change" });
    let adaptive = snapshot.adaptive_sync.get(&modal.selected);
    let effective = adaptive.and_then(|status| status.effective).map_or("unconfirmed", |value| if value { "on" } else { "off" });
    let capability = adaptive.map(|status| status.support).unwrap_or_default();
    let capability = match capability {
        crate::host::display::AdaptiveSyncSupport::Unknown => "Capability not reported",
        crate::host::display::AdaptiveSyncSupport::Unsupported => "Unavailable on this output",
        crate::host::display::AdaptiveSyncSupport::Supported => "Supported",
        crate::host::display::AdaptiveSyncSupport::RequiresModeset => "Supported; changing it may briefly blank the display",
    };
    let pending = if adaptive.is_some_and(|status| status.pending) { " · awaiting display confirmation" } else { "" };
    let reason = adaptive.and_then(|status| status.reason.as_deref()).unwrap_or("");
    text(world, modal.adaptive_sync_status, &format!("{capability} · active: {effective}{pending}\n{reason}"));
    let color_request = modal.draft.settings.get(&modal.selected).map_or(Default::default(), |settings| settings.color);
    text(world, modal.color.1, &format!("Color: {} · change", color_request.label()));
    let color = snapshot.color.get(&modal.selected);
    let prepared = color.and_then(|status| status.prepared).map_or("unconfirmed", |value| value.mode.label());
    let effective = color.and_then(|status| status.effective).map_or("unconfirmed", |value| value.mode.label());
    let capability = if color.is_some_and(|status| status.hdr10) { "HDR10 available" } else { "SDR available; HDR10 unavailable or unreported" };
    let pending = if color.is_some_and(|status| status.pending) { " · awaiting display confirmation" } else { "" };
    let peak = color.and_then(|status| status.prepared.filter(|value| value.mode == crate::host::color::ColorMode::Hdr10))
        .map(|value| format!(" · {} peak {} cd/m²", if color.is_some_and(|status| status.peak_reported) { "reported" } else { "nominal" }, value.peak_nits)).unwrap_or_default();
    let reason = color.and_then(|status| status.reason.as_deref()).unwrap_or("");
    text(world, modal.color_status, &format!("{capability} · prepared: {prepared} · last confirmed: {effective}{pending}{peak}\n{reason}"));
    let reference = modal
        .reference
        .and_then(|id| modal.draft.origin.heads.iter().find(|head| head.id == id))
        .map(|head| head.name.as_str())
        .unwrap_or("—");
    text(
        world,
        modal.reference_label,
        &format!("{}: {reference}", Tr::ParkMonitorReference.text()),
    );
    let info = snapshot
        .outputs
        .iter()
        .find(|output| output.id == modal.selected)
        .map(|output| {
            format!(
                "Logical: {:.0} × {:.0} · usable: {}",
                output.bounds.size()[0],
                output.bounds.size()[1],
                output
                    .usable
                    .map(|usable| format!(
                        "{:.0} × {:.0} at {:.0}, {:.0}",
                        usable.size()[0],
                        usable.size()[1],
                        usable.origin()[0],
                        usable.origin()[1]
                    ))
                    .unwrap_or_else(|| "none".into())
            )
        })
        .unwrap_or_else(|| "This monitor is disabled.".into());
    text(world, modal.info, &info);
    for (index, entity) in modal.mode_buttons.iter().copied().enumerate() {
        if let Some(mode) = head.and_then(|head| head.modes.get(index)) {
            let selected = modal
                .draft
                .settings
                .get(&modal.selected)
                .is_some_and(|settings| settings.mode == *mode);
            let caption = format!(
                "{}{} × {} · {:.3} Hz{}",
                if selected { "✓ " } else { "" },
                mode.size[0],
                mode.size[1],
                f64::from(mode.refresh_mhz) / 1000.0,
                if mode.preferred { " ★" } else { "" }
            );
            let child = world.get::<Children>(entity).and_then(|children| {
                children
                    .iter()
                    .find(|child| world.get::<Text>(*child).is_some())
            });
            if let Some(child) = child {
                text(world, child, &caption);
            }
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity)
                && node.label() != Some(caption.as_str())
            {
                node.set_label(caption);
            }
        }
    }
    for entity in modal
        .editing
        .iter()
        .chain(modal.mode_buttons.iter())
        .copied()
    {
        disable(world, entity, busy || stale);
    }
    disable(world, modal.adaptive_sync.0, busy || stale || !requested && !adaptive.is_some_and(|status| status.support.available()));
    disable(world, modal.color.0, busy || stale || color_request != crate::host::color::ColorMode::Hdr10 && !color.is_some_and(|status| status.hdr10));
    for (entity, _) in modal.rectangles.values() {
        disable(world, *entity, busy || stale);
    }
    disable(world, modal.test, busy || stale || error.is_some());
    disable(world, modal.apply, busy || stale || error.is_some());
    disable(world, modal.reload, busy);
    let reference_controls: Vec<_> = world
        .query_filtered::<Entity, With<NeedsReference>>()
        .iter(world)
        .collect();
    for entity in reference_controls {
        disable(world, entity, busy || stale || modal.reference.is_none());
    }
    disable(
        world,
        modal.identify,
        modal.pending.is_some()
            || !snapshot
                .heads
                .iter()
                .any(|head| head.id == modal.selected && head.current.is_some()),
    );
    let can_confirm = snapshot
        .preview
        .as_ref()
        .is_some_and(|preview| !preview.reverting)
        && modal.pending.is_none();
    disable(world, modal.keep, !can_confirm || !crate::host::display::color_confirmation_ready(&snapshot.heads, &snapshot.color));
    disable(world, modal.revert, !can_confirm);
    let status = if let Some(preview) = &snapshot.preview {
        let seconds = preview
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64()
            .ceil() as u64;
        if modal.countdown != Some(seconds) {
            modal.countdown = Some(seconds);
            if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
                wake.after(Duration::from_secs(1));
            }
        }
        if preview.reverting {
            "Restoring previous monitor settings…".into()
        } else {
            format!(
                "{} · {} s · {}",
                Tr::ParkMonitorKeep.text(),
                seconds,
                modal.message
            )
        }
    } else if stale {
        Tr::ParkMonitorStale.text().into()
    } else if modal.pending.is_some() {
        "Waiting for the display backend…".into()
    } else {
        error.unwrap_or_else(|| modal.message.clone())
    };
    text(world, modal.status, &status);
    diagram(world, &mut modal);
    world.resource_mut::<Editor>().modal = Some(modal);
}

#[cfg(test)]
mod tests;
