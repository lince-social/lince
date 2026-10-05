use crate::{
    actions::{Action, ActionButton},
    canvas::{CanvasItem, CanvasView},
    inspection::Inspection,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pinned {
    pub anchor: [f64; 2],
    pub scale: f64,
}

impl Pinned {
    pub(crate) fn valid(&self) -> bool {
        DVec2::from_array(self.anchor).is_finite()
            && (CanvasView::MIN_ZOOM..=CanvasView::MAX_ZOOM).contains(&self.scale)
    }

    pub(crate) fn view(&self, item: &CanvasItem, viewport: Vec2) -> CanvasView {
        CanvasView {
            center: item.position
                - (DVec2::from_array(self.anchor) - DVec2::splat(0.5)) * viewport.as_dvec2()
                    / self.scale,
            zoom: self.scale,
        }
    }

    pub(crate) fn moved(&mut self, delta: DVec2, viewport: Vec2) {
        let anchor = DVec2::from_array(self.anchor) + delta / viewport.as_dvec2();
        if anchor.is_finite() {
            self.anchor = anchor.to_array();
        }
    }
}

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct Placement {
    #[serde(default)]
    pub practice_source: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub canvas_component: Option<nucleus::canvas::Component>,
    #[serde(default)]
    pub backend_component: Option<crate::component_push::Placed>,
    #[serde(default)]
    pub layout: Option<crate::layout::LayoutBox>,
    #[serde(default)]
    pub events: crate::scoped_events::EventBoundary,
    pub pinned: Option<Pinned>,
    pub order: i32,
    #[serde(default)]
    pub group: Option<crate::canvas_selection::SandGroup>,
    #[serde(default)]
    pub spatial: crate::topology::Spatial,
    #[serde(default)]
    pub attachment: Option<crate::topology::Attachment>,
    #[serde(default)]
    pub group_pose: Option<crate::topology::groups::GroupPose>,
}

impl Placement {
    pub(crate) fn capture(world: &World, entity: Entity) -> Self {
        let mut backend_component = world.get::<crate::component_push::Placed>(entity).cloned();
        if let Some(component) = &mut backend_component
            && let Some(composition) = crate::component_push::composition::capture(world, entity)
        {
            component.component = nucleus::component::ComponentState::Composition { composition };
        }
        Self {
            practice_source: world.get::<crate::practice_cells::PracticeSource>(entity).map(|source| source.0.clone()),
            identity: world.get::<crate::canvas_host::Identity>(entity).map(|id| id.0.clone()),
            canvas_component: crate::canvas_host::composition::capture(world, entity).map(|composition| nucleus::canvas::Component::Composition { composition }).or_else(|| world.get::<crate::canvas_host::Content>(entity).map(|content| content.0.clone())),
            backend_component,
            layout: world.get::<crate::layout::LayoutBox>(entity).copied(),
            events: world
                .get::<crate::scoped_events::EventBoundary>(entity)
                .cloned()
                .unwrap_or_default(),
            pinned: world.get::<Pinned>(entity).copied(),
            spatial: crate::topology::spatial(world, entity),
            attachment: world.get::<crate::topology::Attachment>(entity).copied(),
            group_pose: world
                .get::<crate::topology::groups::GroupPose>(entity)
                .copied(),
            group: world
                .get::<crate::canvas_selection::SandGroup>(entity)
                .copied(),
            order: world.get::<ZIndex>(entity).map_or(0, |index| index.0),
        }
    }

    pub(crate) fn restore(self, world: &mut World, entity: Entity) {
        if let Some(source) = self.practice_source { world.entity_mut(entity).insert(crate::practice_cells::PracticeSource(source)); }
        if let Some(identity) = self.identity { world.entity_mut(entity).insert(crate::canvas_host::Identity(identity)); }
        if let Some(component) = self.canvas_component {
            if let Err(error) = crate::canvas_host::restore_content(world, entity, &component) { crate::notifications::report(world, "interface::canvas", &error); }
            world.entity_mut(entity).insert(crate::canvas_host::Content(component));
        }
        if let Some(component) = self.backend_component {
            if let nucleus::component::ComponentState::Composition { composition } = &component.component {
                if let Err(error) = crate::component_push::composition::populate(world, entity, composition.clone()) {
                    crate::notifications::report(world, "interface::components", &error);
                }
            }
            world.entity_mut(entity).insert(component);
        }
        if let Some(layout) = self.layout {
            world.entity_mut(entity).insert(layout);
            if world.get::<crate::area::InfluenceArea>(entity).is_some() {
                world.entity_mut(entity).insert(Pickable::default());
            }
        }
        world.entity_mut(entity).insert(self.events);
        world.entity_mut(entity).insert(ZIndex(self.order));
        world.entity_mut(entity).insert(self.spatial);
        if let Some(pose) = self.group_pose {
            world.entity_mut(entity).insert(pose);
        }
        if let Some(attachment) = self.attachment {
            world.entity_mut(entity).insert(attachment);
        }
        if let Some(group) = self.group {
            world.entity_mut(entity).insert(group);
        }
        if let Some(pinned) = self.pinned {
            world.entity_mut(entity).insert((pinned, GlobalZIndex(1)));
        }
    }

    pub(crate) fn valid(&self) -> bool {
        self.practice_source.as_ref().is_none_or(|source| nucleus::valid_uid(source, "g"))
            && self.identity.as_ref().is_none_or(|id| nucleus::valid_uid(id, "placement"))
            && self.canvas_component.as_ref().is_none_or(|component| component.validate_snapshot(&crate::canvas_host::registry()).is_ok())
            && self.backend_component.as_ref().is_none_or(crate::component_push::Placed::valid)
            && self.pinned.is_none_or(|pin| pin.valid())
            && self.layout.is_none_or(|layout| layout.valid())
            && self.events.valid()
            && self.spatial.valid()
            && self.group_pose.is_none_or(|pose| pose.valid())
            && self.attachment.is_none_or(|attachment| attachment.valid())
    }
}

#[derive(Clone, Copy)]
pub enum PlacementAction {
    Delete,
    Pin,
    Front,
    Forward,
    Backward,
    Back,
}

impl Action for PlacementAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: match self {
                Self::Delete => "Canvas Item Clicked Delete",
                Self::Pin => "Sand Clicked Toggle Pin",
                Self::Front => "Sand Clicked Bring To Front",
                Self::Forward => "Sand Clicked Bring Forward",
                Self::Backward => "Sand Clicked Send Backward",
                Self::Back => "Sand Clicked Send To Back",
            }
            .into(),
        }]
    }

    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            return;
        };
        if matches!(self, Self::Delete) {
            if !active(world, parent, entity)
                || !world
                    .get::<crate::edit_mode::EditMode>(parent)
                    .is_some_and(|mode| mode.enabled)
            {
                return;
            }
            let targets = crate::canvas_selection::companions(world, parent, entity);
            crate::deletion::request(world, parent, targets);
            return;
        }
        if world.get::<crate::area::InfluenceArea>(entity).is_some() {
            return;
        }
        if world.get::<CanvasItem>(entity).is_none() {
            return;
        }
        let Some(view) = world.get::<CanvasView>(parent).copied() else {
            return;
        };
        if !view.center.is_finite()
            || !view.zoom.is_finite()
            || view.zoom <= 0.0
            || !active(world, parent, entity)
        {
            return;
        }
        if !world
            .get::<crate::edit_mode::EditMode>(parent)
            .is_some_and(|mode| mode.enabled)
        {
            return;
        }
        if matches!(self, Self::Pin) {
            if world
                .get::<crate::topology::presentation::SpatialRoot>(parent)
                .is_some()
                && (world
                    .get::<crate::canvas_selection::SandGroup>(entity)
                    .is_some()
                    || world
                        .get::<crate::topology::assets::ImportedAsset>(entity)
                        .is_some())
            {
                crate::notifications::report(
                    world,
                    "Topology",
                    "Screen pinning is available for individual content Sands. Use world pinning for this object.",
                );
                return;
            }
            if crate::layout::linked(world, entity) && world.get::<Pinned>(entity).is_none() {
                return;
            }
            let Some(viewport) = crate::inspection::bounds(world, parent).map(|rect| rect.size())
            else {
                return;
            };
            if !viewport.is_finite() || viewport.min_element() <= 0.0 {
                return;
            }
            let members = crate::canvas_selection::companions(world, parent, entity);
            let unpin = members
                .iter()
                .all(|member| world.get::<Pinned>(*member).is_some());
            for member in members {
                if unpin {
                    let pin = world.get::<Pinned>(member).unwrap();
                    let position = view.center
                        + (DVec2::from_array(pin.anchor) - DVec2::splat(0.5)) * viewport.as_dvec2()
                            / view.zoom;
                    world.get_mut::<CanvasItem>(member).unwrap().position = position;
                    world.entity_mut(member).remove::<Pinned>();
                    world.entity_mut(member).remove::<GlobalZIndex>();
                } else if world.get::<Pinned>(member).is_none() {
                    let item = world.get::<CanvasItem>(member).unwrap();
                    let anchor = DVec2::splat(0.5)
                        + (item.position - view.center) * view.zoom / viewport.as_dvec2();
                    let pin = Pinned {
                        anchor: anchor.to_array(),
                        scale: view.zoom,
                    };
                    if pin.valid() {
                        world.entity_mut(member).insert((pin, GlobalZIndex(1)));
                    }
                }
            }
            return;
        }
        let workspace = world.get::<WorkspaceMember>(entity).map(|member| member.0);
        let pinned = world.get::<Pinned>(entity).is_some();
        let mut query =
            world.query::<(Entity, &ChildOf, Option<&WorkspaceMember>, Option<&ZIndex>)>();
        let mut siblings: Vec<_> = query
            .iter(world)
            .filter(|(candidate, child, member, _)| {
                child.parent() == parent
                    && world.get::<CanvasItem>(*candidate).is_some()
                    && world.get::<Pinned>(*candidate).is_some() == pinned
                    && member.map(|member| member.0) == workspace
            })
            .map(|(candidate, _, _, index)| (candidate, index.map_or(0, |index| index.0)))
            .collect();
        let children = world.get::<Children>(parent).unwrap();
        siblings.sort_by_key(|(candidate, index)| {
            (
                *index,
                children
                    .iter()
                    .position(|child| child == *candidate)
                    .unwrap_or(0),
            )
        });
        let Some(index) = siblings
            .iter()
            .position(|(candidate, _)| *candidate == entity)
        else {
            return;
        };
        let target = match self {
            Self::Front => siblings.len() - 1,
            Self::Forward => (index + 1).min(siblings.len() - 1),
            Self::Backward => index.saturating_sub(1),
            Self::Back => 0,
            Self::Pin | Self::Delete => unreachable!(),
        };
        let entry = siblings.remove(index);
        siblings.insert(target, entry);
        for (index, (candidate, _)) in siblings.into_iter().enumerate() {
            world.entity_mut(candidate).insert(ZIndex(index as i32));
        }
    }
}

#[derive(Component)]
pub(crate) struct PlacementMenu;

fn active(world: &World, root: Entity, entity: Entity) -> bool {
    world
        .get::<crate::workspace::Workspaces>(root)
        .is_none_or(|spaces| {
            world
                .get::<WorkspaceMember>(entity)
                .map_or(spaces.entries[0].id, |member| member.0)
                == spaces.active
        })
}

#[derive(Resource, Default)]
struct Menu {
    target: Option<Entity>,
    entity: Option<Entity>,
    pinned: bool,
    grouping: (bool, bool),
    layout_linked: bool,
    date_boundary: bool,
    click: Option<Vec2>,
    anchor: Vec2,
}

pub struct PlacementPlugin;

impl Plugin for PlacementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Menu>().add_systems(
            PostUpdate,
            menu.after(crate::actions::ApplyActions)
                .after(bevy::ui::UiSystems::PostLayout),
        );
    }
}

fn menu(world: &mut World) {
    let mut roots = world.query::<(Entity, &crate::edit_mode::EditMode, &Inspection)>();
    let candidate = roots.iter(world).find_map(|(root, mode, state)| {
        mode.enabled
            .then_some((
                root,
                state
                    .selected
                    .or(if state.hover { state.hovered } else { None }),
            ))
            .and_then(|(root, candidate)| candidate.map(|candidate| (root, candidate)))
    });
    let target = candidate.and_then(|(root, mut candidate)| {
        loop {
            let parent = world.get::<ChildOf>(candidate)?.parent();
            if parent == root {
                break world
                    .get::<CanvasItem>(candidate)
                    .map(|_| (root, candidate));
            }
            candidate = parent;
        }
    });
    let target = target.or_else(|| retained_menu_target(world));
    let target = target.filter(|(root, entity)| active(world, *root, *entity));
    let pinned = target.is_some_and(|(root, entity)| {
        let members = crate::canvas_selection::companions(world, root, entity);
        !members.is_empty()
            && members
                .iter()
                .all(|member| world.get::<Pinned>(*member).is_some())
    });
    let grouping = target.map_or((false, false), |(root, target)| {
        crate::canvas_selection::options(world, root, target)
    });
    let date_boundary = target.is_some_and(|(root, target)| {
        crate::canvas_selection::group_members(world, root, target)
            .iter()
            .any(|entity| {
                world
                    .get::<crate::scoped_events::EventBoundary>(*entity)
                    .is_some_and(|boundary| {
                        boundary
                            .0
                            .iter()
                            .any(|name| name == crate::calendar::DATE_SELECTED)
                    })
            })
    });
    let current = world.resource::<Menu>();
    let layout_linked = target.is_some_and(|(_, entity)| crate::layout::linked(world, entity));
    if current.target == target.map(|(_, entity)| entity)
        && current.pinned == pinned
        && current.grouping == grouping
        && current.layout_linked == layout_linked
        && current.date_boundary == date_boundary
        && current
            .entity
            .is_none_or(|entity| world.get_entity(entity).is_ok())
    {
        if let (Some((root, target)), Some(panel)) = (target, current.entity) {
            position_menu(world, root, target, panel);
        }
        return;
    }
    let anchor = current.anchor;
    let click = current.click;
    let same_target = current.target == target.map(|(_, entity)| entity);
    if let Some(entity) = world.resource_mut::<Menu>().entity.take() {
        world.despawn(entity);
    }
    *world.resource_mut::<Menu>() = Menu {
        target: target.map(|(_, entity)| entity),
        pinned,
        grouping,
        layout_linked,
        date_boundary,
        entity: None,
        click: if same_target { click } else { None },
        anchor: if same_target {
            anchor
        } else {
            Vec2::splat(0.5)
        },
    };
    let Some((root, target)) = target else { return };
    let area = world.get::<crate::area::InfluenceArea>(target).is_some();
    let deletable = crate::canvas_selection::eligible(world, root, target);
    let panel = world
        .spawn((
            PlacementMenu,
            crate::inspection::InspectionExcluded,
            crate::sand::Square,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(4)),
                column_gap: px(4),
                row_gap: px(4),
                border: UiRect::all(px(1)),
                ..default()
            },
            GlobalZIndex(23),
            ChildOf(root),
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
        ))
        .id();
    use crate::icons::{Icon, IconButton, IconStyle};
    world.spawn((
        IconButton::new(Icon::Grow, "Layout: size, growth, scrolling and parent"),
        IconStyle {
            size: 20.0,
            padding: 6.0,
            ..default()
        },
        ActionButton::new(
            target,
            crate::actions![crate::layout::panel::LayoutAction::Open],
        ),
        ChildOf(panel),
    ));
    if area {
        world.spawn((
            IconButton::new(Icon::General, "Configure this area and its behaviors"),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(
                root,
                crate::actions![crate::edit_mode::EditAction::Area(
                    crate::area_panel::AreaAction::Select(target)
                )],
            ),
            ChildOf(panel),
        ));
    }
    let mut buttons = Vec::new();
    if !area {
        buttons.extend([
            (
                PlacementAction::Pin,
                Icon::Pin,
                if pinned {
                    "Unpin from screen"
                } else {
                    "Pin to screen"
                },
            ),
            (PlacementAction::Front, Icon::BringHere, "Bring to front"),
            (PlacementAction::Forward, Icon::Forward, "Bring forward"),
            (PlacementAction::Backward, Icon::Backward, "Send backward"),
            (PlacementAction::Back, Icon::Back, "Send to back"),
        ]);
    }
    if deletable {
        buttons.push((PlacementAction::Delete, Icon::Delete, "Delete"));
    }
    for (action, icon, name) in buttons {
        if matches!(action, PlacementAction::Pin) && layout_linked && !pinned {
            continue;
        }
        world.spawn((
            IconButton::new(icon, name),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(target, crate::actions![action]),
            ChildOf(panel),
        ));
    }
    for (show, action, icon, name) in [
        (
            grouping.1,
            crate::canvas_selection::GroupAction::Detach,
            Icon::Detach,
            "Ungroup this component",
        ),
        (
            grouping.0,
            crate::canvas_selection::GroupAction::Group,
            Icon::Group,
            "Group selected Sands",
        ),
        (
            grouping.1,
            crate::canvas_selection::GroupAction::Ungroup,
            Icon::Ungroup,
            "Ungroup Sands",
        ),
    ] {
        if show {
            world.spawn((
                IconButton::new(icon, name),
                IconStyle {
                    size: 20.0,
                    padding: 6.0,
                    ..default()
                },
                ActionButton::new(target, crate::actions![action]),
                ChildOf(panel),
            ));
        }
    }
    if !area {
        world.spawn((
            IconButton::new(
                if date_boundary {
                    Icon::EventsLocal
                } else {
                    Icon::EventsShared
                },
                if date_boundary {
                    "Date events stay in this group. Click to let them leave."
                } else {
                    "Date events can leave this group. Click to keep them inside."
                },
            ),
            IconStyle {
                size: 20.0,
                padding: 6.0,
                ..default()
            },
            ActionButton::new(
                target,
                crate::actions![crate::scoped_events::ToggleDateBoundary],
            ),
            ChildOf(panel),
        ));
    }
    let controls: Vec<_> = world.get::<Children>(panel).unwrap().iter().collect();
    for control in controls {
        world.entity_mut(control).insert((
            crate::icons::InlineTooltip,
            crate::icons::TooltipIcon { source: control },
        ));
    }
    world.resource_mut::<Menu>().entity = Some(panel);
    if !same_target {
        let point = world
            .get::<Inspection>(root)
            .filter(|state| state.selected.is_some())
            .and_then(|state| state.selected_point)
            .or_else(|| {
                world
                    .query::<&Window>()
                    .iter(world)
                    .filter(|window| window.focused)
                    .find_map(Window::cursor_position)
            });
        set_menu_anchor(world, target, point);
    }
    position_menu(world, root, target, panel);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn menu_position(viewport: Rect, sand: Rect, size: Vec2, point: Vec2) -> Vec2 {
    let margin = Vec2::splat(8.0);
    let min = viewport.min + margin;
    let max = (viewport.max - size - margin).max(min);
    let point = point.clamp(sand.min, sand.max);
    let along = (point - size * 0.5).clamp(min, max);
    let positions = [
        Vec2::new(along.x, sand.min.y - size.y - margin.y),
        Vec2::new(along.x, sand.max.y + margin.y),
        Vec2::new(sand.min.x - size.x - margin.x, along.y),
        Vec2::new(sand.max.x + margin.x, along.y),
    ];
    let score = |position: Vec2| {
        let overflow = (min - position).max(Vec2::ZERO)
            + (position + size + margin - viewport.max).max(Vec2::ZERO);
        let nearest = point.clamp(position, position + size);
        (overflow.element_sum(), nearest.distance_squared(point))
    };
    positions
        .into_iter()
        .min_by(|a, b| {
            let a = score(*a);
            let b = score(*b);
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        })
        .unwrap()
        - viewport.min
}

fn set_menu_anchor(world: &mut World, target: Entity, point: Option<Vec2>) {
    let Some(root) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
        return;
    };
    let Some(sand) = menu_bounds(world, root, target) else {
        return;
    };
    let scale = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
    let anchor = point.map_or(Vec2::splat(0.5), |point| {
        ((point / scale - sand.min) / sand.size()).clamp(Vec2::ZERO, Vec2::ONE)
    });
    let mut menu = world.resource_mut::<Menu>();
    menu.anchor = anchor;
    menu.click = point;
}

fn menu_bounds(world: &World, root: Entity, target: Entity) -> Option<Rect> {
    crate::canvas_selection::group_members(world, root, target)
        .into_iter()
        .filter_map(|member| {
            crate::inspection::bounds(world, member)
                .or_else(|| crate::canvas_selection::screen_bounds(world, root, member))
        })
        .reduce(|a, b| Rect::from_corners(a.min.min(b.min), a.max.max(b.max)))
}

fn retained_menu_target(world: &mut World) -> Option<(Entity, Entity)> {
    let menu = world.resource::<Menu>();
    let target = menu.target?;
    let panel = menu.entity?;
    let root = world.get::<ChildOf>(target)?.parent();
    if !world.get::<crate::edit_mode::EditMode>(root)?.enabled {
        return None;
    }
    let sand = menu_bounds(world, root, target)?;
    let controls = crate::inspection::bounds(world, panel)?;
    let corridor = Rect::from_corners(sand.min.min(controls.min), sand.max.max(controls.max));
    world
        .query::<&Window>()
        .iter(world)
        .filter(|window| window.focused)
        .filter_map(Window::cursor_position)
        .any(|point| !sand.contains(point) && corridor.contains(point))
        .then_some((root, target))
}

fn position_menu(world: &mut World, root: Entity, target: Entity, panel: Entity) {
    let Some(viewport) = crate::inspection::bounds(world, root) else {
        return;
    };
    let Some(sand) = menu_bounds(world, root, target) else {
        return;
    };
    let click = world
        .get::<Inspection>(root)
        .filter(|state| state.selected.is_some())
        .and_then(|state| state.selected_point);
    if click.is_some() && click != world.resource::<Menu>().click {
        set_menu_anchor(world, target, click);
    }
    let size =
        crate::inspection::bounds(world, panel).map_or(Vec2::new(216.0, 48.0), |rect| rect.size());
    let point = sand.min + sand.size() * world.resource::<Menu>().anchor;
    let position = menu_position(viewport, sand, size, point);
    let controls = world.get::<Children>(panel).unwrap();
    let width = controls
        .iter()
        .map(|control| match world.get::<Node>(control).unwrap().width {
            Val::Px(width) => width,
            _ => 34.0,
        })
        .sum::<f32>()
        + 4.0 * controls.len().saturating_sub(1) as f32
        + 10.0;
    let width = px(width.min((viewport.width() - 16.0).max(44.0)));
    let mut node = world.get_mut::<Node>(panel).unwrap();
    if node.left != px(position.x) || node.top != px(position.y) || node.width != width {
        node.left = px(position.x);
        node.top = px(position.y);
        node.width = width;
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    } else if crate::inspection::bounds(world, panel).is_some() {
        world
            .get_mut::<Visibility>(panel)
            .unwrap()
            .set_if_neq(Visibility::Inherited);
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn controls_stay_outside_the_sand_near_the_clicked_edge() {
        let viewport = Rect::from_corners(Vec2::new(20.0, 30.0), Vec2::new(1220.0, 830.0));
        let sand = Rect::from_corners(Vec2::new(400.0, 250.0), Vec2::new(800.0, 600.0));
        let size = Vec2::new(216.0, 48.0);
        for (point, expected) in [
            (Vec2::new(520.0, 255.0), Vec2::new(412.0, 194.0)),
            (Vec2::new(680.0, 595.0), Vec2::new(572.0, 608.0)),
            (Vec2::new(405.0, 430.0), Vec2::new(176.0, 406.0)),
            (Vec2::new(795.0, 430.0), Vec2::new(808.0, 406.0)),
        ] {
            assert_eq!(
                menu_position(viewport, sand, size, point) + viewport.min,
                expected
            );
        }
        for sand in [
            Rect::from_corners(Vec2::new(30.0, 40.0), Vec2::new(1190.0, 140.0)),
            Rect::from_corners(Vec2::new(30.0, 740.0), Vec2::new(1190.0, 820.0)),
            Rect::from_corners(Vec2::new(30.0, 40.0), Vec2::new(500.0, 820.0)),
            Rect::from_corners(Vec2::new(720.0, 40.0), Vec2::new(1210.0, 820.0)),
        ] {
            for point in [sand.min, sand.center(), sand.max] {
                let position = menu_position(viewport, sand, size, point) + viewport.min;
                assert!(position.cmpge(viewport.min + Vec2::splat(8.0)).all());
                assert!(
                    (position + size)
                        .cmple(viewport.max - Vec2::splat(8.0))
                        .all()
                );
                assert!(
                    position.x + size.x <= sand.min.x - 8.0
                        || position.x >= sand.max.x + 8.0
                        || position.y + size.y <= sand.min.y - 8.0
                        || position.y >= sand.max.y + 8.0
                );
            }
        }
        let position = menu_position(viewport, viewport, size, viewport.center()) + viewport.min;
        assert!(position.y + size.y <= viewport.min.y - 8.0);
    }

    #[cfg_attr(test, test)]
    fn menu_uses_the_whole_group_and_keeps_the_click_anchor_when_it_moves() {
        let mut world = World::new();
        world.init_resource::<Menu>();
        world.insert_resource(UiScale(2.0));
        let root = world.spawn(CanvasView::default()).id();
        let mut members = Vec::new();
        for (group, size, position) in [
            (1, Vec2::new(300.0, 80.0), Vec2::new(400.0, 150.0)),
            (1, Vec2::new(300.0, 400.0), Vec2::new(400.0, 400.0)),
            (2, Vec2::splat(100.0), Vec2::new(900.0, 900.0)),
        ] {
            members.push(
                world
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size,
                        },
                        crate::canvas_selection::SandGroup([group; 16]),
                        ComputedNode { size, ..default() },
                        UiGlobalTransform::from(bevy::math::Affine2::from_translation(position)),
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
        let bounds = menu_bounds(&world, root, members[0]).unwrap();
        assert_eq!(bounds.min, Vec2::new(250.0, 110.0));
        assert_eq!(bounds.max, Vec2::new(550.0, 600.0));
        set_menu_anchor(&mut world, members[0], Some(Vec2::new(1040.0, 240.0)));
        let anchor = world.resource::<Menu>().anchor;
        let point = bounds.min + bounds.size() * anchor;
        assert!(point.distance(Vec2::new(520.0, 120.0)) < 0.01);
        for member in &members[..2] {
            let position = world.get::<UiGlobalTransform>(*member).unwrap().translation;
            world.entity_mut(*member).insert(UiGlobalTransform::from(
                bevy::math::Affine2::from_translation(position + Vec2::splat(50.0)),
            ));
        }
        let moved = menu_bounds(&world, root, members[0]).unwrap();
        assert!((moved.min + moved.size() * anchor).distance(point + Vec2::splat(50.0)) < 0.01);
    }

    #[cfg_attr(test, test)]
    fn grouped_sand_controls_have_distinct_icons_and_direct_tooltips() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                crate::icons::IconPlugin,
                PlacementPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        let mut sands = Vec::new();
        for group in [1, 1, 2] {
            sands.push(
                app.world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::splat(100.0),
                        },
                        crate::canvas_selection::SandGroup([group; 16]),
                        ChildOf(root),
                    ))
                    .id(),
            );
        }
        app.world_mut()
            .entity_mut(root)
            .insert(crate::canvas_selection::SandSelection(sands.clone()));
        app.world_mut()
            .get_mut::<Inspection>(root)
            .unwrap()
            .selected = Some(sands[0]);
        for enabled in [false, true] {
            app.world_mut()
                .resource_mut::<crate::icons::TooltipSettings>()
                .enabled = enabled;
            for _ in 0..3 {
                app.update();
            }
            let panel = app.world().resource::<Menu>().entity.unwrap();
            assert_eq!(app.world().get::<ChildOf>(panel).unwrap().parent(), root);
            let controls = app.world().get::<Children>(panel).unwrap();
            let mut icons = Vec::new();
            for control in controls.iter() {
                let button = app
                    .world()
                    .get::<crate::icons::IconButton>(control)
                    .unwrap();
                assert!(
                    !icons.contains(&button.icon),
                    "duplicate icon: {}",
                    button.label
                );
                icons.push(button.icon);
                assert_eq!(
                    app.world().get::<crate::icons::Tooltip>(control).unwrap().0,
                    button.label
                );
                assert!(
                    app.world()
                        .get::<crate::icons::InlineTooltip>(control)
                        .is_some()
                );
                assert_eq!(
                    app.world()
                        .get::<crate::icons::TooltipIcon>(control)
                        .unwrap()
                        .source,
                    control
                );
                assert!(
                    app.world()
                        .get::<Children>(control)
                        .unwrap()
                        .iter()
                        .all(|child| {
                            app.world()
                                .get::<crate::icons::TooltipIcon>(child)
                                .is_none()
                        })
                );
            }
            assert_eq!(icons.len(), 11);
        }
    }

    #[cfg_attr(test, test)]
    fn pinned_projection_keeps_screen_fraction_and_scale_after_viewport_resize() {
        let item = CanvasItem {
            position: DVec2::splat(1e12),
            size: Vec2::splat(100.0),
        };
        let pin = Pinned {
            anchor: [0.25, 0.75],
            scale: 2.0,
        };
        for viewport in [Vec2::new(800.0, 600.0), Vec2::new(1600.0, 1000.0)] {
            let position = pin
                .view(&item, viewport)
                .screen_position(&item, viewport)
                .unwrap();
            assert_eq!(position + item.size, Vec2::new(0.25, 0.75) * viewport);
        }
        let mut moved = pin;
        moved.moved(DVec2::new(80.0, -60.0), Vec2::new(800.0, 600.0));
        assert_eq!(moved.anchor, [0.35, 0.65]);
        assert!(
            !Pinned {
                anchor: [f64::NAN, 0.0],
                ..pin
            }
            .valid()
        );
        assert!(!Pinned { scale: 0.0, ..pin }.valid());
    }

    #[cfg_attr(test, test)]
    fn layer_commands_reorder_whole_sands_without_crossing_workspaces_or_screen_layer() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
        let mut sands = Vec::new();
        for _ in 0..4 {
            sands.push(
                app.world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::splat(100.0),
                        },
                        ChildOf(root),
                        WorkspaceMember(1),
                    ))
                    .id(),
            );
        }
        let child = app
            .world_mut()
            .spawn((Node::default(), ChildOf(sands[0])))
            .id();
        app.world_mut()
            .entity_mut(sands[2])
            .insert(WorkspaceMember(2));
        app.world_mut().entity_mut(sands[3]).insert(Pinned {
            anchor: [0.5, 0.5],
            scale: 1.0,
        });
        for (action, expected) in [
            (PlacementAction::Front, 1),
            (PlacementAction::Backward, 0),
            (PlacementAction::Forward, 1),
            (PlacementAction::Back, 0),
        ] {
            action.apply(app.world_mut(), sands[0]);
            assert_eq!(app.world().get::<ZIndex>(sands[0]).unwrap().0, expected);
            assert_eq!(
                app.world().get::<ChildOf>(child).unwrap().parent(),
                sands[0]
            );
            assert_eq!(app.world().get::<ZIndex>(sands[2]).unwrap().0, 0);
            assert_eq!(app.world().get::<ZIndex>(sands[3]).unwrap().0, 0);
        }
    }

    crate::laboratory_cases! {
        controls_stay_outside_the_sand_near_the_clicked_edge,
        menu_uses_the_whole_group_and_keeps_the_click_anchor_when_it_moves,
        grouped_sand_controls_have_distinct_icons_and_direct_tooltips,
        pinned_projection_keeps_screen_fraction_and_scale_after_viewport_resize,
        layer_commands_reorder_whole_sands_without_crossing_workspaces_or_screen_layer,
    }
}
