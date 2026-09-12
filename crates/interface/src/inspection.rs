use crate::{
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
use std::collections::HashSet;

#[derive(Component, Clone)]
pub struct Inspection {
    pub hover: bool,
    pub contours: bool,
    pub events: bool,
    pub hidden: bool,
    pub selected: Option<Entity>,
    pub(crate) hovered: Option<Entity>,
    suppressed: Option<Entity>,
}

impl Default for Inspection {
    fn default() -> Self {
        Self {
            hover: true,
            contours: false,
            events: false,
            hidden: false,
            selected: None,
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
                0 => state.hover = !state.hover,
                1 => state.contours = !state.contours,
                2 => state.events = !state.events,
                _ => state.hidden = !state.hidden,
            }
        }
    }
}

pub struct Deselect;

impl Action for Deselect {
    fn apply(&self, world: &mut World, root: Entity) {
        crate::canvas_selection::clear(world, root);
        if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(root)
            && editor.tool.is_some()
        {
            editor.cancel();
            crate::edit_mode::render_panel(world, root);
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
    crate::edit_mode::label(
        world,
        panel,
        "Ctrl + right-drag selects Sands. Group and Ungroup are beside the selection.",
        12.0,
    );
    for (index, name) in ["Hover", "Groups", "Events", "Hidden"]
        .into_iter()
        .enumerate()
    {
        let setting = Setting { root, index };
        let button = world
            .spawn((
                crate::sand::button(0),
                setting,
                ActionButton::new(root, crate::actions![setting]),
                ChildOf(row),
                Node {
                    padding: UiRect::axes(px(5), px(5)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                crate::token_style::border(crate::tokens::Token::Accent),
                crate::token_style::background(crate::tokens::Token::Surface),
            ))
            .id();
        crate::edit_mode::label(world, button, name, 12.0);
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
                    .after(crate::actions::ApplyActions)
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
            With<crate::canvas::CanvasItem>,
        )>,
    >,
    settings: Query<(), Or<(With<Setting>, With<crate::sand_placement::PlacementMenu>)>>,
    excluded: Query<(), With<InspectionExcluded>>,
    mut roots: Query<(Entity, &EditMode, &mut Inspection)>,
) {
    let pressed = events.read().any(|event| {
        event.pointer_id == PointerId::Mouse
            && matches!(event.action, PointerAction::Press(PointerButton::Primary))
    });
    let hit = hover.get(&PointerId::Mouse).and_then(|hits| {
        hits.iter()
            .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
            .map(|(entity, _)| *entity)
    });
    let mut chain = Vec::new();
    let mut cursor = hit;
    while let Some(entity) = cursor {
        chain.push(entity);
        cursor = parents.get(entity).ok().map(ChildOf::parent);
    }
    let control = chain.iter().any(|entity| settings.contains(*entity));
    let excluded = chain.iter().any(|entity| excluded.contains(*entity));
    for (root, mode, mut state) in &mut roots {
        if !mode.enabled {
            state.selected = None;
            state.hovered = None;
            state.suppressed = None;
            continue;
        }
        if control {
            continue;
        }
        if excluded {
            state.hovered = None;
            continue;
        }
        let candidate = (!control && !excluded && chain.contains(&root))
            .then(|| {
                chain
                    .iter()
                    .copied()
                    .find(|entity| *entity != root && candidates.contains(*entity))
            })
            .flatten();
        if candidate != state.suppressed {
            state.suppressed = None;
        }
        state.hovered = candidate.filter(|entity| Some(*entity) != state.suppressed);
        if pressed && !control && chain.contains(&root) {
            state.selected = candidate;
            state.suppressed = None;
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
        let enabled = [state.hover, state.contours, state.events, state.hidden][setting.index];
        let name = ["Hover", "Groups", "Events", "Hidden"][setting.index];
        let value = format!("{name}: {}", if enabled { "on" } else { "off" });
        for child in children {
            if let Ok(mut text) = labels.get_mut(*child)
                && text.0 != value
            {
                text.0.clone_from(&value);
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

fn family(world: &World, root: Entity, entity: Entity) -> HashSet<Entity> {
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
    let mut result = HashSet::new();
    descendants(world, outer, &mut result);
    for member in crate::canvas_selection::group_members(world, root, outer) {
        descendants(world, member, &mut result);
    }
    result
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
        .get::<crate::workspace::Workspaces>(root)
        .map(|spaces| spaces.active);
    let mut cursor = Some(entity);
    while let Some(entity) = cursor {
        if entity == root {
            return true;
        }
        if let Some(member) = world.get::<crate::workspace::WorkspaceMember>(entity)
            && active.is_some_and(|active| member.0 != active)
        {
            return false;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

fn preview(world: &World, entity: Entity) -> String {
    let mut members = HashSet::new();
    descendants(world, entity, &mut members);
    let mut members: Vec<_> = members.into_iter().collect();
    members.sort();
    let text = members
        .iter()
        .filter_map(|entity| {
            world
                .get::<Text>(*entity)
                .map(|text| text.0.clone())
                .or_else(|| {
                    world
                        .get::<EditableText>(*entity)
                        .map(|text| text.value().to_string())
                })
        })
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        "Hidden Sand".into()
    } else {
        format!("Hidden: {}", text.chars().take(160).collect::<String>())
    }
}

pub(crate) fn bounds(world: &World, entity: Entity) -> Option<Rect> {
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

fn connections(world: &World, entity: Entity) -> Vec<Connection> {
    if excluded(world, entity) {
        return Vec::new();
    }
    let mut links = world
        .get::<EventConnections>(entity)
        .map(|links| links.0.clone())
        .unwrap_or_default();
    if let Some(effect) = world.get::<crate::effect::SendBoxEvent>(entity)
        && world.get::<Square>(entity).is_some()
        && world
            .get::<crate::container::BoxRoot>(effect.box_entity)
            .is_some()
        && world
            .get::<crate::sand::InBox>(entity)
            .is_some_and(|owner| owner.0 == effect.box_entity)
        && world
            .get::<crate::sand::InBox>(effect.square)
            .is_some_and(|owner| owner.0 == effect.box_entity)
        && world.get::<Square>(effect.square).is_some()
    {
        links.push(Connection {
            target: effect.square,
            name: "Sand Clicked Toggle".into(),
        });
    }
    if let Some(button) = world.get::<ActionButton>(entity) {
        let described = button.actions.connections(world, button.target);
        if described.is_empty() {
            let label = world
                .get::<crate::icons::IconButton>(entity)
                .map(|icon| icon.label.as_str())
                .or_else(|| {
                    world
                        .get::<crate::icons::Tooltip>(entity)
                        .map(|tip| tip.0.as_str())
                })
                .unwrap_or("Button");
            links.push(Connection {
                target: button.target,
                name: format!("{label} Clicked"),
            });
        } else {
            links.extend(described);
        }
    }
    if let Some(record) = world.get::<crate::record_view::RecordEditor>(entity) {
        links.push(Connection {
            target: record.status,
            name: "Record Text Changed Save".into(),
        });
    }
    links
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
        EditAction::ReloadWorkspaceSettings => "Workspace Settings Clicked Reload",
        EditAction::DisarmAreaChanges => "Area Record Changes Clicked Disarm All",
        EditAction::SwitchWorkspace(_) => "Workspace Clicked Switch",
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
        EditAction::Notifications => "Notifications Clicked Show",
        EditAction::Information => "Information Clicked Show",
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

#[derive(Clone, PartialEq)]
enum Mark {
    Contour(Rect, BorderRadius, bool),
    Line(Vec2, Vec2),
    Label(Vec2, String, bool),
}

#[derive(Resource, Default)]
struct Drawing {
    marks: Vec<Mark>,
    entities: Vec<Entity>,
}

fn draw(world: &mut World) {
    let roots: Vec<_> = world
        .query::<(Entity, &EditMode, &Inspection)>()
        .iter(world)
        .filter(|(_, mode, _)| mode.enabled)
        .map(|(entity, _, state)| (entity, state.clone()))
        .collect();
    let mut marks = Vec::new();
    for (root, state) in roots {
        let focus = state
            .selected
            .filter(|entity| world.get_entity(*entity).is_ok())
            .or(if state.hover { state.hovered } else { None });
        let related = focus
            .map(|entity| family(world, root, entity))
            .unwrap_or_default();
        let mut all = HashSet::new();
        descendants(world, root, &mut all);
        let mut entities: Vec<_> = all.into_iter().collect();
        entities.sort();
        let mut contoured = HashSet::new();
        let mut previews = HashSet::new();
        for entity in entities {
            if entity == root
                || world.get::<crate::area::InfluenceArea>(entity).is_some()
                || excluded(world, entity)
                || world.get::<Setting>(entity).is_some()
                || !active_member(world, root, entity)
            {
                continue;
            }
            let local = related.contains(&entity);
            let invisible = hidden(world, entity);
            let rect = bounds(world, entity);
            let sand = world.get::<Square>(entity).is_some()
                || world.get::<ImageSand>(entity).is_some()
                || world.get::<Text>(entity).is_some()
                || world.get::<EditableText>(entity).is_some()
                || world.get::<Castle>(entity).is_some()
                || world.get::<crate::canvas::CanvasItem>(entity).is_some();
            if sand
                && (((state.contours || local) && !invisible)
                    || (invisible && (state.hidden || local)))
                && let Some(rect) = rect
                && contoured.insert(entity)
            {
                let radius = world
                    .get::<Node>(entity)
                    .map(|node| node.border_radius)
                    .unwrap_or_default();
                let mut margin = 0.0;
                if world.get::<Castle>(entity).is_some() {
                    let mut depth = 1;
                    let mut cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                    while let Some(parent) = cursor {
                        if world.get::<Castle>(parent).is_some() {
                            depth += 1;
                        }
                        cursor = world.get::<ChildOf>(parent).map(ChildOf::parent);
                    }
                    margin = 6.0 / depth as f32;
                }
                marks.push(Mark::Contour(
                    Rect::from_corners(
                        rect.min - Vec2::splat(margin),
                        rect.max + Vec2::splat(margin),
                    ),
                    radius,
                    invisible,
                ));
            }
            let links = connections(world, entity);
            for link in links {
                if !active_member(world, root, link.target)
                    || excluded(world, link.target)
                    || world.get_entity(link.target).is_err()
                    || !(state.events || state.hidden || local || related.contains(&link.target))
                {
                    continue;
                }
                let Some(source) = rect else { continue };
                let destination = bounds(world, link.target).unwrap_or(Rect::from_center_size(
                    source.center() + Vec2::new(-100.0, -60.0),
                    Vec2::new(160.0, 50.0),
                ));
                if state.events || local || related.contains(&link.target) {
                    let radius = world
                        .get::<Node>(link.target)
                        .map(|node| node.border_radius)
                        .unwrap_or_default();
                    if contoured.insert(link.target) {
                        marks.push(Mark::Contour(
                            destination,
                            radius,
                            hidden(world, link.target),
                        ));
                    }
                    marks.push(Mark::Line(source.center(), destination.center()));
                    marks.push(Mark::Label(
                        (source.center() + destination.center()) * 0.5,
                        link.name,
                        false,
                    ));
                }
                if hidden(world, link.target)
                    && (state.hidden || local)
                    && previews.insert(link.target)
                {
                    if contoured.insert(link.target) {
                        marks.push(Mark::Contour(destination, BorderRadius::all(px(4)), true));
                    }
                    marks.push(Mark::Label(
                        destination.min,
                        preview(world, link.target),
                        true,
                    ));
                }
            }
            if (state.hidden || local)
                && !invisible
                && let Some(rect) = rect
                && let Some(tip) = world.get::<crate::icons::Tooltip>(entity)
                && !tip.0.is_empty()
            {
                let position = rect.min + Vec2::new(0.0, -48.0);
                marks.push(Mark::Label(position, format!("Tooltip: {}", tip.0), true));
                if state.events || local {
                    marks.push(Mark::Line(rect.center(), position));
                    marks.push(Mark::Label(
                        (rect.center() + position) * 0.5,
                        "Sand Hovered Show Tooltip".into(),
                        false,
                    ));
                }
            }
        }
    }
    if world.resource::<Drawing>().marks == marks {
        return;
    }
    let previous = std::mem::take(&mut world.resource_mut::<Drawing>().entities);
    for entity in previous {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    let mut entities = Vec::new();
    for mark in &marks {
        let mut node = Node {
            position_type: PositionType::Absolute,
            ..default()
        };
        let mut transform = UiTransform::default();
        match mark {
            Mark::Contour(rect, radius, _) => {
                node.left = px(rect.min.x);
                node.top = px(rect.min.y);
                node.width = px(rect.width());
                node.height = px(rect.height());
                node.border = UiRect::all(px(1));
                node.border_radius = *radius;
            }
            Mark::Line(start, end) => {
                let delta = *end - *start;
                let center = (*start + *end) * 0.5;
                node.left = px(center.x - delta.length() * 0.5);
                node.top = px(center.y - 0.5);
                node.width = px(delta.length());
                node.height = px(1);
                transform.rotation = Rot2::radians(delta.y.atan2(delta.x));
            }
            Mark::Label(position, _, _) => {
                node.left = px(position.x.max(0.0));
                node.top = px(position.y.max(0.0));
                node.max_width = px(230);
                node.padding = UiRect::all(px(3));
            }
        }
        let entity = world
            .spawn((
                InspectionOverlay,
                node,
                transform,
                Pickable::IGNORE,
                GlobalZIndex(90),
                crate::token_style::border(crate::tokens::Token::Connections),
            ))
            .id();
        match mark {
            Mark::Line(..) => {
                world
                    .entity_mut(entity)
                    .insert(crate::token_style::background(
                        crate::tokens::Token::Connections,
                    ));
            }
            Mark::Contour(_, _, true) => {
                world
                    .entity_mut(entity)
                    .insert(crate::token_style::background(
                        crate::tokens::Token::ConnectionFill,
                    ));
            }
            Mark::Label(_, value, ghost) => {
                let font = world
                    .resource::<Typography>()
                    .text(if *ghost { 14.0 } else { 11.0 });
                world.entity_mut(entity).insert((
                    Text::new(value),
                    font,
                    crate::token_style::text(if *ghost {
                        crate::tokens::Token::Connections
                    } else {
                        crate::tokens::Token::Ink
                    }),
                    crate::token_style::background(crate::tokens::Token::Surface),
                ));
            }
            _ => {}
        }
        entities.push(entity);
    }
    *world.resource_mut::<Drawing>() = Drawing { marks, entities };
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn built_in_inspection_opt_out_is_inherited_without_hiding_authored_sands() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
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
        crate::laboratory::isolate(&mut world);
        let root = world.spawn_empty().id();
        let outer = world.spawn((Castle, ChildOf(root))).id();
        let inner = world.spawn((Castle, ChildOf(outer))).id();
        let sand = world.spawn((Square, ChildOf(inner))).id();
        let sibling = world.spawn((Square, ChildOf(outer))).id();
        let other = world.spawn((Square, ChildOf(root))).id();
        let members = family(&world, root, sand);
        assert_eq!(members, HashSet::from([outer, inner, sand, sibling]));
        assert!(!members.contains(&root));
        assert!(!members.contains(&other));
    }

    #[cfg_attr(test, test)]
    fn action_sequences_describe_their_real_targets_without_executing() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        let target = world.spawn(crate::canvas::CanvasView::default()).id();
        let button = world
            .spawn(ActionButton::new(
                target,
                crate::actions![
                    crate::canvas_controls::CanvasAction::ZoomIn,
                    crate::canvas_controls::CanvasAction::Recenter,
                ],
            ))
            .id();
        let links = connections(&world, button);
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.target == target));
        assert_eq!(links[0].name, "Zoom in Clicked");
        assert_eq!(
            world.get::<crate::canvas::CanvasView>(target).unwrap().zoom,
            1.0
        );
    }

    #[cfg_attr(test, test)]
    fn disconnected_boxes_do_not_appear_as_working_event_routes() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        let first = world.spawn(crate::container::BoxRoot).id();
        let second = world.spawn(crate::container::BoxRoot).id();
        let target = world
            .spawn((Square, crate::sand::InBox(second), ChildOf(second)))
            .id();
        let source = world
            .spawn((
                Square,
                crate::sand::InBox(first),
                ChildOf(first),
                crate::effect::SendBoxEvent {
                    box_entity: first,
                    square: target,
                },
            ))
            .id();
        assert!(connections(&world, source).is_empty());
        assert!(!active_member(&world, first, target));
        world
            .entity_mut(target)
            .insert((crate::sand::InBox(first), ChildOf(first)));
        assert_eq!(connections(&world, source)[0].target, target);
        assert!(active_member(&world, first, target));
    }

    #[cfg_attr(test, test)]
    fn hidden_preview_reads_descendants_without_revealing_them() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        let parent = world.spawn((Square, Visibility::Hidden)).id();
        let child = world
            .spawn((Text::new("Private draft"), ChildOf(parent)))
            .id();
        assert!(hidden(&world, child));
        assert_eq!(preview(&world, parent), "Hidden: Private draft");
        assert_eq!(world.get::<Visibility>(parent), Some(&Visibility::Hidden));
        assert_eq!(world.get::<Text>(child).unwrap().0, "Private draft");
    }

    crate::laboratory_cases! {
        built_in_inspection_opt_out_is_inherited_without_hiding_authored_sands,
        selection_includes_nested_castles_and_siblings_but_not_other_groups,
        action_sequences_describe_their_real_targets_without_executing,
        disconnected_boxes_do_not_appear_as_working_event_routes,
        hidden_preview_reads_descendants_without_revealing_them,
    }
}
