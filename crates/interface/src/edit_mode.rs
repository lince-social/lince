use crate::{
    actions::{
        Action, ActionButton, ActionsPlugin, KeyBinding, KeyBindings, Modifiers, WindowActionTarget,
    },
    canvas::CanvasView,
    sand::{button, text_editor},
    sand_store::{SandKind, StoredSand, spawn_sand},
    theme::{PURPLE, Typography},
    workspace::{self, PrepareWorkspaces, Workspaces},
};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, FocusGained, FocusLost, InputFocus, tab_navigation::TabGroup},
    picking::events::Scroll,
    prelude::*,
    text::EditableText,
};

pub use crate::canvas_colors::ColorField;
pub use crate::sand_text_editor::TextAction;

#[derive(EntityEvent, Clone, Copy)]
pub struct EditPopupToggle {
    pub entity: Entity,
    pub enabled: bool,
}

#[derive(Component)]
pub struct EditMode {
    pub enabled: bool,
    pub panel: Entity,
    pub toggle: Entity,
    name: Option<Entity>,
    confirm_remove: Option<u64>,
    general: bool,
    credits: bool,
    store: bool,
    canvas: bool,
    notifications: bool,
    information: bool,
    customization: bool,
    pub(crate) areas: bool,
    content: Option<Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Open,
    Toggle,
    Close,
    CreateWorkspace,
    TogglePhysics,
    ReloadWorkspaceSettings,
    DisarmAreaChanges,
    SwitchWorkspace(u64),
    RemoveWorkspace(u64),
    ConfirmRemoveWorkspace,
    CancelRemoveWorkspace,
    AddSand(SandKind),
    RemoveSand(Entity),
    EditSand(Entity),
    Text(TextAction),
    Credits,
    General,
    Workspaces,
    Store,
    Canvas,
    Notifications,
    Information,
    Customization,
    Areas,
    Area(crate::area_panel::AreaAction),
    ResetCanvasColors,
    CanvasColor(ColorField, [u8; 3]),
}

impl Action for EditAction {
    fn connections(&self, world: &World, root: Entity) -> Vec<crate::inspection::Connection> {
        crate::inspection::edit_connection(world, root, *self)
            .into_iter()
            .collect()
    }

    fn apply(&self, world: &mut World, target: Entity) {
        apply(world, target, *self);
    }
}

#[derive(Component)]
pub struct EditControl {
    pub root: Entity,
    pub action: EditAction,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    WorkspaceName,
    StartingText,
}

#[derive(Component)]
struct WorkspaceNotice(Entity);

pub struct EditModePlugin;
impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        app.init_resource::<crate::tokens::ThemeSettings>()
            .add_observer(crate::customization::toggle)
            .add_systems(
                Update,
                (
                    setup
                        .after(PrepareWorkspaces)
                        .after(crate::canvas_controls::create_controls),
                    notices.after(setup),
                    expand_tabs,
                    crate::sand_store::refresh.after(setup),
                ),
            )
            .add_systems(
                PostUpdate,
                (reveal_focus, anchor_panel).after(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                (
                    autosave_name,
                    crate::customization::autosave,
                    crate::canvas_colors::autosave,
                    crate::sand_text_editor::autosave,
                    crate::canvas_colors::preview,
                )
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            )
            .add_observer(
                |event: On<FocusGained>, mut controls: Query<&mut Outline, With<EditControl>>| {
                    if event.cause == FocusCause::Navigated
                        && let Ok(mut outline) = controls.get_mut(event.entity)
                    {
                        outline.width = px(2);
                    }
                },
            )
            .add_observer(
                |event: On<FocusLost>, mut controls: Query<&mut Outline, With<EditControl>>| {
                    if let Ok(mut outline) = controls.get_mut(event.entity) {
                        outline.width = px(0);
                    }
                },
            );
    }
}

#[derive(Component)]
struct EditTabs;

fn expand_tabs(
    settings: Res<crate::tokens::ThemeSettings>,
    hover: Option<Res<bevy::picking::hover::HoverMap>>,
    focus: Res<InputFocus>,
    outlines: Query<&Outline>,
    parents: Query<&ChildOf>,
    mut tabs: Query<(Entity, &mut Node), With<EditTabs>>,
) {
    for (entity, mut node) in &mut tabs {
        let contains = |mut candidate: Entity| {
            loop {
                if candidate == entity {
                    return true;
                }
                let Ok(parent) = parents.get(candidate) else {
                    return false;
                };
                candidate = parent.parent();
            }
        };
        let expanded = focus.get().is_some_and(|entity| {
            contains(entity)
                && outlines
                    .get(entity)
                    .is_ok_and(|outline| outline.width != px(0))
        }) || hover.as_ref().is_some_and(|hover| {
            hover
                .values()
                .any(|hits| hits.keys().copied().any(contains))
        });
        let number = |token| {
            settings
                .resolve(token, None, &Default::default())
                .0
                .number()
        };
        let height = if expanded {
            Val::Auto
        } else {
            px(number(crate::tokens::Token::IconSize)
                + 2.0
                    * (number(crate::tokens::Token::IconPadding)
                        + number(crate::tokens::Token::ControlBorder)))
        };
        if node.max_height != height {
            node.max_height = height;
        }
    }
}

fn autosave_name(world: &mut World) {
    let edits: Vec<_> = world
        .query::<(Entity, &EditMode)>()
        .iter(world)
        .filter(|(_, mode)| mode.enabled)
        .filter_map(|(root, mode)| {
            let text = world.get::<EditableText>(mode.name?)?;
            if text.is_composing() || text.pending_paste.is_some() {
                return None;
            }
            let name = text.value().to_string();
            let spaces = world.get::<Workspaces>(root)?;
            let current = spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)?;
            (!name.trim().is_empty() && name.trim() != current.name).then_some((root, name))
        })
        .collect();
    for (root, name) in edits {
        if !workspace::rename(world, root, &name) {
            continue;
        }
        let active = world.get::<Workspaces>(root).unwrap().active;
        let labels: Vec<_> = world
            .query::<(&EditControl, &Children)>()
            .iter(world)
            .filter(|(control, _)| {
                control.root == root && control.action == EditAction::SwitchWorkspace(active)
            })
            .flat_map(|(_, children)| children.iter())
            .collect();
        for entity in labels {
            if let Some(mut text) = world.get_mut::<Text>(entity) {
                text.0 = name.trim().into();
            }
        }
    }
}

fn reveal_focus(
    focus: Res<InputFocus>,
    mut previous: Local<Option<Entity>>,
    parents: Query<&ChildOf>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    modes: Query<&EditMode>,
    mut scrolls: Query<&mut ScrollPosition>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    if *previous == focus.get() {
        return;
    }
    *previous = focus.get();
    let Some(entity) = focus.get() else {
        return;
    };
    let Ok((node, transform)) = geometry.get(entity) else {
        return;
    };
    for mode in &modes {
        if !mode.enabled {
            continue;
        }
        let mut ancestor = entity;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if ancestor != mode.panel {
                continue;
            }
            let Ok((panel, panel_transform)) = geometry.get(mode.panel) else {
                break;
            };
            let top = transform.translation.y - node.size().y * 0.5;
            let bottom = top + node.size().y;
            let panel_top = panel_transform.translation.y - panel.size().y * 0.5 + 8.0;
            let panel_bottom = panel_top + panel.size().y - 16.0;
            let delta = if top < panel_top {
                top - panel_top
            } else if bottom > panel_bottom {
                bottom - panel_bottom
            } else {
                0.0
            };
            if delta != 0.0
                && let Ok(mut scroll) = scrolls.get_mut(mode.panel)
            {
                scroll.0.y = (scroll.0.y + delta * panel.inverse_scale_factor()).max(0.0);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            }
            break;
        }
    }
}

fn anchor_panel(
    modes: Query<(Entity, &EditMode)>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut nodes: Query<&mut Node>,
    wake: Option<Res<crate::wake::WakeSignal>>,
) {
    for (root, mode) in &modes {
        let Ok((root_node, root_transform)) = geometry.get(root) else {
            continue;
        };
        let Ok((button, transform)) = geometry.get(mode.toggle) else {
            continue;
        };
        if root_node.size().min_element() <= 0.0 || button.size().min_element() <= 0.0 {
            continue;
        }
        let scale = root_node.inverse_scale_factor();
        let position = transform.translation - root_transform.translation + root_node.size() * 0.5;
        let right =
            px(((root_node.size().x - position.x - button.size().x * 0.5) * scale).max(12.0));
        let bottom = px((root_node.size().y - position.y + button.size().y * 0.5) * scale + 8.0);
        if let Ok(mut node) = nodes.get_mut(mode.panel)
            && (node.right != right || node.bottom != bottom)
        {
            node.right = right;
            node.bottom = bottom;
            if let Some(wake) = &wake {
                wake.ring();
            }
        }
    }
}

fn setup(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<Workspaces>, Without<EditMode>, Without<crate::laboratory::LaboratoryRoot>)>()
        .iter(world)
        .collect();
    for root in roots {
        if world.get::<crate::inspection::Inspection>(root).is_none() {
            world
                .entity_mut(root)
                .insert(crate::inspection::Inspection::default());
        }
        let notice = label(world, root, "", 14.0);
        world.entity_mut(notice).insert((
            WorkspaceNotice(root),
            GlobalZIndex(22),
            crate::token_style::background(crate::tokens::Token::Surface),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                bottom: px(64),
                left: px(12),
                right: px(12),
                ..default()
            },
        ));
        let toolbar = crate::canvas_controls::toolbar(world, root);
        let toggle = control(world, root, toolbar, EditAction::Toggle, "Edit mode");
        let disarm = control(
            world,
            root,
            toolbar,
            EditAction::DisarmAreaChanges,
            "Disarm area changes",
        );
        world
            .entity_mut(disarm)
            .insert(crate::area_mutation::DisarmControl(root));
        world.get_mut::<Node>(disarm).unwrap().display = Display::None;
        world.entity_mut(toggle).insert(Node {
            width: px(120),
            justify_content: JustifyContent::Center,
            padding: UiRect::axes(px(12), px(8)),
            border: UiRect::all(px(1)),
            flex_shrink: 0.0,
            ..default()
        });
        let panel = world
            .spawn((
                crate::inspection::InspectionExcluded,
                crate::sand::Square,
                Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    right: px(16),
                    top: px(12),
                    bottom: px(64),
                    width: px(376),
                    max_width: percent(94),
                    padding: UiRect::all(px(16)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexStart,
                    align_items: AlignItems::Stretch,
                    row_gap: px(10),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                crate::token_style::background(crate::tokens::Token::Surface),
                crate::token_style::border(crate::tokens::Token::Accent),
                GlobalZIndex(21),
                ScrollPosition::default(),
                TabGroup::modal(),
                ChildOf(root),
            ))
            .observe(
                |mut event: On<Pointer<Scroll>>, mut scrolls: Query<&mut ScrollPosition>| {
                    if let Ok(mut position) = scrolls.get_mut(event.entity) {
                        let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line
                        {
                            24.0
                        } else {
                            1.0
                        };
                        position.0.y -= event.y * multiplier;
                        event.propagate(false);
                    }
                },
            )
            .id();
        world.entity_mut(root).insert(EditMode {
            enabled: false,
            panel,
            toggle,
            name: None,
            confirm_remove: None,
            general: false,
            credits: false,
            store: false,
            canvas: false,
            notifications: false,
            information: false,
            customization: false,
            areas: false,
            content: None,
        });
        let bindings = [
            KeyBinding::new(
                KeyCode::KeyE,
                Modifiers::ALT,
                crate::actions![EditAction::Toggle],
            ),
            KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::actions![crate::inspection::Deselect],
            ),
        ];
        if let Some(mut existing) = world.get_mut::<KeyBindings>(root) {
            existing.0.splice(0..0, bindings);
        } else {
            world.entity_mut(root).insert(KeyBindings(bindings.into()));
        }
        if let Ok(window) = world
            .query_filtered::<Entity, (
                With<bevy::window::PrimaryWindow>,
                Without<WindowActionTarget>,
            )>()
            .single(world)
        {
            world.entity_mut(window).insert(WindowActionTarget(root));
        }
    }
}

fn notices(
    spaces: Query<&Workspaces, Changed<Workspaces>>,
    mut notices: Query<(&WorkspaceNotice, &mut Text, &mut Node)>,
) {
    for (notice, mut text, mut node) in &mut notices {
        let Ok(spaces) = spaces.get(notice.0) else {
            continue;
        };
        let value = spaces.error.as_deref().unwrap_or_default();
        if text.0 != value {
            text.0 = value.into();
        }
        let display = if value.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
}

fn apply(world: &mut World, root: Entity, action: EditAction) {
    if action == EditAction::DisarmAreaChanges {
        crate::area_mutation::disarm_all(world, root);
        return;
    }
    let Some(mode) = world.get::<EditMode>(root) else {
        return;
    };
    if let EditAction::Area(crate::area_panel::AreaAction::Select(entity)) = action
        && mode.areas
        && world
            .get::<crate::area_panel::AreaEditor>(root)
            .is_some_and(|editor| editor.selected == Some(entity) && editor.tool.is_none())
    {
        if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
            inspection.selected = Some(entity);
        }
        return;
    }
    if matches!(action, EditAction::Open | EditAction::Toggle) {
        set_open(world, root, action == EditAction::Open || !mode.enabled);
        return;
    }
    if !mode.enabled {
        return;
    }
    if matches!(
        action,
        EditAction::General
            | EditAction::Workspaces
            | EditAction::Store
            | EditAction::Canvas
            | EditAction::Notifications
            | EditAction::Information
            | EditAction::Customization
            | EditAction::Credits
            | EditAction::EditSand(_)
            | EditAction::SwitchWorkspace(_)
            | EditAction::CreateWorkspace
            | EditAction::ConfirmRemoveWorkspace
    ) {
        world.get_mut::<EditMode>(root).unwrap().areas = false;
        if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
            editor.cancel();
        }
    }
    if matches!(
        action,
        EditAction::General
            | EditAction::Workspaces
            | EditAction::Store
            | EditAction::Canvas
            | EditAction::Areas
            | EditAction::Area(_)
            | EditAction::Notifications
            | EditAction::Information
            | EditAction::Customization
            | EditAction::Credits
            | EditAction::EditSand(_)
    ) {
        world.get_mut::<EditMode>(root).unwrap().general = action == EditAction::General;
        world.get_mut::<EditMode>(root).unwrap().information = action == EditAction::Information;
    }
    let mode = world.get::<EditMode>(root).unwrap();
    match action {
        EditAction::TogglePhysics => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            let enabled = crate::workspace_config::enabled(world, root, active);
            crate::workspace_config::set_physics(world, root, active, !enabled);
        }
        EditAction::ReloadWorkspaceSettings => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            crate::workspace_config::reload(world, root, active);
        }
        EditAction::Areas | EditAction::Area(_) => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.areas = true;
            mode.customization = false;
            mode.notifications = false;
            mode.canvas = false;
            mode.credits = false;
            if let EditAction::Area(action) = action {
                crate::area_panel::apply(world, root, action);
            }
        }
        EditAction::Close => {
            set_open(world, root, false);
            return;
        }
        EditAction::CreateWorkspace => workspace::create(world, root),
        EditAction::SwitchWorkspace(id) => {
            workspace::switch(world, root, id);
        }
        EditAction::RemoveWorkspace(workspace) => {
            let spaces = world.get::<Workspaces>(root).unwrap();
            if spaces.entries.len() > 1 && spaces.entries.iter().any(|entry| entry.id == workspace)
            {
                world.get_mut::<EditMode>(root).unwrap().confirm_remove = Some(workspace);
            }
        }
        EditAction::ConfirmRemoveWorkspace => {
            if let Some(workspace) = world.get::<EditMode>(root).unwrap().confirm_remove {
                workspace::remove(world, root, workspace);
            }
            world.get_mut::<EditMode>(root).unwrap().confirm_remove = None;
        }
        EditAction::CancelRemoveWorkspace => {
            world.get_mut::<EditMode>(root).unwrap().confirm_remove = None;
        }
        EditAction::AddSand(kind) => {
            let initial = mode
                .content
                .and_then(|entity| world.get::<EditableText>(entity))
                .map(|text| text.value().to_string())
                .unwrap_or_default();
            let active = world.get::<Workspaces>(root).unwrap().active;
            let center = world.get::<CanvasView>(root).unwrap().center;
            if !center.is_finite() {
                return;
            }
            let initial = if initial.is_empty() && kind != SandKind::Square {
                kind.name()
            } else {
                &initial
            };
            spawn_sand(world, root, active, kind, initial, center);
        }
        EditAction::RemoveSand(sand) => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            if world
                .get::<ChildOf>(sand)
                .is_some_and(|parent| parent.parent() == root)
                && world
                    .get::<workspace::WorkspaceMember>(sand)
                    .is_some_and(|member| member.0 == active)
                && world.get::<StoredSand>(sand).is_some()
            {
                world.despawn(sand);
            }
        }
        EditAction::EditSand(sand) => crate::sand_text_editor::open(world, root, sand),
        EditAction::Text(action) => {
            if !crate::sand_text_editor::apply(world, root, action) {
                return;
            }
        }
        EditAction::Credits => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.credits = true;
            mode.general = false;
            mode.areas = false;
            mode.customization = false;
            mode.notifications = false;
            mode.canvas = false;
        }
        EditAction::Customization => {
            world.trigger(crate::customization::GlobalCustomizationPanelToggle { entity: root });
            return;
        }
        EditAction::General | EditAction::Workspaces => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.notifications = false;
            mode.store = false;
            mode.canvas = false;
            mode.credits = false;
        }
        EditAction::Store => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.notifications = false;
            mode.store = true;
            mode.canvas = false;
            mode.credits = false;
        }
        EditAction::Canvas => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.notifications = false;
            mode.customization = false;
            mode.canvas = true;
            mode.credits = false;
        }
        EditAction::Notifications => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.notifications = true;
            mode.canvas = false;
            mode.credits = false;
        }
        EditAction::Information => {}
        EditAction::ResetCanvasColors => {
            if !mode.canvas || !crate::canvas_colors::save(world, root, true) {
                return;
            }
        }
        EditAction::CanvasColor(field, rgb) => {
            if mode.canvas {
                crate::canvas_colors::preset(world, root, field, rgb);
            }
            return;
        }
        EditAction::Open | EditAction::Toggle | EditAction::DisarmAreaChanges => {}
    }
    if matches!(
        action,
        EditAction::SwitchWorkspace(_) | EditAction::CreateWorkspace
    ) {
        world.get_mut::<EditMode>(root).unwrap().confirm_remove = None;
    }
    if matches!(
        action,
        EditAction::General
            | EditAction::Workspaces
            | EditAction::Customization
            | EditAction::Notifications
            | EditAction::Information
            | EditAction::Canvas
            | EditAction::Store
            | EditAction::Credits
            | EditAction::SwitchWorkspace(_)
            | EditAction::CreateWorkspace
            | EditAction::ConfirmRemoveWorkspace
    ) {
        let panel = world.get::<EditMode>(root).unwrap().panel;
        world
            .entity_mut(panel)
            .remove::<crate::sand_text_editor::TextPanel>();
    }
    render_panel(world, root);
}

pub(crate) fn toggle_customization(world: &mut World, root: Entity) {
    let Some(mode) = world.get::<EditMode>(root) else {
        return;
    };
    if mode.enabled && mode.customization {
        set_open(world, root, false);
        return;
    }
    set_open(world, root, true);
    let mut mode = world.get_mut::<EditMode>(root).unwrap();
    mode.general = false;
    mode.customization = true;
    mode.information = false;
    mode.areas = false;
    mode.notifications = false;
    mode.canvas = false;
    mode.credits = false;
    let panel = mode.panel;
    world
        .entity_mut(panel)
        .remove::<crate::sand_text_editor::TextPanel>();
    render_panel(world, root);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn set_open(world: &mut World, root: Entity, enabled: bool) {
    let Some(mut mode) = world.get_mut::<EditMode>(root) else {
        return;
    };
    if mode.enabled == enabled {
        return;
    }
    mode.enabled = enabled;
    mode.confirm_remove = None;
    mode.credits = false;
    let panel = mode.panel;
    let toggle = mode.toggle;
    if !enabled && let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root) {
        editor.cancel();
    }
    world.get_mut::<Node>(panel).unwrap().display = if enabled {
        Display::Flex
    } else {
        Display::None
    };
    let label = if enabled { "Done editing" } else { "Edit mode" };
    world
        .get_mut::<AccessibilityNode>(toggle)
        .unwrap()
        .set_label(label);
    world
        .get_mut::<crate::icons::IconButton>(toggle)
        .unwrap()
        .label = label.into();
    if enabled {
        render_panel(world, root);
    } else {
        world
            .resource_mut::<InputFocus>()
            .set(toggle, FocusCause::Pressed);
    }
    world.trigger(EditPopupToggle {
        entity: root,
        enabled,
    });
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) fn label(world: &mut World, parent: Entity, value: &str, size: f32) -> Entity {
    let font = world.resource::<Typography>().text(size);
    world
        .spawn((
            Text::new(value),
            font,
            crate::token_style::text(crate::tokens::Token::Ink),
            Node {
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) fn control(
    world: &mut World,
    root: Entity,
    parent: Entity,
    action: EditAction,
    value: &str,
) -> Entity {
    let entity = world
        .spawn((
            EditControl { root, action },
            ActionButton::new(root, crate::actions![action]),
            button(0),
            Node {
                padding: UiRect::axes(px(10), px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::OutlineToken(crate::tokens::Token::Accent),
            Outline {
                width: px(0),
                offset: px(2),
                color: PURPLE,
            },
            crate::token_style::border(crate::tokens::Token::Accent),
            crate::token_style::background(crate::tokens::Token::Surface),
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(value);
    use crate::icons::Icon;
    let icon = match action {
        EditAction::Open | EditAction::Toggle | EditAction::EditSand(_) => Some(Icon::Paintbrush),
        EditAction::Close
        | EditAction::RemoveWorkspace(_)
        | EditAction::RemoveSand(_)
        | EditAction::CancelRemoveWorkspace
        | EditAction::Text(TextAction::Remove) => Some(Icon::Close),
        EditAction::CreateWorkspace => Some(Icon::Plus),
        EditAction::ConfirmRemoveWorkspace => Some(Icon::Check),
        EditAction::Credits | EditAction::Information => Some(Icon::Info),
        EditAction::General => Some(Icon::General),
        EditAction::Workspaces => Some(Icon::Workspaces),
        EditAction::Store => Some(Icon::Store),
        EditAction::Canvas | EditAction::Customization => Some(Icon::Palette),
        EditAction::Areas => Some(Icon::Circle),
        EditAction::Area(_) => None,
        EditAction::TogglePhysics
        | EditAction::ReloadWorkspaceSettings
        | EditAction::DisarmAreaChanges => None,
        EditAction::Notifications => Some(Icon::Bell),
        EditAction::ResetCanvasColors => Some(Icon::Reset),
        EditAction::CanvasColor(_, rgb) => {
            world.entity_mut(entity).insert(crate::icons::IconStyle {
                color: crate::canvas_background::color(rgb),
                ..default()
            });
            Some(Icon::Circle)
        }
        EditAction::Text(TextAction::Add(editable)) => Some(if editable {
            Icon::EditableText
        } else {
            Icon::Text
        }),
        EditAction::Text(TextAction::Overflow(overflow)) => Some(match overflow {
            crate::sand_text::TextOverflow::Scroll => Icon::Scroll,
            crate::sand_text::TextOverflow::Grow => Icon::Grow,
        }),
        EditAction::AddSand(kind) => Some(match kind {
            SandKind::Square => Icon::Square,
            SandKind::Text => Icon::Text,
            SandKind::EditableText => Icon::EditableText,
        }),
        EditAction::SwitchWorkspace(_) | EditAction::Text(TextAction::Select(_)) => None,
    };
    if let Some(icon) = icon {
        world
            .entity_mut(entity)
            .insert(crate::icons::IconButton::new(icon, value));
        if action == EditAction::Notifications {
            world
                .entity_mut(entity)
                .insert(crate::notifications::NotificationCount);
        }
    } else {
        let text = label(world, entity, value, 15.0);
        if action == EditAction::Notifications {
            world
                .entity_mut(text)
                .insert(crate::notifications::NotificationCount);
        }
    }
    entity
}

pub(crate) fn render_panel(world: &mut World, root: Entity) {
    let mode = world.get::<EditMode>(root).unwrap();
    let panel = mode.panel;
    let confirm = mode.confirm_remove;
    let credits = mode.credits;
    let store = mode.store;
    let canvas = mode.canvas;
    let notifications = mode.notifications;
    let customization = mode.customization;
    let areas = mode.areas;
    let general = mode.general;
    let information = mode.information;
    let children: Vec<_> = world
        .get::<Children>(panel)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    let existing_tabs = children
        .iter()
        .copied()
        .find(|child| world.get::<EditTabs>(*child).is_some());
    for child in children {
        if Some(child) != existing_tabs {
            world.despawn(child);
        }
    }
    world.get_mut::<Node>(panel).unwrap().width = px(if customization { 720 } else { 376 });
    world
        .entity_mut(panel)
        .insert(crate::token_metrics::WidthToken(
            if customization {
                crate::tokens::Token::CustomizationWidth
            } else {
                crate::tokens::Token::PanelWidth
            },
            false,
        ));
    world.get_mut::<ScrollPosition>(panel).unwrap().0 = Vec2::ZERO;
    if existing_tabs.is_none() {
        let tabs = world
            .spawn((
                EditTabs,
                Node {
                    max_height: px(40),
                    overflow: Overflow::clip(),
                    column_gap: px(8),
                    width: percent(100),
                    flex_wrap: FlexWrap::Wrap,
                    align_content: AlignContent::FlexStart,
                    row_gap: px(8),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        control(world, root, tabs, EditAction::General, "General");
        control(world, root, tabs, EditAction::Information, "Information");
        control(world, root, tabs, EditAction::Workspaces, "Workspaces");
        control(world, root, tabs, EditAction::Store, "Sand store");
        control(world, root, tabs, EditAction::Areas, "Areas of influence");
        control(
            world,
            root,
            tabs,
            EditAction::Customization,
            "Customization",
        );
        control(
            world,
            root,
            tabs,
            EditAction::Notifications,
            "Notifications",
        );
        control(
            world,
            root,
            tabs,
            EditAction::Credits,
            "Licenses and credits",
        );
        world.spawn((
            Node {
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(tabs),
        ));
        control(world, root, tabs, EditAction::Close, "Close edit mode");
    }
    if information {
        crate::information::panel(world, root, panel);
        return;
    }
    if general {
        label(world, panel, "General", 22.0);
        crate::inspection::controls(world, root, panel);
        return;
    }
    if areas {
        crate::area_panel::render(world, root, panel);
        return;
    }
    if customization {
        crate::customization::render(world, root, panel);
        return;
    }
    if notifications {
        crate::notifications::panel(world, panel);
        return;
    }
    if canvas {
        crate::canvas_colors::render(world, root, panel);
        return;
    }
    if crate::sand_text_editor::render(world, root, panel) {
        return;
    }
    if credits {
        crate::credits::render(world, panel);
        return;
    }
    let active = world.get::<Workspaces>(root).unwrap().active;
    if !store {
        let heading = world
            .spawn((
                Node {
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        label(world, heading, "Workspace", 22.0);
        control(
            world,
            root,
            heading,
            EditAction::CreateWorkspace,
            "New workspace",
        );
        let spaces = world.get::<Workspaces>(root).unwrap();
        let active = spaces.active;
        let entries = spaces.entries.clone();
        let error = spaces.error.clone();
        world.get_mut::<EditMode>(root).unwrap().name = None;
        for entry in &entries {
            let row = world
                .spawn((
                    Node {
                        column_gap: px(8),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    ChildOf(panel),
                ))
                .id();
            let switch = if entry.id == active {
                let bundle = text_editor(&entry.name, world.resource::<Typography>(), 0);
                let editor = world
                    .spawn((bundle, EditField::WorkspaceName, ChildOf(row)))
                    .id();
                world.entity_mut(editor).insert(EditableText {
                    allow_newlines: false,
                    visible_lines: Some(1.0),
                    max_characters: Some(80),
                    ..crate::sand::editable(&entry.name)
                });
                world.get_mut::<EditMode>(root).unwrap().name = Some(editor);
                editor
            } else {
                control(
                    world,
                    root,
                    row,
                    EditAction::SwitchWorkspace(entry.id),
                    &entry.name,
                )
            };
            world.get_mut::<Node>(switch).unwrap().flex_grow = 1.0;
            world.get_mut::<Node>(switch).unwrap().width = px(0);
            if entries.len() > 1 {
                control(
                    world,
                    root,
                    row,
                    EditAction::RemoveWorkspace(entry.id),
                    &format!("Remove {}", entry.name),
                );
            }
        }
        crate::workspace_config::controls(world, root, panel);
        if let Some(removed) = confirm {
            if let Some(name) = entries
                .iter()
                .find(|entry| entry.id == removed)
                .map(|entry| entry.name.as_str())
            {
                label(
                    world,
                    panel,
                    &format!("Remove {name}? Its Sands will move to another workspace."),
                    14.0,
                );
                control(
                    world,
                    root,
                    panel,
                    EditAction::ConfirmRemoveWorkspace,
                    "Remove and move Sands",
                );
                control(
                    world,
                    root,
                    panel,
                    EditAction::CancelRemoveWorkspace,
                    "Keep workspace",
                );
            }
        }
        if let Some(error) = error {
            label(world, panel, &error, 14.0);
        }
    } else {
        let heading = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    column_gap: px(8),
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        label(world, heading, "Sand store", 22.0);
        control(
            world,
            root,
            heading,
            EditAction::Credits,
            "Sand credits and licenses",
        );
        label(
            world,
            panel,
            "Add a Sand at the camera. Notes are saved as you type.",
            14.0,
        );
        label(world, panel, "Starting text", 14.0);
        let bundle = text_editor("", world.resource::<Typography>(), 0);
        let content = world
            .spawn((
                bundle,
                EditField::StartingText,
                ChildOf(panel),
                crate::token_style::background(crate::tokens::Token::Surface),
            ))
            .id();
        world
            .get_mut::<EditableText>(content)
            .unwrap()
            .max_characters = Some(4096);
        world
            .get_mut::<EditableText>(content)
            .unwrap()
            .visible_lines = Some(1.0);
        world.entity_mut(content).insert((
            Node {
                width: percent(100),
                height: px(36),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                flex_grow: 0.0,
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::Accent),
        ));
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(content) {
            node.set_label("Starting text for the new Sand");
        }
        world.get_mut::<EditMode>(root).unwrap().content = Some(content);
        for kind in SandKind::ALL {
            crate::sand_store::entry(world, root, panel, kind, None);
        }
        let sands: Vec<_> = world
            .query::<(Entity, &ChildOf, &workspace::WorkspaceMember, &StoredSand)>()
            .iter(world)
            .filter(|(_, parent, member, _)| parent.parent() == root && member.0 == active)
            .map(|(entity, _, _, sand)| (entity, sand.kind))
            .collect();
        if !sands.is_empty() {
            label(world, panel, "In this workspace", 18.0);
        }
        for (index, (entity, kind)) in sands.into_iter().enumerate() {
            crate::sand_store::entry(world, root, panel, kind, Some((entity, index)));
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::{
        container::BoxRoot,
        workspace::{WorkspaceMember, WorkspacePlugin},
    };
    use bevy::ui_widgets::Activate;

    #[derive(Resource, Default)]
    struct Toggles(Vec<(Entity, bool)>);

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .init_resource::<InputFocus>()
            .init_resource::<Toggles>()
            .add_plugins((WorkspacePlugin, EditModePlugin))
            .add_observer(|event: On<EditPopupToggle>, mut toggles: ResMut<Toggles>| {
                toggles.0.push((event.entity, event.enabled));
            });
        let root = app.world_mut().spawn(BoxRoot).id();
        app.update();
        (app, root)
    }

    fn activate(app: &mut App, root: Entity, action: EditAction) {
        if action == EditAction::Canvas {
            action.apply(app.world_mut(), root);
            app.update();
            return;
        }
        let entity = app
            .world_mut()
            .query::<(Entity, &EditControl)>()
            .iter(app.world())
            .find(|(_, control)| control.root == root && control.action == action)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity });
        app.update();
    }

    #[cfg_attr(test, test)]
    fn general_settings_are_separate_and_tab_entities_survive_navigation() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        let tabs = app
            .world_mut()
            .query_filtered::<Entity, With<EditTabs>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<Node>(tabs).unwrap().max_height, px(40));
        EditAction::General.apply(app.world_mut(), root);
        assert!(app.world().get::<EditMode>(root).unwrap().general);
        assert!(app.world().get_entity(tabs).is_ok());
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Hover")
        );
        EditAction::Areas.apply(app.world_mut(), root);
        assert!(!app.world().get::<EditMode>(root).unwrap().general);
        assert!(
            !app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Hover")
        );
        assert!(app.world().get_entity(tabs).is_ok());
        app.init_resource::<bevy::picking::hover::HoverMap>();
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .entry(bevy::picking::pointer::PointerId::Mouse)
            .or_default()
            .insert(
                tabs,
                bevy::picking::backend::HitData::new(root, 0.0, None, None),
            );
        app.update();
        assert_eq!(app.world().get::<Node>(tabs).unwrap().max_height, Val::Auto);
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .clear();
        app.update();
        assert_eq!(app.world().get::<Node>(tabs).unwrap().max_height, px(40));
    }

    #[cfg_attr(test, test)]
    fn workspace_name_saves_from_its_row_and_pointer_focus_has_no_outline() {
        let (mut app, root) = fixture();
        activate(&mut app, root, EditAction::Toggle);
        let mode = app.world().get::<EditMode>(root).unwrap();
        let field = mode.name.unwrap();
        let toggle = mode.toggle;
        let panel = mode.panel;
        assert_ne!(app.world().get::<ChildOf>(field).unwrap().parent(), panel);
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("Notes");
        app.update();
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries[0].name,
            "Notes"
        );
        app.world_mut().trigger(FocusGained {
            entity: toggle,
            cause: FocusCause::Pressed,
        });
        assert_eq!(app.world().get::<Outline>(toggle).unwrap().width, px(0));
    }

    #[cfg_attr(test, test)]
    fn edit_button_emits_one_scoped_event_and_close_restores_focus() {
        let (mut app, root) = fixture();
        let draws = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let requested = draws.clone();
        app.insert_resource(crate::wake::WakeSignal::new(move || {
            requested.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
        let other = app.world_mut().spawn(BoxRoot).id();
        app.update();
        activate(&mut app, root, EditAction::Toggle);
        assert!(draws.load(std::sync::atomic::Ordering::Relaxed) > 0);
        assert!(app.world().get::<EditMode>(root).unwrap().enabled);
        assert!(!app.world().get::<EditMode>(other).unwrap().enabled);
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
        crate::actions::dispatch(app.world_mut(), root, crate::actions![EditAction::Close]);
        app.update();
        let mode = app.world().get::<EditMode>(root).unwrap();
        assert!(!mode.enabled);
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(mode.toggle)
        );
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().resource::<Toggles>().0,
            vec![(root, true), (root, false)]
        );
        apply(app.world_mut(), root, EditAction::CreateWorkspace);
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries.len(),
            1
        );
    }

    #[cfg_attr(test, test)]
    fn canvas_colors_save_valid_fields_and_stay_with_their_workspace() {
        let (mut app, root) = fixture();
        activate(&mut app, root, EditAction::Toggle);
        activate(&mut app, root, EditAction::Canvas);
        for (field, value) in [
            (ColorField::Background, "invalid"),
            (ColorField::Grid, "#ABC"),
        ] {
            let entity = app
                .world_mut()
                .query::<(Entity, &ColorField)>()
                .iter(app.world())
                .find(|(_, candidate)| **candidate == field)
                .unwrap()
                .0;
            app.world_mut()
                .get_mut::<EditableText>(entity)
                .unwrap()
                .editor
                .set_text(value);
        }
        app.update();
        assert_eq!(
            crate::canvas_background::current(app.world(), root),
            crate::canvas_background::CanvasColors {
                grid: [170, 187, 204],
                ..default()
            }
        );
        let field = app
            .world_mut()
            .query::<(Entity, &ColorField)>()
            .iter(app.world())
            .find(|(_, candidate)| **candidate == ColorField::Background)
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("#102030");
        app.update();
        let colors = crate::canvas_background::CanvasColors {
            background: [16, 32, 48],
            grid: [170, 187, 204],
        };
        assert_eq!(crate::canvas_background::current(app.world(), root), colors);
        activate(&mut app, root, EditAction::Workspaces);
        activate(&mut app, root, EditAction::CreateWorkspace);
        assert_eq!(
            crate::canvas_background::current(app.world(), root),
            Default::default()
        );
        activate(&mut app, root, EditAction::SwitchWorkspace(1));
        assert_eq!(crate::canvas_background::current(app.world(), root), colors);
        activate(&mut app, root, EditAction::Canvas);
        activate(&mut app, root, EditAction::ResetCanvasColors);
        assert_eq!(
            crate::canvas_background::current(app.world(), root),
            Default::default()
        );
    }

    #[cfg_attr(test, test)]
    fn text_properties_save_while_typing_without_replacing_the_focused_editor() {
        let (mut app, root) = fixture();
        activate(&mut app, root, EditAction::Toggle);
        activate(&mut app, root, EditAction::Store);
        activate(&mut app, root, EditAction::AddSand(SandKind::Text));
        let sand = app
            .world_mut()
            .query_filtered::<Entity, With<StoredSand>>()
            .single(app.world())
            .unwrap();
        activate(&mut app, root, EditAction::EditSand(sand));
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        let field = app
            .world()
            .get::<crate::sand_text_editor::TextPanel>(panel)
            .unwrap()
            .fields[0];
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        for value in ["First edit", "Second edit"] {
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            let selected = app
                .world()
                .get::<StoredSand>(sand)
                .unwrap()
                .content
                .unwrap();
            assert_eq!(crate::sand_text::value(app.world(), selected), value);
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        }
        let width = app
            .world()
            .get::<crate::sand_text_editor::TextPanel>(panel)
            .unwrap()
            .fields[3];
        let selected = app
            .world()
            .get::<StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        let previous = app
            .world()
            .get::<crate::sand_text::SandText>(selected)
            .unwrap()
            .size;
        app.world_mut()
            .get_mut::<EditableText>(width)
            .unwrap()
            .editor
            .set_text("-");
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::sand_text::SandText>(selected)
                .unwrap()
                .size,
            previous
        );
        assert_eq!(
            app.world()
                .get::<EditableText>(width)
                .unwrap()
                .value()
                .to_string(),
            "-"
        );
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("Text still saves");
        app.update();
        assert_eq!(
            crate::sand_text::value(app.world(), selected),
            "Text still saves"
        );
        app.world_mut()
            .get_mut::<EditableText>(width)
            .unwrap()
            .editor
            .set_text("320");
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::sand_text::SandText>(selected)
                .unwrap()
                .size[0],
            320.0
        );
    }

    #[cfg_attr(test, test)]
    fn workspace_and_store_controls_work_and_preserve_created_content() {
        let (mut app, root) = fixture();
        activate(&mut app, root, EditAction::Toggle);
        activate(&mut app, root, EditAction::CreateWorkspace);
        let name = app.world().get::<EditMode>(root).unwrap().name.unwrap();
        app.world_mut()
            .get_mut::<EditableText>(name)
            .unwrap()
            .editor
            .set_text("Drawing");
        app.update();
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries[1].name,
            "Drawing"
        );
        activate(&mut app, root, EditAction::Store);
        let content = app.world().get::<EditMode>(root).unwrap().content.unwrap();
        app.world_mut()
            .get_mut::<EditableText>(content)
            .unwrap()
            .editor
            .set_text("My note");
        activate(&mut app, root, EditAction::AddSand(SandKind::EditableText));
        let (sand, state, member) = app
            .world_mut()
            .query::<(Entity, &StoredSand, &WorkspaceMember)>()
            .single(app.world())
            .unwrap();
        let editor = state.content.unwrap();
        assert_eq!(member.0, 2);
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "My note"
        );
        assert!(
            app.world()
                .get::<crate::sand_store::SandCredits>(sand)
                .unwrap()
                .0
                .iter()
                .all(|credit| !credit.license.is_empty())
        );
        activate(&mut app, root, EditAction::Workspaces);
        activate(&mut app, root, EditAction::RemoveWorkspace(2));
        assert_eq!(
            app.world().get::<Workspaces>(root).unwrap().entries.len(),
            2
        );
        activate(&mut app, root, EditAction::CancelRemoveWorkspace);
        activate(&mut app, root, EditAction::RemoveWorkspace(2));
        activate(&mut app, root, EditAction::ConfirmRemoveWorkspace);
        assert_eq!(app.world().get::<WorkspaceMember>(sand).unwrap().0, 1);
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "My note"
        );
        activate(&mut app, root, EditAction::Store);
        activate(&mut app, root, EditAction::RemoveSand(sand));
        assert!(app.world().get_entity(editor).is_err());
    }

    #[cfg_attr(test, test)]
    fn workspace_rows_keep_delete_icons_with_their_workspace() {
        let (mut app, root) = fixture();
        activate(&mut app, root, EditAction::Toggle);
        activate(&mut app, root, EditAction::CreateWorkspace);
        let deletes: Vec<_> = app
            .world_mut()
            .query::<(Entity, &EditControl, &ChildOf)>()
            .iter(app.world())
            .filter_map(|(entity, control, parent)| match control.action {
                EditAction::RemoveWorkspace(workspace) => {
                    Some((entity, workspace, parent.parent()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(deletes.len(), 2);
        for (delete, _, row) in &deletes {
            assert_eq!(
                app.world().get::<Children>(*row).unwrap().last(),
                Some(delete)
            );
            assert_eq!(
                app.world()
                    .get::<crate::icons::IconButton>(*delete)
                    .unwrap()
                    .icon,
                crate::icons::Icon::Close
            );
        }
        let delete = deletes
            .iter()
            .find(|(_, workspace, _)| *workspace == 1)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: delete });
        app.update();
        assert_eq!(
            app.world().get::<EditMode>(root).unwrap().confirm_remove,
            Some(1)
        );
        activate(&mut app, root, EditAction::ConfirmRemoveWorkspace);
        let spaces = app.world().get::<Workspaces>(root).unwrap();
        assert_eq!(spaces.active, 2);
        assert_eq!(spaces.entries.len(), 1);
        assert_eq!(spaces.entries[0].id, 2);
    }

    crate::laboratory_cases! {
        general_settings_are_separate_and_tab_entities_survive_navigation,
        workspace_name_saves_from_its_row_and_pointer_focus_has_no_outline,
        edit_button_emits_one_scoped_event_and_close_restores_focus,
        canvas_colors_save_valid_fields_and_stay_with_their_workspace,
        text_properties_save_while_typing_without_replacing_the_focused_editor,
        workspace_and_store_controls_work_and_preserve_created_content,
        workspace_rows_keep_delete_icons_with_their_workspace,
    }
}
