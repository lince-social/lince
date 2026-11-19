use crate::native::{
    actions::{Action, ActionButton},
    castle::Castle,
    edit_mode::{EditAction, EditMode},
    sand::{ImageSand, Square},
    theme::Typography,
};
use bevy::{
    picking::{
        hover::{HoverMap, generate_hovermap},
        pointer::{PointerAction, PointerButton, PointerId, PointerInput},
    },
    prelude::*,
    text::EditableText,
};
use std::{borrow::Cow, collections::HashSet};

mod drawing;
mod preparation;
use drawing::{Drawing, Mark};
use preparation::draw;

#[derive(Component, Clone, PartialEq)]
pub struct Inspection {
    pub click: bool,
    pub hover: bool,
    pub contours: bool,
    pub events: bool,
    pub hidden: bool,
    pub selected: Option<Entity>,
    pub(crate) selected_point: Option<Vec2>,
    pub(crate) hovered: Option<Entity>,
    suppressed: Option<Entity>,
}

impl Default for Inspection {
    fn default() -> Self {
        Self {
            click: false,
            hover: false,
            contours: false,
            events: false,
            hidden: false,
            selected: None,
            selected_point: None,
            hovered: None,
            suppressed: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub target: Entity,
    pub name: String,
}

#[derive(Component, Default)]
pub struct EventConnections(pub Vec<Connection>);

#[derive(Component)]
pub struct InspectionOverlay;

#[derive(Component, Default)]
pub struct InspectionExcluded;

pub(crate) fn excluded(world: &World, entity: Entity) -> bool {
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if world.get::<InspectionExcluded>(entity).is_some() {
            return true;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

#[derive(Component, Clone, Copy)]
struct Setting {
    root: Entity,
    index: usize,
}

impl Action for Setting {
    fn apply(&self, world: &mut World, _: Entity) {
        if let Some(mut state) = world.get_mut::<Inspection>(self.root) {
            match self.index {
                0 => {
                    state.click = !state.click;
                    if !state.click {
                        state.selected = None;
                    }
                }
                1 => state.hover = !state.hover,
                2 => state.contours = !state.contours,
                3 => state.events = !state.events,
                _ => state.hidden = !state.hidden,
            }
        }
    }
}

pub struct Deselect;

impl Action for Deselect {
    fn apply(&self, world: &mut World, root: Entity) {
        crate::native::canvas_selection::clear(world, root);
        if let Some(mut editor) = world.get_mut::<crate::native::area_panel::AreaEditor>(root)
            && editor.tool.is_some()
        {
            editor.cancel();
            crate::native::edit_mode::render_panel(world, root);
            return;
        }
        if let Some(mut state) = world.get_mut::<Inspection>(root)
            && (state.selected.is_some() || state.hovered.is_some())
        {
            state.selected = None;
            state.suppressed = state.hovered;
            state.hovered = None;
        } else {
            EditAction::Close.apply(world, root);
        }
    }
}

pub(crate) fn controls(world: &mut World, root: Entity, panel: Entity) {
    if world.get::<Inspection>(root).is_none() {
        world.entity_mut(root).insert(Inspection::default());
    }
    let row = world
        .spawn((
            Node {
                column_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    for (index, name) in ["Click", "Hover", "Groups", "Events", "Hidden"]
        .into_iter()
        .enumerate()
    {
        let setting = Setting { root, index };
        let button = world
            .spawn((
                crate::native::sand::button(0),
                setting,
                ActionButton::new(root, crate::native::actions![setting]),
                ChildOf(row),
                Node {
                    padding: UiRect::axes(px(5), px(5)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ))
            .id();
        crate::native::edit_mode::label(world, button, name, 12.0);
    }
}

pub struct InspectionPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InspectInput;

impl Plugin for InspectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Drawing>()
            .add_systems(
                PreUpdate,
                input.in_set(InspectInput).after(generate_hovermap),
            )
            .add_systems(
                PostUpdate,
                (settings, draw)
                    .chain()
                    .after(crate::native::actions::ApplyActions)
                    .after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

fn input(
    mut events: MessageReader<PointerInput>,
    hover: Res<HoverMap>,
    parents: Query<&ChildOf>,
    candidates: Query<
        (),
        Or<(
            With<Square>,
            With<ImageSand>,
            With<Text>,
            With<EditableText>,
            With<Castle>,
            With<crate::native::canvas::CanvasItem>,
        )>,
    >,
    settings: Query<
        (),
        Or<(
            With<Setting>,
            With<crate::native::canvas_item::CanvasItemMenu>,
        )>,
    >,
    excluded: Query<(), With<InspectionExcluded>>,
    mut roots: Query<(Entity, &EditMode, &mut Inspection)>,
    mut chain: Local<Vec<Entity>>,
) {
    let _input = engine::performance::Span::new("inspection-input");
    let pressed = events
        .read()
        .filter(|event| {
            event.pointer_id == PointerId::Mouse
                && matches!(event.action, PointerAction::Press(PointerButton::Primary))
        })
        .map(|event| event.location.position)
        .last();
    let hit = hover.get(&PointerId::Mouse).and_then(|hits| {
        hits.iter()
            .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
            .map(|(entity, _)| *entity)
    });
    chain.clear();
    let mut cursor = hit;
    while let Some(entity) = cursor {
        chain.push(entity);
        cursor = parents.get(entity).ok().map(ChildOf::parent);
    }
    let control = chain.iter().any(|entity| settings.contains(*entity));
    let excluded = chain.iter().any(|entity| excluded.contains(*entity));
    for (root, mode, mut state) in &mut roots {
        let mut next = (*state).clone();
        if !mode.enabled {
            next.selected = None;
            next.selected_point = None;
            next.hovered = None;
            next.suppressed = None;
        } else if control {
            continue;
        } else if excluded {
            next.hovered = None;
        } else {
            let in_root = chain.contains(&root);
            let candidate = in_root
                .then(|| {
                    chain
                        .iter()
                        .copied()
                        .find(|entity| *entity != root && candidates.contains(*entity))
                })
                .flatten();
            if candidate != next.suppressed {
                next.suppressed = None;
            }
            next.hovered = candidate.filter(|entity| Some(*entity) != next.suppressed);
            if pressed.is_some() && in_root {
                next.selected_point = pressed;
                if next.click {
                    select_control(&mut next, candidate, pressed);
                }
            }
        }
        if state.set_if_neq(next) {
            engine::performance::add("inspection-state-writes", 1);
        }
    }
}

fn settings(
    states: Query<&Inspection>,
    buttons: Query<(&Setting, &Children)>,
    mut labels: Query<&mut Text>,
) {
    for (setting, children) in &buttons {
        let Ok(state) = states.get(setting.root) else {
            continue;
        };
        let enabled = [
            state.click,
            state.hover,
            state.contours,
            state.events,
            state.hidden,
        ][setting.index];
        let (off, on) = [
            ("Click: off", "Click: on"),
            ("Hover: off", "Hover: on"),
            ("Groups: off", "Groups: on"),
            ("Events: off", "Events: on"),
            ("Hidden: off", "Hidden: on"),
        ][setting.index];
        let value = if enabled { on } else { off };
        for child in children {
            if let Ok(mut text) = labels.get_mut(*child)
                && text.0 != value
            {
                text.0.clear();
                text.0.push_str(value);
            }
        }
    }
}

fn descendants(world: &World, entity: Entity, result: &mut HashSet<Entity>) {
    if !result.insert(entity) {
        return;
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children {
            descendants(world, *child, result);
        }
    }
}

fn family(world: &World, root: Entity, entity: Entity, result: &mut HashSet<Entity>) {
    let mut outer = entity;
    let mut cursor = entity;
    while let Some(parent) = world.get::<ChildOf>(cursor) {
        cursor = parent.parent();
        if cursor == root {
            break;
        }
        if world.get::<Castle>(cursor).is_some() {
            outer = cursor;
        }
    }
    result.clear();
    descendants(world, outer, result);
    for member in crate::native::canvas_selection::group_members_iter(world, root, outer) {
        descendants(world, member, result);
    }
}

fn hidden(world: &World, entity: Entity) -> bool {
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
            || world
                .get::<Node>(entity)
                .is_some_and(|node| node.display == Display::None)
        {
            return true;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

fn active_member(world: &World, root: Entity, entity: Entity) -> bool {
    let active = world
        .get::<crate::native::workspace::Workspaces>(root)
        .map(|spaces| spaces.active);
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if entity == root {
            return true;
        }
        if let Some(member) = world.get::<crate::native::workspace::WorkspaceMember>(entity)
            && active.is_some_and(|active| {
                member.0 != active && !crate::native::topology::holes::live(world, root, member.0)
            })
        {
            return false;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

#[derive(Default)]
struct Preview {
    members: HashSet<Entity>,
    ordered: Vec<Entity>,
}

impl Preview {
    fn text(&mut self, world: &World, entity: Entity) -> String {
        self.members.clear();
        descendants(world, entity, &mut self.members);
        self.ordered.clear();
        self.ordered.extend(self.members.iter().copied());
        self.ordered.sort();
        let mut text = String::from("Hidden: ");
        let mut remaining = 160;
        let mut first = true;
        for entity in self.ordered.iter().copied() {
            let mut append = |characters: &mut dyn Iterator<Item = char>| {
                if !first {
                    text.push(' ');
                    remaining -= 1;
                }
                first = false;
                for ch in characters.take(remaining) {
                    text.push(ch);
                    remaining -= 1;
                }
            };
            if let Some(value) = world.get::<Text>(entity) {
                append(&mut value.0.chars());
            } else if let Some(value) = world.get::<EditableText>(entity) {
                append(&mut value.value().chars());
            }
            if remaining == 0 {
                break;
            }
        }
        if text.len() == "Hidden: ".len() {
            "Hidden Sand".into()
        } else {
            text
        }
    }
}

pub(crate) fn bounds(world: &World, entity: Entity) -> Option<Rect> {
    if world
        .get::<crate::native::topology::presentation::SpatialRoot>(entity)
        .is_some()
    {
        return world
            .get::<Camera>(
                world
                    .get_resource::<crate::native::topology::presentation::SceneCamera>()?
                    .0,
            )?
            .logical_viewport_rect();
    }
    if let Some(bounds) = crate::native::topology::presentation::bounds(world, entity) {
        return Some(bounds);
    }
    let computed = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let scale = computed.inverse_scale_factor();
    let size = computed.size()
        * Vec2::new(
            transform.matrix2.x_axis.length(),
            transform.matrix2.y_axis.length(),
        )
        * scale;
    let center = transform.translation * scale;
    (size.is_finite() && center.is_finite() && size.min_element() > 0.0)
        .then(|| Rect::from_center_size(center, size))
}

pub(crate) fn connections(world: &World, entity: Entity) -> Vec<Connection> {
    if excluded(world, entity) {
        return Vec::new();
    }
    let mut links = Vec::new();
    visit_connections(world, entity, |target, name| {
        links.push(Connection {
            target,
            name: name.into_owned(),
        });
    });
    links
}

fn visit_connections<'a>(
    world: &'a World,
    entity: Entity,
    mut visit: impl FnMut(Entity, Cow<'a, str>),
) {
    if let Some(links) = world.get::<EventConnections>(entity) {
        for link in &links.0 {
            visit(link.target, Cow::Borrowed(&link.name));
        }
    }
    if let Some(effect) = world.get::<crate::native::effect::SendBoxEvent>(entity)
        && world.get::<Square>(entity).is_some()
        && world
            .get::<crate::native::container::BoxRoot>(effect.box_entity)
            .is_some()
        && world
            .get::<crate::native::sand::InBox>(entity)
            .is_some_and(|owner| owner.0 == effect.box_entity)
        && world
            .get::<crate::native::sand::InBox>(effect.square)
            .is_some_and(|owner| owner.0 == effect.box_entity)
        && world.get::<Square>(effect.square).is_some()
    {
        visit(effect.square, Cow::Borrowed("Sand Clicked Toggle"));
    }
    if let Some(button) = world.get::<ActionButton>(entity) {
        let described = button.actions.connections(world, button.target);
        if described.is_empty() {
            let label = world
                .get::<crate::native::icons::IconButton>(entity)
                .map(|icon| icon.label.as_str())
                .or_else(|| {
                    world
                        .get::<crate::native::icons::Tooltip>(entity)
                        .map(|tip| tip.0.as_str())
                })
                .unwrap_or("Button");
            visit(button.target, Cow::Owned(format!("{label} Clicked")));
        } else {
            for link in described {
                visit(link.target, Cow::Owned(link.name));
            }
        }
    }
    if let Some(record) = world.get::<crate::native::record_view::RecordEditor>(entity) {
        visit(record.status, Cow::Borrowed("Record Text Changed Save"));
    }
}

pub(crate) fn inspect_control(world: &mut World, root: Entity, control: Entity) {
    if world
        .get::<Inspection>(root)
        .is_none_or(|state| !state.click)
    {
        Setting { root, index: 0 }.apply(world, root);
    }
    if let Some(mut state) = world.get_mut::<Inspection>(root) {
        select_control(&mut state, Some(control), None);
    }
}

fn select_control(state: &mut Inspection, candidate: Option<Entity>, point: Option<Vec2>) {
    state.selected_point = point;
    state.selected = candidate;
    state.suppressed = None;
}

pub(crate) fn edit_connection(
    world: &World,
    root: Entity,
    action: EditAction,
) -> Option<Connection> {
    let mode = world.get::<EditMode>(root)?;
    let target = match action {
        EditAction::RemoveSand(entity) | EditAction::EditSand(entity) => entity,
        EditAction::CreateWorkspace
        | EditAction::SwitchWorkspace(_)
        | EditAction::ConfirmRemoveWorkspace
        | EditAction::AddSand(_)
        | EditAction::CanvasColor(..)
        | EditAction::ResetCanvasColors => root,
        _ => mode.panel,
    };
    let name = match action {
        EditAction::Open => "Edit Panel Clicked Open",
        EditAction::Close => "Edit Panel Clicked Close",
        EditAction::Toggle => "Edit Panel Clicked Toggle",
        EditAction::CreateWorkspace => "Workspace Clicked Add",
        EditAction::TogglePhysics => "Workspace Physics Clicked Toggle",
        EditAction::ToggleAlwaysShowControls => "Always Show Controls Clicked Toggle",
        EditAction::ReloadWorkspaceSettings => "Workspace Settings Clicked Reload",
        EditAction::SwitchWorkspace(_) => "Workspace Clicked Switch",
        EditAction::RestoreWorkspace(_) => "Workspace Clicked Restore",
        EditAction::SetTrashRetention(_) => "Workspace Trash Retention Clicked Change",
        EditAction::RemoveWorkspace(_) => "Workspace Clicked Confirm Removal",
        EditAction::ConfirmRemoveWorkspace => "Workspace Clicked Remove",
        EditAction::CancelRemoveWorkspace => "Workspace Clicked Keep",
        EditAction::AddSand(_) => "Sand Store Clicked Add Sand",
        EditAction::RemoveSand(_) => "Sand Clicked Remove",
        EditAction::EditSand(_) => "Sand Clicked Edit Text",
        EditAction::Text(_) => "Text Properties Clicked Change",
        EditAction::Credits => "Credits Clicked Show",
        EditAction::General => "General Settings Clicked Show",
        EditAction::Workspaces => "Workspaces Clicked Show",
        EditAction::Store => "Sand Store Clicked Show",
        EditAction::Canvas => "Canvas Settings Clicked Show",
        EditAction::Customization => "Global Customization Panel Toggle",
        EditAction::Information => "Information Clicked Show",
        EditAction::Shortcuts => "Cheat Sheet Clicked Show",
        EditAction::Park => "Park Settings Clicked Show",
        EditAction::Areas => "Areas of Influence Clicked Show",
        EditAction::Area(_) => "Area of Influence Clicked Edit",
        EditAction::ResetCanvasColors => "Canvas Colors Clicked Reset",
        EditAction::CanvasColor(..) => "Canvas Color Clicked Change",
    };
    Some(Connection {
        target,
        name: name.into(),
    })
}

pub(crate) mod tests {
    use super::*;
    use bevy::{
        camera::NormalizedRenderTarget, ecs::system::RunSystemOnce, picking::backend::HitData,
    };

    #[cfg_attr(test, test)]
    fn inspection_defaults_are_off_and_click_controls_selection() {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<HoverMap>()
            .add_message::<PointerInput>()
            .add_plugins((
                crate::native::workspace::WorkspacePlugin,
                crate::native::edit_mode::EditModePlugin,
                InspectionPlugin,
            ));
        let root = app
            .world_mut()
            .spawn(crate::native::container::BoxRoot)
            .id();
        app.update();
        EditAction::Open.apply(app.world_mut(), root);
        let target = app.world_mut().spawn((Square, ChildOf(root))).id();
        app.world_mut()
            .resource_mut::<HoverMap>()
            .entry(PointerId::Mouse)
            .or_default()
            .insert(target, HitData::new(root, 0.0, None, None));
        let press = PointerInput::new(
            PointerId::Mouse,
            bevy::picking::pointer::Location {
                target: NormalizedRenderTarget::None {
                    width: 800,
                    height: 600,
                },
                position: Vec2::new(220.0, 180.0),
            },
            PointerAction::Press(PointerButton::Primary),
        );

        let state = app.world().get::<Inspection>(root).unwrap();
        assert!(!state.click);
        assert!(!state.hover);
        assert!(!state.contours);
        assert!(!state.events);
        assert!(!state.hidden);
        app.world_mut().write_message(press.clone());
        app.update();
        assert_eq!(app.world().get::<Inspection>(root).unwrap().selected, None);
        assert_eq!(
            app.world().get::<Inspection>(root).unwrap().selected_point,
            Some(Vec2::new(220.0, 180.0))
        );

        app.world_mut().get_mut::<Inspection>(root).unwrap().click = true;
        app.world_mut().write_message(press);
        app.update();
        assert_eq!(
            app.world().get::<Inspection>(root).unwrap().selected,
            Some(target)
        );
    }

    #[cfg_attr(test, test)]
    fn built_in_inspection_opt_out_is_inherited_without_hiding_authored_sands() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let root = world.spawn_empty().id();
        let target = world.spawn(Square).id();
        let panel = world.spawn((InspectionExcluded, ChildOf(root))).id();
        let control = world
            .spawn((
                Square,
                EventConnections(vec![Connection {
                    target,
                    name: "Control".into(),
                }]),
                ChildOf(panel),
            ))
            .id();
        let authored = world
            .spawn((
                Square,
                EventConnections(vec![Connection {
                    target,
                    name: "Authored".into(),
                }]),
                ChildOf(root),
            ))
            .id();
        assert!(excluded(&world, control));
        assert!(connections(&world, control).is_empty());
        assert!(!excluded(&world, authored));
        assert_eq!(connections(&world, authored).len(), 1);
    }

    #[cfg_attr(test, test)]
    fn selection_includes_nested_castles_and_siblings_but_not_other_groups() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let root = world.spawn_empty().id();
        let outer = world.spawn((Castle, ChildOf(root))).id();
        let inner = world.spawn((Castle, ChildOf(outer))).id();
        let sand = world.spawn((Square, ChildOf(inner))).id();
        let sibling = world.spawn((Square, ChildOf(outer))).id();
        let other = world.spawn((Square, ChildOf(root))).id();
        let mut members = HashSet::new();
        family(&world, root, sand, &mut members);
        assert_eq!(members, HashSet::from([outer, inner, sand, sibling]));
        assert!(!members.contains(&root));
        assert!(!members.contains(&other));
    }

    #[cfg_attr(test, test)]
    fn action_sequences_describe_their_real_targets_without_executing() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let target = world
            .spawn(crate::native::canvas::CanvasView::default())
            .id();
        let button = world
            .spawn(ActionButton::new(
                target,
                crate::native::actions![
                    crate::native::canvas_controls::CanvasAction::ZoomIn,
                    crate::native::canvas_controls::CanvasAction::Recenter,
                ],
            ))
            .id();
        let links = connections(&world, button);
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.target == target));
        assert_eq!(links[0].name, "Zoom in Clicked");
        assert_eq!(
            world
                .get::<crate::native::canvas::CanvasView>(target)
                .unwrap()
                .zoom,
            1.0
        );
    }

    #[cfg_attr(test, test)]
    fn disconnected_boxes_do_not_appear_as_working_event_routes() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let first = world.spawn(crate::native::container::BoxRoot).id();
        let second = world.spawn(crate::native::container::BoxRoot).id();
        let target = world
            .spawn((Square, crate::native::sand::InBox(second), ChildOf(second)))
            .id();
        let source = world
            .spawn((
                Square,
                crate::native::sand::InBox(first),
                ChildOf(first),
                crate::native::effect::SendBoxEvent {
                    box_entity: first,
                    square: target,
                },
            ))
            .id();
        assert!(connections(&world, source).is_empty());
        assert!(!active_member(&world, first, target));
        world
            .entity_mut(target)
            .insert((crate::native::sand::InBox(first), ChildOf(first)));
        assert_eq!(connections(&world, source)[0].target, target);
        assert!(active_member(&world, first, target));
    }

    #[cfg_attr(test, test)]
    fn hidden_preview_reads_descendants_without_revealing_them() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let parent = world.spawn((Square, Visibility::Hidden)).id();
        let child = world
            .spawn((Text::new("Private draft"), ChildOf(parent)))
            .id();
        assert!(hidden(&world, child));
        assert_eq!(
            Preview::default().text(&world, parent),
            "Hidden: Private draft"
        );
        assert_eq!(world.get::<Visibility>(parent), Some(&Visibility::Hidden));
        assert_eq!(world.get::<Text>(child).unwrap().0, "Private draft");
    }

    crate::native::laboratory_cases! {
        inspection_defaults_are_off_and_click_controls_selection,
        built_in_inspection_opt_out_is_inherited_without_hiding_authored_sands,
        selection_includes_nested_castles_and_siblings_but_not_other_groups,
        action_sequences_describe_their_real_targets_without_executing,
        disconnected_boxes_do_not_appear_as_working_event_routes,
        hidden_preview_reads_descendants_without_revealing_them,
        click_off_hides_inspection_even_when_canvas_selection_sets_a_target,
    }

    #[cfg_attr(test, test)]
    fn click_off_hides_inspection_even_when_canvas_selection_sets_a_target() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        EditAction::Open.apply(app.world_mut(), root);
        let world = app.world_mut();
        world.init_resource::<Drawing>();
        let sand = world
            .spawn((
                Square,
                ChildOf(root),
                ComputedNode {
                    size: Vec2::splat(100.0),
                    ..default()
                },
                crate::native::icons::Tooltip("A Sand tooltip".into()),
            ))
            .id();
        let button = world
            .spawn((
                Square,
                ChildOf(sand),
                ComputedNode {
                    size: Vec2::splat(40.0),
                    ..default()
                },
                EventConnections(vec![Connection {
                    target: sand,
                    name: "Button Clicked".into(),
                }]),
            ))
            .id();
        world.get_mut::<Inspection>(root).unwrap().selected = Some(sand);
        world.run_system_once(draw).unwrap();
        assert!(world.resource::<Drawing>().marks.is_empty());
        let click = Setting { root, index: 0 };
        click.apply(world, root);
        world.run_system_once(draw).unwrap();
        assert!(
            world
                .resource::<Drawing>()
                .marks
                .iter()
                .any(|mark| matches!(mark, Mark::Line(..)))
        );
        assert!(!world.resource::<Drawing>().entities.is_empty());
        click.apply(world, root);
        world.get_mut::<Inspection>(root).unwrap().selected = Some(button);
        world.run_system_once(draw).unwrap();
        assert!(world.resource::<Drawing>().marks.is_empty());
        assert!(world.resource::<Drawing>().entities.is_empty());
        world.get_mut::<Inspection>(root).unwrap().hover = true;
        world.get_mut::<Inspection>(root).unwrap().hovered = Some(sand);
        world.run_system_once(draw).unwrap();
        assert!(!world.resource::<Drawing>().marks.is_empty());
        world.get_mut::<Inspection>(root).unwrap().hover = false;
        world.run_system_once(draw).unwrap();
        assert!(world.resource::<Drawing>().marks.is_empty());
    }
}
