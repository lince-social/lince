use crate::i18n::Token as Tr;
use crate::native::{
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

pub use crate::native::canvas_colors::ColorField;
pub use crate::native::sand_text_editor::TextAction;

mod workspaces;

#[derive(Component)]
pub(crate) struct FocusScroll;

#[derive(Component)]
pub(crate) struct FocusScrollGroup;

#[derive(Component)]
struct FocusScrollRevision(u64);

#[derive(EntityEvent, Clone, Copy)]
pub struct EditPopupToggle {
    pub entity: Entity,
    pub enabled: bool,
}

#[derive(Component)]
pub struct EditMode {
    pub enabled: bool,
    pub(crate) revision: u64,
    pub panel: Entity,
    pub toggle: Entity,
    name: Option<Entity>,
    confirm_remove: Option<u64>,
    general: bool,
    credits: bool,
    store: bool,
    canvas: bool,
    information: bool,
    shortcuts: bool,
    park: bool,
    customization: bool,
    pub(crate) areas: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Open,
    Toggle,
    Close,
    CreateWorkspace,
    TogglePhysics,
    ToggleAlwaysShowControls,
    ReloadWorkspaceSettings,
    SwitchWorkspace(u64),
    RemoveWorkspace(u64),
    RestoreWorkspace(u64),
    SetTrashRetention(u16),
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
    Information,
    Shortcuts,
    Park,
    Customization,
    Areas,
    Area(crate::native::area_panel::AreaAction),
    ResetCanvasColors,
    CanvasColor(ColorField, [u8; 3]),
}

impl Action for EditAction {
    fn practice_intent(&self) -> crate::native::actions::PracticeIntent {
        use crate::native::actions::PracticeIntent;
        use lince_interface::practice::Operation;
        match self {
            Self::Open | Self::Toggle => PracticeIntent::Navigation,
            Self::AddSand(SandKind::Square | SandKind::Text) => {
                PracticeIntent::Feature(Operation::PlaceSand)
            }
            Self::EditSand(entity) => PracticeIntent::Object(*entity),
            Self::Area(crate::native::area_panel::AreaAction::Select(entity)) => {
                PracticeIntent::Object(*entity)
            }
            Self::Area(
                crate::native::area_panel::AreaAction::Add(_)
                | crate::native::area_panel::AreaAction::Draw(_),
            ) => PracticeIntent::Feature(Operation::PlaceArea),
            Self::Area(_) => PracticeIntent::SelectedArea,
            Self::Text(_) => PracticeIntent::EditedSand,
            Self::Close
            | Self::Workspaces
            | Self::SwitchWorkspace(_)
            | Self::Store
            | Self::Canvas
            | Self::Areas => PracticeIntent::Navigation,
            _ => PracticeIntent::Target,
        }
    }

    fn connections(
        &self,
        world: &World,
        root: Entity,
    ) -> Vec<crate::native::inspection::Connection> {
        crate::native::inspection::edit_connection(world, root, *self)
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
}

#[derive(Component)]
struct WorkspaceName(u64);

#[derive(Component)]
struct StoreFilter {
    input: Entity,
    groups: Vec<StoreFilterGroup>,
    observed: String,
}

struct StoreFilterGroup {
    heading: Entity,
    entries: Vec<Entity>,
}

pub struct EditModePlugin;
impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        crate::native::layout::desktop::input_settings::install(app);
        crate::native::layout::desktop::magnification::install(app);
        crate::native::layout::desktop::pointer_settings::install(app);
        crate::native::layout::desktop::absolute_settings::install(app);
        crate::native::layout::desktop::focus::settings::install(app);
        crate::native::layout::desktop::opening::install(app);
        crate::native::layout::desktop::control::install(app);
        crate::native::layout::desktop::gestures::install(app);
        crate::native::layout::desktop::overview::settings::install(app);
        crate::native::layout::desktop::external_settings::install(app);
        app.add_plugins(crate::native::custom_castle::LibraryPlugin);
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        if !app.is_plugin_added::<crate::native::operation::OperationPlugin>() {
            app.add_plugins(crate::native::operation::OperationPlugin);
        }
        if !app.is_plugin_added::<crate::native::deletion::DeletionPlugin>() {
            app.add_plugins(crate::native::deletion::DeletionPlugin);
        }
        if !app.is_plugin_added::<crate::native::color_picker::ColorPickerPlugin>() {
            app.add_plugins(crate::native::color_picker::ColorPickerPlugin);
        }
        if !app.is_plugin_added::<crate::native::shortcuts::EditorPlugin>() {
            app.add_plugins(crate::native::shortcuts::EditorPlugin);
        }
        app.init_resource::<crate::native::tokens::ThemeSettings>()
            .init_resource::<crate::native::shortcuts::Settings>()
            .add_observer(crate::native::customization::toggle)
            .add_systems(
                PreUpdate,
                crate::native::sand_settings::select.after(crate::native::inspection::InspectInput),
            )
            .add_systems(
                Update,
                (
                    setup
                        .after(PrepareWorkspaces)
                        .after(crate::native::canvas_controls::create_controls),
                    expand_tabs,
                    crate::native::sand_store::refresh.after(setup),
                    crate::native::shortcuts::synchronize.after(setup),
                ),
            )
            .add_systems(
                PostUpdate,
                (reveal_focus, anchor_panel).after(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                anchor_close
                    .after(crate::native::icons::SyncIcons)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (
                    autosave_name,
                    filter_store,
                    crate::native::customization::autosave,
                    crate::native::canvas_colors::autosave,
                    crate::native::sand_text_editor::autosave,
                    crate::native::canvas_colors::preview,
                )
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions),
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

#[derive(Component)]
struct EditClose;

fn expand_tabs(
    settings: Res<crate::native::tokens::ThemeSettings>,
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
            px(number(crate::native::tokens::Token::IconSize)
                + 2.0
                    * (number(crate::native::tokens::Token::IconPadding)
                        + number(crate::native::tokens::Token::ControlBorder)))
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
            let field = mode.name?;
            let workspace = world.get::<WorkspaceName>(field)?.0;
            let text = world.get::<EditableText>(field)?;
            if text.is_composing() || text.pending_paste.is_some() {
                return None;
            }
            let name = text.value().to_string();
            let spaces = world.get::<Workspaces>(root)?;
            let current = spaces.entries.iter().find(|space| space.id == workspace)?;
            (!name.trim().is_empty() && name.trim() != current.name)
                .then_some((root, workspace, name))
        })
        .collect();
    for (root, workspace, name) in edits {
        if !workspace::rename_id(world, root, workspace, &name) {
            continue;
        }
        let labels: Vec<_> = world
            .query::<(&EditControl, &Children)>()
            .iter(world)
            .filter(|(control, _)| {
                control.root == root && control.action == EditAction::SwitchWorkspace(workspace)
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

fn descendant_text(world: &World, entity: Entity, value: &mut String) {
    if let Some(text) = world.get::<Text>(entity) {
        value.push_str(&text.0);
        value.push(' ');
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children {
            descendant_text(world, *child, value);
        }
    }
}

fn filter_store(world: &mut World) {
    let filters: Vec<_> = world
        .query::<(Entity, &StoreFilter)>()
        .iter(world)
        .filter_map(|(panel, filter)| {
            let input = world.get::<EditableText>(filter.input)?.value().to_string();
            let value = input.trim().to_lowercase();
            (value != filter.observed).then(|| {
                (
                    panel,
                    value,
                    filter
                        .groups
                        .iter()
                        .map(|group| (group.heading, group.entries.clone()))
                        .collect::<Vec<_>>(),
                )
            })
        })
        .collect();
    for (panel, filter, groups) in filters {
        for (heading, entries) in groups {
            let mut visible = false;
            for entry in entries {
                let mut value = String::new();
                descendant_text(world, entry, &mut value);
                let matches = filter.is_empty() || value.to_lowercase().contains(&filter);
                if let Some(mut node) = world.get_mut::<Node>(entry) {
                    node.display = if matches {
                        Display::Flex
                    } else {
                        Display::None
                    };
                }
                visible |= matches;
            }
            if let Some(mut node) = world.get_mut::<Node>(heading) {
                node.display = if visible {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
        world.get_mut::<StoreFilter>(panel).unwrap().observed = filter;
    }
}

fn reveal_focus(
    focus: Res<InputFocus>,
    mut previous: Local<(Option<Entity>, Vec<(Entity, Vec2, u64)>)>,
    parents: Query<&ChildOf>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    modes: Query<(Entity, &EditMode)>,
    inspectors: Query<(Entity, &crate::native::topology::ui::Inspector)>,
    modal_lists: Query<Entity, With<FocusScroll>>,
    groups: Query<(), With<FocusScrollGroup>>,
    revisions: Query<&FocusScrollRevision>,
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    mut scrolls: Query<&mut ScrollPosition>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
) {
    if buttons
        .as_deref()
        .is_some_and(|buttons| buttons.get_pressed().next().is_some())
    {
        return;
    }
    let panels: Vec<_> = modes
        .iter()
        .filter(|(_, mode)| mode.enabled)
        .map(|(_, mode)| mode.panel)
        .chain(
            inspectors
                .iter()
                .filter(|(_, inspector)| modes.get(inspector.0).is_ok_and(|(_, mode)| mode.enabled))
                .map(|(entity, _)| entity),
        )
        .chain(modal_lists.iter())
        .collect();
    let mut ancestors = Vec::new();
    let mut ancestor = focus.get();
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(current) = ancestor else { break };
        if ancestors.contains(&current) {
            break;
        }
        ancestors.push(current);
        ancestor = parents.get(current).ok().map(ChildOf::parent);
    }
    let mut sizes: Vec<_> = panels
        .iter()
        .filter(|entity| ancestors.contains(entity))
        .filter_map(|entity| {
            geometry.get(*entity).ok().map(|(node, _)| {
                (
                    *entity,
                    node.size(),
                    revisions.get(*entity).map_or(0, |revision| revision.0),
                )
            })
        })
        .collect();
    sizes.sort_unstable_by_key(|entry| entry.0);
    if previous.0 == focus.get() && previous.1 == sizes {
        return;
    }
    *previous = (focus.get(), sizes);
    let Some(entity) = focus.get() else {
        return;
    };
    let Ok((node, transform)) = geometry.get(entity) else {
        return;
    };
    let mut grouped = None;
    let mut ancestor = Some(entity);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(current) = ancestor else { break };
        if groups.contains(current) {
            grouped = geometry.get(current).ok();
            break;
        }
        ancestor = parents.get(current).ok().map(ChildOf::parent);
    }
    for panel_entity in panels {
        let mut ancestor = entity;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if ancestor != panel_entity {
                continue;
            }
            let Ok((panel, panel_transform)) = geometry.get(panel_entity) else {
                break;
            };
            let (node, transform) = grouped
                .filter(|(group, _)| group.size().y <= (panel.size().y - 16.0).max(0.0))
                .unwrap_or((node, transform));
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
                && let Ok(mut scroll) = scrolls.get_mut(panel_entity)
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

fn anchor_close(mut nodes: Query<&mut Node>, closes: Query<Entity, With<EditClose>>) {
    for close in &closes {
        let top = px(0);
        if let Ok(mut node) = nodes.get_mut(close)
            && (node.position_type != PositionType::Absolute
                || node.right != px(0)
                || node.top != top)
        {
            node.position_type = PositionType::Absolute;
            node.right = px(0);
            node.top = top;
        }
    }
}

fn panel_horizontal(root: f32, width: f32, anchor: f32, usable: [f32; 2]) -> (f32, f32) {
    let left = usable[0].clamp(0.0, root);
    let end = usable[1].clamp(left, root);
    let (left, end) = if end > left { (left, end) } else { (0.0, root) };
    let margin = 12.0_f32.min((end - left) * 0.03);
    let maximum = (end - left - margin * 2.0).max(1.0);
    let width = width.clamp(1.0, maximum);
    let low = root - end + margin;
    let high = (root - left - margin - width).max(low);
    (anchor.clamp(low, high), maximum)
}

fn anchor_panel(
    modes: Query<(Entity, &EditMode)>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    viewports: Query<&crate::native::layout::desktop::HostViewport>,
    mut nodes: Query<&mut Node>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
) {
    for (root, mode) in &modes {
        let Ok((root_node, root_transform)) = geometry.get(root) else {
            continue;
        };
        let Ok((button, transform)) = geometry.get(mode.toggle) else {
            continue;
        };
        if !root_node.size().is_finite()
            || !button.size().is_finite()
            || root_node.size().min_element() <= 0.0
            || button.size().min_element() <= 0.0
        {
            continue;
        }
        let scale = root_node.inverse_scale_factor();
        let position = transform.translation - root_transform.translation + root_node.size() * 0.5;
        let anchor = ((root_node.size().x - position.x - button.size().x * 0.5) * scale).max(12.0);
        let bottom = px((root_node.size().y - position.y + button.size().y * 0.5) * scale + 8.0);
        if let Ok(mut node) = nodes.get_mut(mode.panel) {
            let root_width = root_node.size().x * scale;
            let usable = viewports
                .get(root)
                .ok()
                .and_then(|viewport| {
                    viewport.0.usable.map(|usable| {
                        [
                            (usable.origin()[0] - viewport.0.bounds.origin()[0]) as f32,
                            (usable.origin()[0] + usable.size()[0] - viewport.0.bounds.origin()[0])
                                as f32,
                        ]
                    })
                })
                .unwrap_or([0.0, root_width]);
            let width = geometry
                .get(mode.panel)
                .ok()
                .map(|(panel, _)| panel.unrounded_size().x * scale)
                .filter(|width| *width > 0.0)
                .unwrap_or_else(|| {
                    if let Val::Px(width) = node.width {
                        width
                    } else {
                        root_width
                    }
                });
            let (right, maximum) = panel_horizontal(root_width, width, anchor, usable);
            if node.right != px(right) || node.bottom != bottom || node.max_width != px(maximum) {
                node.right = px(right);
                node.bottom = bottom;
                node.max_width = px(maximum);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            }
        }
    }
}

pub(crate) fn setup(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, (
            With<Workspaces>,
            Without<EditMode>,
            Without<crate::native::laboratory::LaboratoryRoot>,
        )>()
        .iter(world)
        .collect();
    for root in roots {
        if world
            .get::<crate::native::inspection::Inspection>(root)
            .is_none()
        {
            world
                .entity_mut(root)
                .insert(crate::native::inspection::Inspection::default());
        }
        let toolbar = crate::native::canvas_controls::toolbar(world, root);
        let toggle = control(
            world,
            root,
            toolbar,
            EditAction::Toggle,
            Tr::EditMode.text(),
        );
        world.entity_mut(toggle).insert((
            crate::native::canvas_controls::ToolbarOrder(0),
            Node {
                width: px(120),
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(px(12), px(8)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
        ));
        let panel = world
            .spawn((
                crate::native::inspection::InspectionExcluded,
                crate::native::sand::Square,
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
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
                GlobalZIndex(21),
                ScrollPosition::default(),
                TabGroup::modal(),
                ChildOf(root),
            ))
            .observe(|mut event: On<Pointer<Scroll>>| event.propagate(false))
            .id();
        world.entity_mut(root).insert(EditMode {
            enabled: false,
            revision: 0,
            panel,
            toggle,
            name: None,
            confirm_remove: None,
            general: false,
            credits: false,
            store: false,
            canvas: false,
            information: false,
            shortcuts: false,
            park: false,
            customization: false,
            areas: false,
        });
        let bindings = [KeyBinding::new(
            KeyCode::Escape,
            Modifiers::NONE,
            crate::native::actions![crate::native::inspection::Deselect],
        )];
        if let Some(mut existing) = world.get_mut::<KeyBindings>(root) {
            existing.0.splice(0..0, bindings);
        } else {
            world.entity_mut(root).insert(KeyBindings(bindings.into()));
        }
        crate::native::shortcuts::install(world, root);
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

fn apply(world: &mut World, root: Entity, action: EditAction) {
    let Some(mode) = world.get::<EditMode>(root) else {
        return;
    };
    if let EditAction::Area(crate::native::area_panel::AreaAction::Select(entity)) = action
        && mode.areas
        && world
            .get::<crate::native::area_panel::AreaEditor>(root)
            .is_some_and(|editor| editor.selected == Some(entity) && editor.tool.is_none())
    {
        if let Some(mut inspection) = world.get_mut::<crate::native::inspection::Inspection>(root) {
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
    let active_tab = [
        (mode.park, Some(EditAction::Park)),
        (mode.shortcuts, Some(EditAction::Shortcuts)),
        (mode.information, Some(EditAction::Information)),
        (mode.general, Some(EditAction::General)),
        (mode.areas, Some(EditAction::Areas)),
        (mode.customization, Some(EditAction::Customization)),
        (mode.canvas, Some(EditAction::Canvas)),
        (
            world
                .get::<crate::native::sand_text_editor::TextPanel>(mode.panel)
                .is_some(),
            None,
        ),
        (mode.credits, Some(EditAction::Credits)),
        (mode.store, Some(EditAction::Store)),
        (true, Some(EditAction::Workspaces)),
    ]
    .into_iter()
    .find(|(active, _)| *active)
    .and_then(|(_, tab)| tab);
    if active_tab == Some(action) {
        return;
    }
    if matches!(
        action,
        EditAction::General
            | EditAction::Workspaces
            | EditAction::Store
            | EditAction::Canvas
            | EditAction::Information
            | EditAction::Shortcuts
            | EditAction::Park
            | EditAction::Customization
            | EditAction::Credits
            | EditAction::EditSand(_)
            | EditAction::SwitchWorkspace(_)
            | EditAction::CreateWorkspace
            | EditAction::ConfirmRemoveWorkspace
    ) {
        world.get_mut::<EditMode>(root).unwrap().areas = false;
        if let Some(mut editor) = world.get_mut::<crate::native::area_panel::AreaEditor>(root) {
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
            | EditAction::Information
            | EditAction::Shortcuts
            | EditAction::Park
            | EditAction::Customization
            | EditAction::Credits
            | EditAction::EditSand(_)
    ) {
        world.get_mut::<EditMode>(root).unwrap().general = action == EditAction::General;
        world.get_mut::<EditMode>(root).unwrap().information = action == EditAction::Information;
        world.get_mut::<EditMode>(root).unwrap().shortcuts = action == EditAction::Shortcuts;
        world.get_mut::<EditMode>(root).unwrap().park = action == EditAction::Park;
    }
    let mode = world.get::<EditMode>(root).unwrap();
    match action {
        EditAction::ToggleAlwaysShowControls => {
            world.init_resource::<crate::native::canvas_controls::ControlsSettings>();
            let mut settings =
                world.resource_mut::<crate::native::canvas_controls::ControlsSettings>();
            settings.always_show = !settings.always_show;
        }
        EditAction::TogglePhysics => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            let enabled = crate::native::workspace_config::physics_configured(world, root, active);
            crate::native::workspace_config::set_physics(world, root, active, !enabled);
        }
        EditAction::ReloadWorkspaceSettings => {
            let active = world.get::<Workspaces>(root).unwrap().active;
            crate::native::workspace_config::reload(world, root, active);
        }
        EditAction::Areas | EditAction::Area(_) => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.areas = true;
            mode.customization = false;
            mode.canvas = false;
            mode.credits = false;
            if let EditAction::Area(action) = action {
                crate::native::area_panel::apply(world, root, action);
            }
        }
        EditAction::Close => {
            set_open(world, root, false);
            return;
        }
        EditAction::CreateWorkspace => {
            if let Some(requests) =
                world.get_resource::<crate::native::layout::desktop::HostRequests>()
            {
                if requests
                    .0
                    .try_send(crate::host::HostRequest::CreateWorkspace {
                        owner: root.to_bits(),
                    })
                    .is_err()
                {
                    crate::native::notifications::report(
                        world,
                        "Park",
                        "Park is busy; try creating the workspace again.",
                    );
                }
            } else {
                workspace::create(world, root);
            }
        }
        EditAction::SwitchWorkspace(id) => {
            if let Some(requests) =
                world.get_resource::<crate::native::layout::desktop::HostRequests>()
            {
                if requests
                    .0
                    .try_send(crate::host::HostRequest::Workspace {
                        owner: root.to_bits(),
                        id,
                    })
                    .is_err()
                {
                    crate::native::notifications::report(
                        world,
                        "Park",
                        "Park is busy; try switching the workspace again.",
                    );
                }
            } else {
                workspace::switch(world, root, id);
            }
        }
        EditAction::SetTrashRetention(days) => {
            if (1..=365).contains(&days) {
                world
                    .get_mut::<Workspaces>(root)
                    .unwrap()
                    .trash_retention_days = days;
            }
        }
        EditAction::RestoreWorkspace(workspace) => {
            workspace::restore_trash(world, root, workspace);
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
            place_sand(world, root, kind);
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
                crate::native::canvas_item::DeleteItem.apply(world, sand);
            }
        }
        EditAction::EditSand(sand) => crate::native::sand_text_editor::open(world, root, sand),
        EditAction::Text(action) => {
            if !crate::native::sand_text_editor::apply(world, root, action) {
                return;
            }
        }
        EditAction::Credits => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.credits = true;
            mode.general = false;
            mode.areas = false;
            mode.customization = false;
            mode.canvas = false;
        }
        EditAction::Customization => {
            world.trigger(
                crate::native::customization::GlobalCustomizationPanelToggle { entity: root },
            );
            return;
        }
        EditAction::General | EditAction::Workspaces => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.store = false;
            mode.canvas = false;
            mode.credits = false;
        }
        EditAction::Store => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.store = true;
            mode.canvas = false;
            mode.credits = false;
        }
        EditAction::Canvas => {
            let mut mode = world.get_mut::<EditMode>(root).unwrap();
            mode.customization = false;
            mode.canvas = true;
            mode.credits = false;
        }
        EditAction::Information | EditAction::Shortcuts | EditAction::Park => {}
        EditAction::ResetCanvasColors => {
            if !mode.canvas || !crate::native::canvas_colors::save(world, root, true) {
                return;
            }
        }
        EditAction::CanvasColor(field, rgb) => {
            if mode.canvas {
                crate::native::canvas_colors::preset(world, root, field, rgb);
            }
            return;
        }
        EditAction::Open | EditAction::Toggle => {}
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
            | EditAction::Information
            | EditAction::Shortcuts
            | EditAction::Park
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
            .remove::<crate::native::sand_text_editor::TextPanel>();
    }
    let scroll = if matches!(
        action,
        EditAction::Area(
            crate::native::area_panel::AreaAction::TargetCenter
                | crate::native::area_panel::AreaAction::TargetPoint
        )
    ) {
        world
            .get::<ScrollPosition>(world.get::<EditMode>(root).unwrap().panel)
            .map(|scroll| scroll.0)
    } else {
        None
    };
    render_panel(world, root);
    if let Some(scroll) = scroll {
        let panel = world.get::<EditMode>(root).unwrap().panel;
        world.entity_mut(panel).insert(ScrollPosition(scroll));
    }
}

pub(crate) fn place_sand(world: &mut World, root: Entity, kind: SandKind) -> Option<Entity> {
    let active = world.get::<Workspaces>(root)?.active;
    let center = world.get::<CanvasView>(root)?.center;
    if !center.is_finite() {
        return None;
    }
    let initial = if kind == SandKind::Square {
        ""
    } else {
        kind.name()
    };
    let entity = spawn_sand(world, root, active, kind, initial, center);
    crate::native::instinct::practice::track_sand(world, root, entity, kind);
    Some(entity)
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
    mode.shortcuts = false;
    mode.park = false;
    mode.areas = false;
    mode.canvas = false;
    mode.credits = false;
    let panel = mode.panel;
    world
        .entity_mut(panel)
        .remove::<crate::native::sand_text_editor::TextPanel>();
    render_panel(world, root);
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) fn customizing(world: &World, root: Entity) -> bool {
    world
        .get::<EditMode>(root)
        .is_some_and(|mode| mode.enabled && mode.customization)
}

pub(crate) fn show_customization(world: &mut World, root: Entity) {
    if !customizing(world, root) {
        toggle_customization(world, root);
    } else {
        render_panel(world, root);
    }
}

fn set_open(world: &mut World, root: Entity, enabled: bool) {
    if !enabled {
        crate::native::protein_area::presentation::cancel(world, root);
    }
    let Some(mut mode) = world.get_mut::<EditMode>(root) else {
        return;
    };
    if mode.enabled == enabled {
        return;
    }
    mode.enabled = enabled;
    mode.revision = mode.revision.wrapping_add(1);
    mode.confirm_remove = None;
    mode.credits = false;
    let panel = mode.panel;
    let toggle = mode.toggle;
    if enabled {
        crate::native::notifications::close(world, root);
    }
    if !enabled
        && let Some(mut editor) = world.get_mut::<crate::native::area_panel::AreaEditor>(root)
    {
        editor.cancel();
    }
    world.get_mut::<Node>(panel).unwrap().display = if enabled {
        Display::Flex
    } else {
        Display::None
    };
    let label = if enabled {
        Tr::DoneEditing.text()
    } else {
        Tr::EditMode.text()
    };
    world
        .get_mut::<AccessibilityNode>(toggle)
        .unwrap()
        .set_label(label);
    world
        .get_mut::<crate::native::icons::IconButton>(toggle)
        .unwrap()
        .label = label.into();
    if enabled {
        render_panel(world, root);
        let first = world.query::<(Entity, &EditControl)>().iter(world)
            .find_map(|(entity, control)| (control.root == root && control.action == EditAction::General).then_some(entity));
        if let Some(first) = first {
            world.resource_mut::<InputFocus>().set(first, FocusCause::Navigated);
        }
    } else {
        world
            .resource_mut::<InputFocus>()
            .set(toggle, FocusCause::Pressed);
    }
    world.trigger(EditPopupToggle {
        entity: root,
        enabled,
    });
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) fn label(world: &mut World, parent: Entity, value: &str, size: f32) -> Entity {
    let font = world.resource::<Typography>().text(size);
    world
        .spawn((
            Text::new(value),
            font,
            crate::native::token_style::text(crate::native::tokens::Token::Ink),
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
            ActionButton::new(root, crate::native::actions![action]),
            button(0),
            Node {
                padding: UiRect::axes(px(10), px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::token_style::OutlineToken(crate::native::tokens::Token::Accent),
            Outline {
                width: px(0),
                offset: px(2),
                color: PURPLE,
            },
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_label(value);
    use crate::native::icons::Icon;
    let icon = match action {
        EditAction::Open | EditAction::Toggle | EditAction::EditSand(_) => Some(Icon::Paintbrush),
        EditAction::Close
        | EditAction::RemoveWorkspace(_)
        | EditAction::RemoveSand(_)
        | EditAction::CancelRemoveWorkspace
        | EditAction::Text(TextAction::Remove) => Some(Icon::Close),
        EditAction::CreateWorkspace => Some(Icon::Plus),
        EditAction::ConfirmRemoveWorkspace => Some(Icon::Check),
        EditAction::RestoreWorkspace(_) => Some(Icon::Reset),
        EditAction::SetTrashRetention(_) => None,
        EditAction::Credits => Some(Icon::Credits),
        EditAction::Information | EditAction::Shortcuts => Some(Icon::Info),
        EditAction::Park => Some(Icon::General),
        EditAction::General => Some(Icon::General),
        EditAction::Workspaces => Some(Icon::Workspaces),
        EditAction::Store => Some(Icon::Store),
        EditAction::Canvas | EditAction::Customization => Some(Icon::Palette),
        EditAction::Areas => Some(Icon::Circle),
        EditAction::Area(action) => {
            use crate::native::area::{Direction, ShapeKind};
            use crate::native::area_panel::AreaAction;
            match action {
                AreaAction::Add(kind) | AreaAction::Draw(kind) | AreaAction::Shape(kind) => {
                    Some(match kind {
                        ShapeKind::Square => Icon::Square,
                        ShapeKind::Circle => Icon::Circle,
                        ShapeKind::Drawn => Icon::Pencil,
                    })
                }
                AreaAction::Redraw => Some(Icon::Pencil),
                AreaAction::TargetCenter => Some(Icon::Reset),
                AreaAction::TargetPoint => Some(Icon::Recenter),
                AreaAction::Direction(Direction::Attract) => Some(Icon::Attract),
                AreaAction::Direction(Direction::Repel) => Some(Icon::Repel),
                AreaAction::AddRule => Some(Icon::Plus),
                AreaAction::RemoveRule(_) | AreaAction::Remove => Some(Icon::Delete),
                AreaAction::Finish | AreaAction::ArmChanges => Some(Icon::Check),
                AreaAction::Cancel | AreaAction::DisarmChanges => Some(Icon::Close),
                AreaAction::PreviewChanges => Some(Icon::Info),
                _ => None,
            }
        }
        EditAction::ReloadWorkspaceSettings => Some(Icon::Reset),
        EditAction::TogglePhysics | EditAction::ToggleAlwaysShowControls => None,
        EditAction::ResetCanvasColors => Some(Icon::Reset),
        EditAction::CanvasColor(_, rgb) => {
            world
                .entity_mut(entity)
                .insert(crate::native::icons::IconStyle {
                    color: crate::native::canvas_background::color(rgb),
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
            crate::native::sand_text::TextOverflow::Scroll => Icon::Scroll,
            crate::native::sand_text::TextOverflow::Grow => Icon::Grow,
        }),
        EditAction::AddSand(kind) => Some(match kind {
            SandKind::Square => Icon::Square,
            SandKind::Text => Icon::Text,
            SandKind::EditableText => Icon::EditableText,
            SandKind::Operation => Icon::Forward,
            SandKind::WorkTimer => Icon::Play,
            SandKind::AccessControl => Icon::Person,
            SandKind::Visibility => Icon::Person,
            SandKind::Trust => Icon::Person,
            SandKind::HandCamera => Icon::Person,
            SandKind::Video => Icon::Play,
            SandKind::Sync => Icon::Reset,
            #[cfg(feature = "stuff")]
            SandKind::Freedoom => Icon::Play,
            SandKind::Terminal => Icon::Forward,
            SandKind::Configuration | SandKind::Organ | SandKind::Ontology => Icon::General,
            SandKind::Todo => Icon::Check,
        }),
        EditAction::SwitchWorkspace(_) | EditAction::Text(TextAction::Select(_)) => None,
    };
    if let Some(icon) = icon {
        world
            .entity_mut(entity)
            .insert(crate::native::icons::IconButton::new(icon, value));
    } else {
        world
            .entity_mut(entity)
            .insert(crate::native::icons::Tooltip(value.into()));
        label(world, entity, value, 15.0);
    }
    entity
}

pub(crate) fn builtin_store_entries(
    world: &mut World,
    root: Entity,
    sands_group: Entity,
    castles_group: Entity,
) -> (Vec<Entity>, Vec<Entity>) {
    let mut sand_entries = Vec::new();
    for kind in SandKind::ALL
        .into_iter()
        .filter(|kind| !matches!(kind, SandKind::Sync | SandKind::HandCamera))
    {
        crate::native::sand_store::entry(world, root, sands_group, kind, None);
        sand_entries.push(*world.get::<Children>(sands_group).unwrap().last().unwrap());
    }
    let mut castle_entries = Vec::new();
    let entry = store_group(world, castles_group);
    crate::native::drawing::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::record_extensions::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::sand_store::entry(world, root, entry, SandKind::Sync, None);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::sand_store::entry(world, root, entry, SandKind::HandCamera, None);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::instinct::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::protein_castle::store_entries(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::karma_castle::store_entry(world, root, entry);
    crate::native::frequency_castle::store_entry(world, root, entry);
    crate::native::transfer_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::full_record::store_entry(world, root, entry);
    crate::native::relation_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::recorder_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::document_viewer::store_entry(world, root, entry);
    crate::native::demeter_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::file_explorer::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::ide::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::shader_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::assertion_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::calendar::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::map_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::simulation_castle::store_entry(world, root, entry);
    castle_entries.push(entry);
    let entry = store_group(world, castles_group);
    crate::native::kanban::store_entry(world, root, entry);
    castle_entries.push(entry);
    (sand_entries, castle_entries)
}

pub(crate) fn workspace_changed(world: &mut World, root: Entity) {
    if world.get::<EditMode>(root).is_some_and(|mode| {
        mode.enabled
            && mode
                .name
                .is_some_and(|field| world.get::<WorkspaceName>(field).is_some())
    }) {
        autosave_name(world);
        render_panel(world, root);
    }
}

pub(crate) fn reorder_workspaces(world: &mut World, root: Entity) {
    workspaces::reorder(world, root);
}

pub(crate) fn render_panel(world: &mut World, root: Entity) {
    render_panel_content(world, root);
    let panel = world.get::<EditMode>(root).unwrap().panel;
    world.spawn((
        ChildOf(panel),
        Node {
            height: px(16),
            min_height: px(16),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

fn render_panel_content(world: &mut World, root: Entity) {
    let mode = world.get::<EditMode>(root).unwrap();
    let panel = mode.panel;
    let confirm = mode.confirm_remove;
    let credits = mode.credits;
    let store = mode.store;
    let canvas = mode.canvas;
    let customization = mode.customization;
    let areas = mode.areas;
    let general = mode.general;
    let information = mode.information;
    let shortcuts = mode.shortcuts;
    let park = mode.park;
    let children: Vec<_> = world
        .get::<Children>(panel)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    let existing_tabs = children
        .iter()
        .copied()
        .find(|child| world.get::<EditTabs>(*child).is_some());
    for child in children {
        if Some(child) != existing_tabs && world.get::<EditClose>(child).is_none() {
            world.despawn(child);
        }
    }
    world.get_mut::<EditMode>(root).unwrap().name = None;
    world.get_mut::<Node>(panel).unwrap().width = px(if customization { 720 } else { 376 });
    world
        .entity_mut(panel)
        .insert(crate::native::token_metrics::WidthToken(
            if customization {
                crate::native::tokens::Token::CustomizationWidth
            } else {
                crate::native::tokens::Token::PanelWidth
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
                    padding: UiRect::right(px(48)),
                    flex_wrap: FlexWrap::Wrap,
                    align_content: AlignContent::FlexStart,
                    row_gap: px(8),
                    flex_shrink: 0.0,
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        control(world, root, tabs, EditAction::General, Tr::General.text());
        control(world, root, tabs, EditAction::Store, Tr::SandStore.text());
        control(
            world,
            root,
            tabs,
            EditAction::Customization,
            Tr::Customization.text(),
        );
        control(
            world,
            root,
            tabs,
            EditAction::Workspaces,
            Tr::Workspaces.text(),
        );
        control(
            world,
            root,
            tabs,
            EditAction::Information,
            Tr::Information.text(),
        );
        control(
            world,
            root,
            tabs,
            EditAction::Shortcuts,
            Tr::Shortcuts.text(),
        );
        control(world, root, tabs, EditAction::Park, "Park");
        control(
            world,
            root,
            tabs,
            EditAction::Areas,
            Tr::AreasInfluence.text(),
        );
        control(world, root, tabs, EditAction::Credits, Tr::Licenses.text());
        let close = control(
            world,
            root,
            tabs,
            EditAction::Close,
            Tr::CloseEditMode.text(),
        );
        world.entity_mut(close).insert((EditClose, ZIndex(2)));
    }
    if park {
        label(world, panel, "Park", 22.0);
        label(world, panel, Tr::ParkSettingsDescription.text(), 14.0);
        crate::native::layout::desktop::monitors::panel(world, root, panel);
        crate::native::layout::desktop::magnification::panel(world, root, panel);
        crate::native::shortcuts::width_settings(world, root, panel);
        crate::native::topology::surface_budget::panel(world, root, panel);
        crate::native::layout::desktop::input_settings::panel(world, panel);
        crate::native::layout::desktop::pointer_settings::panel(world, panel);
        crate::native::layout::desktop::absolute_settings::panel(world, panel);
        crate::native::layout::desktop::focus::settings::panel(world, panel);
        crate::native::layout::desktop::opening::ui::panel(world, root, panel);
        crate::native::layout::desktop::gestures::panel(world, panel);
        crate::native::layout::desktop::overview::settings::panel(world, panel);
        crate::native::layout::desktop::external_settings::panel(world, root, panel);
        crate::native::layout::desktop::control::panel(world, panel);
        crate::native::capture::status_panel(world, panel);
        crate::native::shortcuts::panel(world, panel);
        return;
    }
    if shortcuts {
        crate::native::shortcuts::panel(world, panel);
        return;
    }
    if information {
        crate::native::information::panel(world, root, panel);
        return;
    }
    if general {
        label(world, panel, Tr::General.text(), 22.0);
        let always_show = world
            .get_resource::<crate::native::canvas_controls::ControlsSettings>()
            .is_some_and(|settings| settings.always_show);
        control(
            world,
            root,
            panel,
            EditAction::ToggleAlwaysShowControls,
            if always_show {
                Tr::AlwaysControlsOn.text()
            } else {
                Tr::AlwaysControlsOff.text()
            },
        );
        crate::native::workspace_config::controls(world, root, panel);
        crate::native::layout::desktop::controls(world, root, panel);
        crate::native::inspection::controls(world, root, panel);
        crate::native::shader_effects::controls(world, root, panel);
        if let Some(selected) = world
            .get::<crate::native::inspection::Inspection>(root)
            .and_then(|inspection| inspection.selected)
            .or_else(|| {
                crate::native::canvas_selection::selected(world, root)
                    .first()
                    .copied()
            })
        {
            crate::native::sand_panel::button(
                world,
                panel,
                root,
                Tr::EditComponent.text(),
                crate::native::canvas_host::authoring::inspector::Open(selected),
            );
            crate::native::sand_panel::button(
                world,
                panel,
                root,
                Tr::SurfaceEffect.text(),
                crate::native::shader_effects::OpenSurface(selected),
            );
        }
        crate::native::influence_report::ui::panel(world, root, panel, None);
        crate::native::deletion::controls(world, root, panel);
        return;
    }
    if areas {
        crate::native::area_panel::render(world, root, panel);
        return;
    }
    if customization {
        crate::native::customization::render(world, root, panel);
        return;
    }
    if canvas {
        crate::native::canvas_colors::render(world, root, panel);
        return;
    }
    if crate::native::sand_text_editor::render(world, root, panel) {
        return;
    }
    if credits {
        crate::native::credits::render(world, panel);
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
        label(world, heading, Tr::Workspace.text(), 22.0);
        control(
            world,
            root,
            heading,
            EditAction::CreateWorkspace,
            Tr::NewWorkspace.text(),
        );
        let spaces = world.get::<Workspaces>(root).unwrap();
        let active = spaces.active;
        let entries = spaces.entries.clone();
        let numbers = workspace::numbers(world, root);
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
            let number = numbers
                .iter()
                .position(|id| *id == entry.id)
                .map_or_else(|| "·".to_owned(), |index| (index + 1).to_string());
            let number = label(world, row, &number, 15.0);
            world.entity_mut(row).insert(workspaces::Row {
                workspace: entry.id,
                number,
            });
            let switch = if entry.id == active {
                let bundle = text_editor(&entry.name, world.resource::<Typography>(), 0);
                let editor = world
                    .spawn((
                        bundle,
                        EditField::WorkspaceName,
                        WorkspaceName(entry.id),
                        ChildOf(row),
                    ))
                    .id();
                let font = world.resource::<Typography>().text(15.0);
                world.entity_mut(editor).insert(font);
                world.entity_mut(editor).insert(EditableText {
                    allow_newlines: false,
                    visible_lines: Some(1.0),
                    max_characters: Some(80),
                    ..crate::native::sand::editable(&entry.name)
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
            {
                let mut node = world.get_mut::<Node>(switch).unwrap();
                node.flex_grow = 1.0;
                node.width = px(0);
                node.min_width = px(0);
                if entry.id != active {
                    node.flex_direction = FlexDirection::Column;
                }
            }
            if entry.id != active {
                let labels: Vec<_> = world.get::<Children>(switch).unwrap().iter().collect();
                for label in labels {
                    if world.get::<Text>(label).is_some() {
                        let mut node = world.get_mut::<Node>(label).unwrap();
                        node.width = percent(100);
                        node.min_width = px(0);
                        world.entity_mut(label).insert(TextLayout::linebreak(
                            bevy::text::LineBreak::WordOrCharacter,
                        ));
                    }
                }
            }
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
        let trash = world.get::<Workspaces>(root).unwrap().trash.clone();
        let retention_days = world.get::<Workspaces>(root).unwrap().trash_retention_days;
        label(
            world,
            panel,
            &format!("Trash · kept for {retention_days} days"),
            15.0,
        );
        let retention_row = crate::native::area_panel::row(world, panel);
        if retention_days > 1 {
            control(
                world,
                root,
                retention_row,
                EditAction::SetTrashRetention(retention_days - 1),
                Tr::LessRetention.text(),
            );
        }
        if retention_days < 365 {
            control(
                world,
                root,
                retention_row,
                EditAction::SetTrashRetention(retention_days + 1),
                Tr::MoreRetention.text(),
            );
        }
        for entry in trash {
            let days = (u64::from(retention_days) * 24 * 60 * 60)
                .saturating_sub(workspace::unix_time().saturating_sub(entry.deleted_at))
                .div_ceil(24 * 60 * 60);
            let row = crate::native::area_panel::row(world, panel);
            label(
                world,
                row,
                &format!("{} · {days} days left", entry.workspace.name),
                15.0,
            );
            control(
                world,
                root,
                row,
                EditAction::RestoreWorkspace(entry.workspace.id),
                &format!("Restore {}", entry.workspace.name),
            );
        }
        if let Some(removed) = confirm {
            if let Some(name) = entries
                .iter()
                .find(|entry| entry.id == removed)
                .map(|entry| entry.name.as_str())
            {
                label(
                    world,
                    panel,
                    &format!(
                        "Remove {name}? It and its Sands will stay in Trash for {retention_days} days."
                    ),
                    14.0,
                );
                control(
                    world,
                    root,
                    panel,
                    EditAction::ConfirmRemoveWorkspace,
                    Tr::TrashWorkspace.text(),
                );
                control(
                    world,
                    root,
                    panel,
                    EditAction::CancelRemoveWorkspace,
                    Tr::KeepWorkspace.text(),
                );
            }
        }
    } else {
        label(world, panel, Tr::SandStore.text(), 22.0);
        label(world, panel, Tr::Filter.text(), 14.0);
        let bundle = text_editor("", world.resource::<Typography>(), 0);
        let filter = world
            .spawn((
                bundle,
                ChildOf(panel),
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ))
            .id();
        world
            .get_mut::<EditableText>(filter)
            .unwrap()
            .max_characters = Some(200);
        world.get_mut::<EditableText>(filter).unwrap().visible_lines = Some(1.0);
        world.entity_mut(filter).insert((
            Node {
                width: percent(100),
                height: px(36),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                flex_grow: 0.0,
                ..default()
            },
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
        ));
        if let Some(mut node) = world.get_mut::<AccessibilityNode>(filter) {
            node.set_label(Tr::FilterSandStore.text());
        }
        let sands_heading = label(world, panel, "Sands", 18.0);
        let sands_group = store_group(world, panel);
        let castles_heading = label(world, panel, "Castles", 18.0);
        let castles_group = store_group(world, panel);
        let (sand_entries, mut castle_entries) =
            builtin_store_entries(world, root, sands_group, castles_group);
        let entry = store_group(world, castles_group);
        crate::native::custom_castle::store_entries(world, root, entry);
        castle_entries.push(entry);
        let entry = store_group(world, castles_group);
        crate::native::topology::ui::store_controls(world, root, entry);
        castle_entries.push(entry);
        let sands: Vec<_> = world
            .query::<(Entity, &ChildOf, &workspace::WorkspaceMember, &StoredSand)>()
            .iter(world)
            .filter(|(_, parent, member, _)| parent.parent() == root && member.0 == active)
            .map(|(entity, _, _, sand)| (entity, sand.kind))
            .collect();
        let workspace_heading = label(world, panel, Tr::InWorkspace.text(), 18.0);
        let workspace_group = store_group(world, panel);
        let mut workspace_entries = Vec::new();
        for (index, (entity, kind)) in sands.into_iter().enumerate() {
            crate::native::sand_store::entry(
                world,
                root,
                workspace_group,
                kind,
                Some((entity, index)),
            );
            workspace_entries.push(
                *world
                    .get::<Children>(workspace_group)
                    .unwrap()
                    .last()
                    .unwrap(),
            );
        }
        world.entity_mut(panel).insert(StoreFilter {
            input: filter,
            groups: vec![
                StoreFilterGroup {
                    heading: sands_heading,
                    entries: sand_entries,
                },
                StoreFilterGroup {
                    heading: castles_heading,
                    entries: castle_entries,
                },
                StoreFilterGroup {
                    heading: workspace_heading,
                    entries: workspace_entries,
                },
            ],
            observed: "\0".into(),
        });
    }
}

fn store_group(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) mod tests {
    use super::*;
    use crate::native::{
        container::BoxRoot,
        workspace::{WorkspaceMember, WorkspacePlugin},
    };
    use bevy::ui_widgets::Activate;

    #[derive(Resource, Default)]
    struct Toggles(Vec<(Entity, bool)>);

    pub(crate) fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
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
    fn panel_anchor_stays_visible_beside_a_leftmost_edit_button_and_reserved_bars() {
        for (root, width, anchor, usable) in [
            (360.0, 338.4, 320.0, [0.0, 360.0]),
            (1280.0, 376.0, 495.0, [0.0, 1280.0]),
            (360.0, 376.0, 320.0, [80.0, 340.0]),
            (640.0, 720.0, 600.0, [20.0, 620.0]),
        ] {
            let (right, maximum) = super::panel_horizontal(root, width, anchor, usable);
            let left = root - right - width.min(maximum);
            assert!(left >= usable[0] - 0.001, "{left} {usable:?}");
            assert!(root - right <= usable[1] + 0.001);
            assert!(maximum <= usable[1] - usable[0]);
        }
        assert_eq!(
            super::panel_horizontal(1280.0, 376.0, 495.0, [0.0, 1280.0]).0,
            495.0
        );
    }

    #[test]
    fn fractional_panel_anchoring_is_independent_of_rounded_pixel_width() {
        for scale in [0.5, 0.8, 1.0] {
            let (mut app, root) = fixture();
            let mode = app.world().get::<EditMode>(root).unwrap();
            let (panel, toggle) = (mode.panel, mode.toggle);
            app.world_mut().entity_mut(root).insert((
                ComputedNode {
                    size: Vec2::new(360.0, 720.0),
                    inverse_scale_factor: scale,
                    ..default()
                },
                UiGlobalTransform::from_xy(180.0, 360.0),
            ));
            app.world_mut().entity_mut(toggle).insert((
                ComputedNode {
                    size: Vec2::splat(40.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(20.0, 690.0),
            ));
            for width in [339.0, 338.0, 339.0, 338.0] {
                app.world_mut().entity_mut(panel).insert(ComputedNode {
                    size: Vec2::new(width, 656.0),
                    unrounded_size: Vec2::new(338.4, 656.0),
                    ..default()
                });
                app.update();
                let node = app.world().get::<Node>(panel).unwrap();
                let Val::Px(right) = node.right else {
                    panic!("pixel anchor")
                };
                let Val::Px(maximum) = node.max_width else {
                    panic!("pixel width")
                };
                assert!((right - 10.8 * scale).abs() < 0.001);
                assert!((maximum - 338.4 * scale).abs() < 0.001);
            }
        }
    }

    #[test]
    fn unrelated_panel_changes_do_not_undo_manual_scrolling_of_a_focused_modal() {
        let (mut app, root) = fixture();
        let panel = app
            .world_mut()
            .spawn((
                FocusScroll,
                ComputedNode {
                    size: Vec2::new(300.0, 200.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 100.0),
                ScrollPosition::default(),
                ChildOf(root),
            ))
            .id();
        let field = app
            .world_mut()
            .spawn((
                ComputedNode {
                    size: Vec2::new(180.0, 30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 500.0),
                ChildOf(panel),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 323.0);
        app.world_mut()
            .get_mut::<ScrollPosition>(panel)
            .unwrap()
            .0
            .y = 42.0;
        let other = app
            .world_mut()
            .spawn((
                FocusScroll,
                ComputedNode {
                    size: Vec2::new(300.0, 100.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 50.0),
                ScrollPosition::default(),
                ChildOf(root),
            ))
            .id();
        for height in [100.0, 200.0, 80.0] {
            app.world_mut()
                .get_mut::<ComputedNode>(other)
                .unwrap()
                .size
                .y = height;
            app.update();
            assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 42.0);
        }
        app.world_mut()
            .get_mut::<ComputedNode>(panel)
            .unwrap()
            .size
            .y = 100.0;
        app.update();
        assert!(app.world().get::<ScrollPosition>(panel).unwrap().0.y > 42.0);
    }

    #[test]
    fn appearance_inspector_scrolls_to_keyboard_focus_only_while_editing() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        let panel = app
            .world_mut()
            .spawn((
                crate::native::topology::ui::Inspector(root),
                ComputedNode {
                    size: Vec2::new(300.0, 200.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 100.0),
                ScrollPosition::default(),
                ChildOf(root),
            ))
            .id();
        let control = app
            .world_mut()
            .spawn((
                ComputedNode {
                    size: Vec2::new(180.0, 30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 500.0),
                ChildOf(panel),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(control, FocusCause::Navigated);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 323.0);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 323.0);
        app.world_mut().get_mut::<EditMode>(root).unwrap().enabled = false;
        app.world_mut()
            .get_mut::<ScrollPosition>(panel)
            .unwrap()
            .0
            .y = 0.0;
        let next = app
            .world_mut()
            .spawn((
                ComputedNode {
                    size: Vec2::splat(30.0),
                    ..default()
                },
                UiGlobalTransform::from_xy(150.0, 550.0),
                ChildOf(panel),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(next, FocusCause::Navigated);
        app.update();
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 0.0);
    }

    #[test]
    fn panel_and_child_wheel_events_use_one_shared_bounded_scroll_path() {
        let (mut app, root) = fixture();
        if !app.is_plugin_added::<crate::native::layout::LayoutPlugin>() {
            app.add_plugins(crate::native::layout::LayoutPlugin);
        }
        EditAction::Open.apply(app.world_mut(), root);
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        app.world_mut().entity_mut(panel).insert(ComputedNode {
            size: Vec2::new(376.0, 400.0),
            content_size: Vec2::new(376.0, 1000.0),
            inverse_scale_factor: 1.0,
            ..default()
        });
        let child = app
            .world_mut()
            .spawn((Node::default(), ChildOf(panel)))
            .id();
        for (target, delta, expected) in [
            (panel, -30.0, 30.0),
            (child, -30.0, 60.0),
            (panel, -10000.0, 600.0),
            (child, 10000.0, 0.0),
        ] {
            app.world_mut().trigger(Pointer::new(
                bevy::picking::pointer::PointerId::Mouse,
                bevy::picking::pointer::Location {
                    target: bevy::camera::NormalizedRenderTarget::Image(
                        Handle::<Image>::default().into(),
                    ),
                    position: Vec2::ZERO,
                },
                bevy::picking::events::Scroll {
                    unit: bevy::input::mouse::MouseScrollUnit::Pixel,
                    x: 0.0,
                    y: delta,
                    phase: bevy::input::touch::TouchPhase::Moved,
                    hit: bevy::picking::backend::HitData::new(target, 0.0, None, None),
                },
                target,
            ));
            assert_eq!(
                app.world().get::<ScrollPosition>(panel).unwrap().0.y,
                expected
            );
        }
    }

    #[cfg_attr(test, test)]
    fn store_cards_have_inert_content_previews_and_all_tabs_have_bottom_space() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Store.apply(app.world_mut(), root);
        let world = app.world_mut();
        let previews: Vec<_> = world
            .query_filtered::<Entity, With<crate::native::sand_store::SandPreview>>()
            .iter(world)
            .collect();
        assert!(previews.len() >= 13);
        for preview in previews {
            let card = world.get::<ChildOf>(preview).unwrap().parent();
            assert!(world.get::<ActionButton>(card).is_some());
            assert_eq!(
                world.get::<Node>(card).unwrap().padding,
                UiRect::all(px(10))
            );
            let mut pending = vec![preview];
            while let Some(entity) = pending.pop() {
                assert!(world.get::<ActionButton>(entity).is_none());
                assert!(world.get::<EditableText>(entity).is_none());
                assert!(
                    world
                        .get::<crate::native::canvas::CanvasItem>(entity)
                        .is_none()
                );
                assert!(
                    world
                        .get::<bevy::input_focus::tab_navigation::TabIndex>(entity)
                        .is_none()
                );
                if let Some(children) = world.get::<Children>(entity) {
                    pending.extend(children.iter());
                }
            }
        }
        for expected in [
            "Operation",
            "Total 00:00:00",
            "Record Castle",
            "Fiote Castle",
            "Protein Castle",
        ] {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == expected),
                "Missing {expected}"
            );
        }
        assert!(
            !world
                .query::<&crate::native::icons::Tooltip>()
                .iter(world)
                .any(|tip| tip.0 == "Record slug or identity")
        );
        for action in [
            EditAction::General,
            EditAction::Areas,
            EditAction::Store,
            EditAction::Workspaces,
            EditAction::Credits,
        ] {
            action.apply(world, root);
            let panel = world.get::<EditMode>(root).unwrap().panel;
            let content = *world.get::<Children>(panel).unwrap().last().unwrap();
            let node = world.get::<Node>(content).unwrap();
            assert_eq!(node.height, px(16));
            assert_eq!(node.flex_shrink, 0.0);
        }
    }

    #[cfg_attr(test, test)]
    fn all_edit_panel_tabs_activate_render_and_reopen() {
        let (mut app, root) = fixture();
        app.add_plugins(crate::native::information::InformationPlugin);
        activate(&mut app, root, EditAction::Toggle);
        let tabs = app
            .world_mut()
            .query_filtered::<Entity, With<EditTabs>>()
            .single(app.world())
            .unwrap();
        let controls: Vec<_> = app
            .world()
            .get::<Children>(tabs)
            .unwrap()
            .iter()
            .filter_map(|entity| {
                let control = app.world().get::<EditControl>(entity)?;
                (control.action != EditAction::Close).then_some((entity, control.action))
            })
            .collect();
        let expected = [
            (EditAction::General, "General"),
            (EditAction::Store, "Sand store"),
            (EditAction::Customization, "Customization"),
            (EditAction::Workspaces, "Workspace"),
            (EditAction::Information, "Information"),
            (EditAction::Shortcuts, "Shortcuts"),
            (EditAction::Park, "Park"),
            (EditAction::Areas, "Areas"),
            (EditAction::Credits, "Licenses and credits"),
        ];
        assert_eq!(controls.len(), expected.len());
        for _ in 0..2 {
            for ((entity, action), (expected_action, heading)) in controls.iter().zip(expected) {
                assert_eq!(*action, expected_action);
                app.world_mut().trigger(Activate { entity: *entity });
                app.update();
                app.update();
                let world = app.world_mut();
                let mode = world.get::<EditMode>(root).unwrap();
                assert!(mode.enabled, "{action:?}");
                let panel = mode.panel;
                assert_eq!(world.get::<Node>(panel).unwrap().display, Display::Flex);
                let mut pending = vec![panel];
                let mut found = false;
                while let Some(entity) = pending.pop() {
                    found |= world
                        .get::<Text>(entity)
                        .is_some_and(|text| text.0 == heading);
                    if let Some(children) = world.get::<Children>(entity) {
                        pending.extend(children.iter());
                    }
                }
                assert!(found, "Missing {heading} after activating {action:?}");
                if *action == EditAction::Store {
                    let kinds: Vec<_> = world
                        .query::<&crate::native::sand_store::StoreEntry>()
                        .iter(world)
                        .map(|entry| entry.0)
                        .collect();
                    for kind in SandKind::ALL {
                        assert!(kinds.contains(&kind), "Missing {kind:?} store card");
                    }
                    assert_eq!(world.query::<&StoreFilter>().iter(world).count(), 1);
                    assert_eq!(world.query::<&StoredSand>().iter(world).count(), 0);
                }
            }
            activate(&mut app, root, EditAction::Close);
            assert!(!app.world().get::<EditMode>(root).unwrap().enabled);
            activate(&mut app, root, EditAction::Toggle);
            assert!(app.world().get_entity(tabs).is_ok());
        }
    }

    #[cfg_attr(test, test)]
    fn selected_tabs_preserve_content_and_scroll() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        for action in [
            EditAction::Store,
            EditAction::Information,
            EditAction::Shortcuts,
            EditAction::Park,
            EditAction::General,
            EditAction::Areas,
            EditAction::Customization,
            EditAction::Canvas,
            EditAction::Credits,
            EditAction::Workspaces,
        ] {
            action.apply(app.world_mut(), root);
            app.update();
            let world = app.world_mut();
            let panel = world.get::<EditMode>(root).unwrap().panel;
            let children: Vec<_> = world.get::<Children>(panel).unwrap().iter().collect();
            let scroll = Vec2::new(0.0, 42.0);
            world.get_mut::<ScrollPosition>(panel).unwrap().0 = scroll;
            action.apply(world, root);
            world.flush();
            assert!(world.get::<EditMode>(root).unwrap().enabled, "{action:?}");
            assert_eq!(
                world.get::<ScrollPosition>(panel).unwrap().0,
                scroll,
                "{action:?}"
            );
            assert_eq!(
                world
                    .get::<Children>(panel)
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>(),
                children,
                "{action:?}"
            );
        }
    }

    #[cfg_attr(test, test)]
    fn store_credits_only_appear_in_tabs_and_reselection_keeps_filter() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        activate(&mut app, root, EditAction::Store);
        let world = app.world_mut();
        let credits: Vec<_> = world
            .query::<(Entity, &EditControl)>()
            .iter(world)
            .filter(|(_, control)| control.action == EditAction::Credits)
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(credits.len(), 1);
        let tabs = world.get::<ChildOf>(credits[0]).unwrap().parent();
        assert!(world.get::<EditTabs>(tabs).is_some());
        let filter = world.query::<&StoreFilter>().single(world).unwrap().input;
        world
            .get_mut::<EditableText>(filter)
            .unwrap()
            .editor
            .set_text("Square");
        activate(&mut app, root, EditAction::Store);
        assert_eq!(
            app.world().get::<EditableText>(filter).unwrap().value(),
            "Square"
        );
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
        let close = app
            .world_mut()
            .query_filtered::<Entity, With<EditClose>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<ChildOf>(close).unwrap().parent(), tabs);

        EditAction::General.apply(app.world_mut(), root);
        assert!(app.world().get::<EditMode>(root).unwrap().general);
        for enabled in [true, false] {
            assert!(
                app.world_mut()
                    .query::<&EditControl>()
                    .iter(app.world())
                    .any(|control| { control.action == EditAction::ToggleAlwaysShowControls })
            );
            activate(&mut app, root, EditAction::ToggleAlwaysShowControls);
            assert_eq!(
                app.world()
                    .resource::<crate::native::canvas_controls::ControlsSettings>()
                    .always_show,
                enabled
            );
        }
        assert!(
            app.world_mut()
                .query::<&EditControl>()
                .iter(app.world())
                .any(|control| control.action == EditAction::TogglePhysics)
        );
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
                .query::<&EditControl>()
                .iter(app.world())
                .any(|control| control.action == EditAction::TogglePhysics)
        );
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
        assert!(app.world().get_entity(close).is_ok());
        assert_eq!(
            app.world().get::<Node>(close).unwrap().position_type,
            PositionType::Absolute
        );
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        app.world_mut()
            .get_mut::<ComputedNode>(panel)
            .unwrap()
            .scroll_position
            .y = 120.0;
        app.update();
        assert_eq!(app.world().get::<Node>(close).unwrap().top, px(0));
        assert_eq!(app.world().get::<Node>(close).unwrap().right, px(0));
        assert_eq!(app.world().get::<ChildOf>(close).unwrap().parent(), tabs);
        assert_eq!(app.world().get::<Node>(tabs).unwrap().max_height, Val::Auto);
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .clear();
        app.update();
        assert_eq!(app.world().get::<Node>(tabs).unwrap().max_height, px(40));
    }

    #[test]
    fn asynchronous_workspace_switch_saves_the_bound_name_and_refreshes_the_selected_row() {
        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        let field = app.world().get::<EditMode>(root).unwrap().name.unwrap();
        let mut other = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
        other.id = 31;
        other.name = "Destination".into();
        app.world_mut()
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .push(other);
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text("Source draft");
        assert!(workspace::switch_output(app.world_mut(), root, 31));
        app.update();
        let spaces = app.world().get::<Workspaces>(root).unwrap();
        assert_eq!(spaces.entries[0].name, "Source draft");
        assert_eq!(spaces.entries[1].name, "Destination");
        let next = app.world().get::<EditMode>(root).unwrap().name.unwrap();
        assert_ne!(next, field);
        assert!(app.world().get_entity(field).is_err());
        assert_eq!(app.world().get::<WorkspaceName>(next).unwrap().0, 31);
        assert_eq!(
            app.world().get::<EditableText>(next).unwrap().value(),
            "Destination"
        );
        EditAction::Park.apply(app.world_mut(), root);
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        assert!(app.world().get::<EditMode>(root).unwrap().name.is_none());
        let children = app
            .world()
            .get::<Children>(panel)
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        assert!(workspace::switch_output(app.world_mut(), root, 1));
        assert_eq!(
            app.world()
                .get::<Children>(panel)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
    }

    #[cfg_attr(test, test)]
    fn workspace_name_saves_from_its_row_and_pointer_focus_has_no_outline() {
        let (mut app, root) = fixture();
        app.add_plugins(crate::native::sand::SandPlugin);
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
        app.world_mut().trigger(FocusGained {
            entity: toggle,
            cause: FocusCause::Navigated,
        });
        app.update();
        assert_eq!(app.world().get::<Outline>(toggle).unwrap().width, px(0));
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocusVisible>()
            .0 = true;
        app.world_mut().trigger(FocusGained {
            entity: toggle,
            cause: FocusCause::Navigated,
        });
        app.update();
        assert_eq!(app.world().get::<Outline>(toggle).unwrap().width, px(2));
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocusVisible>()
            .0 = false;
        app.update();
        assert_eq!(app.world().get::<Outline>(toggle).unwrap().width, px(0));
    }

    #[cfg_attr(test, test)]
    fn edit_button_emits_one_scoped_event_and_close_restores_focus() {
        let (mut app, root) = fixture();
        let draws = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let requested = draws.clone();
        app.insert_resource(crate::native::wake::WakeSignal::new(move || {
            requested.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
        let other = app.world_mut().spawn(BoxRoot).id();
        app.update();
        let toggle = app.world().get::<EditMode>(root).unwrap().toggle;
        app.world_mut().resource_mut::<InputFocus>().set(toggle, FocusCause::Navigated);
        activate(&mut app, root, EditAction::Toggle);
        assert!(draws.load(std::sync::atomic::Ordering::Relaxed) > 0);
        assert!(app.world().get::<EditMode>(root).unwrap().enabled);
        assert!(!app.world().get::<EditMode>(other).unwrap().enabled);
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
        let focused = app.world().resource::<InputFocus>().get().unwrap();
        assert_eq!(app.world().get::<EditControl>(focused).unwrap().root, root);
        let mut navigation = bevy::ecs::system::SystemState::<bevy::input_focus::tab_navigation::TabNavigation>::new(app.world_mut());
        let next = navigation.get(app.world()).unwrap().navigate(app.world().resource::<InputFocus>(), bevy::input_focus::tab_navigation::NavAction::Next).unwrap();
        assert_ne!(next, focused);
        assert_eq!(app.world().get::<EditControl>(next).unwrap().root, root);
        crate::native::actions::dispatch(
            app.world_mut(),
            root,
            crate::native::actions![EditAction::Close],
        );
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

    #[test]
    fn workspace_controls_send_stable_identity_requests_without_changing_park_membership() {
        use crate::host::HostRequest;
        use crate::native::layout::desktop::HostRequests;

        let (mut app, root) = fixture();
        EditAction::Open.apply(app.world_mut(), root);
        let mut other = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
        other.id = 31;
        other.name = "Second workspace".into();
        app.world_mut()
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .push(other);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        app.insert_resource(HostRequests(sender));
        EditAction::SwitchWorkspace(31).apply(app.world_mut(), root);
        assert_eq!(
            receiver.try_recv(),
            Ok(HostRequest::Workspace {
                owner: root.to_bits(),
                id: 31
            })
        );
        assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
        EditAction::CreateWorkspace.apply(app.world_mut(), root);
        EditAction::SwitchWorkspace(31).apply(app.world_mut(), root);
        assert_eq!(
            receiver.try_recv(),
            Ok(HostRequest::CreateWorkspace {
                owner: root.to_bits()
            })
        );
        assert!(receiver.try_recv().is_err());
        let spaces = app.world().get::<Workspaces>(root).unwrap();
        assert_eq!(spaces.entries.len(), 2);
        assert_eq!(spaces.active, 1);
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
            crate::native::canvas_background::current(app.world(), root),
            crate::native::canvas_background::CanvasColors {
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
        let colors = crate::native::canvas_background::CanvasColors {
            background: [16, 32, 48],
            grid: [170, 187, 204],
        };
        assert_eq!(
            crate::native::canvas_background::current(app.world(), root),
            colors
        );
        activate(&mut app, root, EditAction::Workspaces);
        activate(&mut app, root, EditAction::CreateWorkspace);
        assert_eq!(
            crate::native::canvas_background::current(app.world(), root),
            Default::default()
        );
        activate(&mut app, root, EditAction::SwitchWorkspace(1));
        assert_eq!(
            crate::native::canvas_background::current(app.world(), root),
            colors
        );
        activate(&mut app, root, EditAction::Canvas);
        activate(&mut app, root, EditAction::ResetCanvasColors);
        assert_eq!(
            crate::native::canvas_background::current(app.world(), root),
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
            .get::<crate::native::sand_text_editor::TextPanel>(panel)
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
            assert_eq!(
                crate::native::sand_text::value(app.world(), selected),
                value
            );
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
        }
        let width = app
            .world()
            .get::<crate::native::sand_text_editor::TextPanel>(panel)
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
            .get::<crate::native::sand_text::SandText>(selected)
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
                .get::<crate::native::sand_text::SandText>(selected)
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
            crate::native::sand_text::value(app.world(), selected),
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
                .get::<crate::native::sand_text::SandText>(selected)
                .unwrap()
                .size[0],
            320.0
        );
    }

    #[cfg_attr(test, test)]
    fn workspace_and_store_controls_work_with_filtering_and_default_content() {
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
        let (filter, sand_entries) = {
            let store = app
                .world_mut()
                .query::<&StoreFilter>()
                .single(app.world())
                .unwrap();
            (store.input, store.groups[0].entries.clone())
        };
        app.world_mut()
            .get_mut::<EditableText>(filter)
            .unwrap()
            .editor
            .set_text("time castle");
        app.update();
        for entry in sand_entries {
            let visible = app.world().get::<Node>(entry).unwrap().display == Display::Flex;
            assert_eq!(
                visible,
                app.world()
                    .get::<crate::native::sand_store::StoreEntry>(entry)
                    .unwrap()
                    .0
                    == SandKind::WorkTimer
            );
        }
        app.world_mut()
            .get_mut::<EditableText>(filter)
            .unwrap()
            .editor
            .set_text("");
        app.update();
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
            "Editable text"
        );
        assert!(
            app.world()
                .get::<crate::native::sand_store::SandCredits>(sand)
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
        assert_eq!(app.world().get::<WorkspaceMember>(sand).unwrap().0, 2);
        let spaces = app.world().get::<Workspaces>(root).unwrap();
        assert_eq!(spaces.active, 1);
        assert_eq!(spaces.entries.len(), 1);
        assert_eq!(spaces.trash[0].workspace.id, 2);
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Editable text"
        );
        activate(&mut app, root, EditAction::RestoreWorkspace(2));
        let spaces = app.world().get::<Workspaces>(root).unwrap();
        assert_eq!(spaces.active, 2);
        assert!(spaces.trash.is_empty());
        assert_eq!(app.world().get::<WorkspaceMember>(sand).unwrap().0, 2);
        activate(&mut app, root, EditAction::Store);
        activate(&mut app, root, EditAction::RemoveSand(sand));
        assert!(app.world().get_entity(editor).is_ok());
        crate::native::deletion::Decision(true).apply(app.world_mut(), root);
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
        let name = app.world().get::<EditMode>(root).unwrap().name.unwrap();
        let active_font = app.world().get::<TextFont>(name).unwrap().clone();
        let switch = app
            .world_mut()
            .query::<(Entity, &EditControl)>()
            .iter(app.world())
            .find(|(_, control)| control.action == EditAction::SwitchWorkspace(1))
            .unwrap()
            .0;
        let text = app.world().get::<Children>(switch).unwrap()[0];
        let inactive_font = app.world().get::<TextFont>(text).unwrap();
        assert_eq!(active_font.font, inactive_font.font);
        assert_eq!(active_font.font_size, inactive_font.font_size);
        for (delete, _, row) in &deletes {
            assert_eq!(
                app.world().get::<Children>(*row).unwrap().last(),
                Some(delete)
            );
            assert_eq!(
                app.world()
                    .get::<crate::native::icons::IconButton>(*delete)
                    .unwrap()
                    .icon,
                crate::native::icons::Icon::Close
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
        assert_eq!(spaces.trash.len(), 1);
        activate(&mut app, root, EditAction::SetTrashRetention(11));
        assert_eq!(
            app.world()
                .get::<Workspaces>(root)
                .unwrap()
                .trash_retention_days,
            11
        );
        activate(&mut app, root, EditAction::RestoreWorkspace(1));
        assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
        assert!(
            app.world()
                .get::<Workspaces>(root)
                .unwrap()
                .trash
                .is_empty()
        );
    }

    crate::native::laboratory_cases! {
        panel_anchor_stays_visible_beside_a_leftmost_edit_button_and_reserved_bars,
        all_edit_panel_tabs_activate_render_and_reopen,
        store_cards_have_inert_content_previews_and_all_tabs_have_bottom_space,
        selected_tabs_preserve_content_and_scroll,
        store_credits_only_appear_in_tabs_and_reselection_keeps_filter,
        general_settings_are_separate_and_tab_entities_survive_navigation,
        workspace_name_saves_from_its_row_and_pointer_focus_has_no_outline,
        edit_button_emits_one_scoped_event_and_close_restores_focus,
        canvas_colors_save_valid_fields_and_stay_with_their_workspace,
        text_properties_save_while_typing_without_replacing_the_focused_editor,
        workspace_and_store_controls_work_with_filtering_and_default_content,
        workspace_rows_keep_delete_icons_with_their_workspace,
    }
}
