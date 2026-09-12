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

#[derive(Component, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Placement {
    pub pinned: Option<Pinned>,
    pub order: i32,
    #[serde(default)]
    pub group: Option<crate::canvas_selection::SandGroup>,
}

impl Placement {
    pub(crate) fn capture(world: &World, entity: Entity) -> Self {
        Self {
            pinned: world.get::<Pinned>(entity).copied(),
            group: world
                .get::<crate::canvas_selection::SandGroup>(entity)
                .copied(),
            order: world.get::<ZIndex>(entity).map_or(0, |index| index.0),
        }
    }

    pub(crate) fn restore(self, world: &mut World, entity: Entity) {
        world.entity_mut(entity).insert(ZIndex(self.order));
        if let Some(group) = self.group {
            world.entity_mut(entity).insert(group);
        }
        if let Some(pinned) = self.pinned {
            world.entity_mut(entity).insert((pinned, GlobalZIndex(1)));
        }
    }

    pub(crate) fn valid(&self) -> bool {
        self.pinned.is_none_or(|pin| pin.valid())
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
            if world.get::<crate::area::InfluenceArea>(entity).is_some() {
                if !crate::area_panel::owns(world, parent, entity) {
                    return;
                }
                crate::area_mutation::disarm(world, entity, "Disarmed after deletion.");
                world.despawn(entity);
                if let Some(mut editor) = world.get_mut::<crate::area_panel::AreaEditor>(parent)
                    && editor.selected == Some(entity)
                {
                    editor.selected = None;
                    editor.cancel();
                }
                crate::edit_mode::render_panel(world, parent);
            } else if world.get::<crate::sand_store::StoredSand>(entity).is_some() {
                crate::edit_mode::EditAction::RemoveSand(entity).apply(world, parent);
            }
            return;
        }
        if world.get::<crate::area::InfluenceArea>(entity).is_some() {
            return;
        }
        let Some(item) = world.get::<CanvasItem>(entity).copied() else {
            return;
        };
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
            let Some(viewport) = world
                .get::<ComputedUiRenderTargetInfo>(parent)
                .map(|target| target.logical_size())
            else {
                return;
            };
            if !viewport.is_finite() || viewport.min_element() <= 0.0 {
                return;
            }
            if let Some(pin) = world.get::<Pinned>(entity).copied() {
                let position = view.center
                    + (DVec2::from_array(pin.anchor) - DVec2::splat(0.5)) * viewport.as_dvec2()
                        / view.zoom;
                world.get_mut::<CanvasItem>(entity).unwrap().position = position;
                world.entity_mut(entity).remove::<Pinned>();
                world.entity_mut(entity).remove::<GlobalZIndex>();
            } else {
                let anchor = DVec2::splat(0.5)
                    + (item.position - view.center) * view.zoom / viewport.as_dvec2();
                let pin = Pinned {
                    anchor: anchor.to_array(),
                    scale: view.zoom,
                };
                if pin.valid() {
                    world.entity_mut(entity).insert((pin, GlobalZIndex(1)));
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
    let pinned = target.is_some_and(|(_, entity)| world.get::<Pinned>(entity).is_some());
    let grouping = target.map_or((false, false), |(root, target)| {
        crate::canvas_selection::options(world, root, target)
    });
    let current = world.resource::<Menu>();
    if current.target == target.map(|(_, entity)| entity)
        && current.pinned == pinned
        && current.grouping == grouping
        && current
            .entity
            .is_none_or(|entity| world.get_entity(entity).is_ok())
    {
        if let (Some((root, target)), Some(panel)) = (target, current.entity) {
            position_menu(world, root, target, panel);
        }
        return;
    }
    if let Some(entity) = world.resource_mut::<Menu>().entity.take() {
        world.despawn(entity);
    }
    *world.resource_mut::<Menu>() = Menu {
        target: target.map(|(_, entity)| entity),
        pinned,
        grouping,
        entity: None,
    };
    let Some((root, target)) = target else { return };
    let area = world.get::<crate::area::InfluenceArea>(target).is_some();
    let deletable = area || world.get::<crate::sand_store::StoredSand>(target).is_some();
    let panel = world
        .spawn((
            PlacementMenu,
            crate::inspection::InspectionExcluded,
            crate::sand::Square,
            Node {
                position_type: PositionType::Absolute,
                width: px((if area {
                    48
                } else if deletable {
                    252
                } else {
                    216
                }) + 36 * (i32::from(grouping.0) + i32::from(grouping.1))),
                height: px(48),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(4)),
                column_gap: px(4),
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
    world.resource_mut::<Menu>().entity = Some(panel);
    position_menu(world, root, target, panel);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn menu_position(viewport: Rect, sand: Rect, size: Vec2) -> Vec2 {
    let margin = Vec2::splat(8.0);
    let below = viewport.max.y - sand.max.y;
    let above = sand.min.y - viewport.min.y;
    let y = if below >= above {
        sand.max.y + margin.y
    } else {
        sand.min.y - size.y - margin.y
    };
    Vec2::new(sand.center().x - size.x * 0.5, y).clamp(
        viewport.min + margin,
        (viewport.max - size - margin).max(viewport.min + margin),
    ) - viewport.min
}

fn retained_menu_target(world: &mut World) -> Option<(Entity, Entity)> {
    let menu = world.resource::<Menu>();
    let target = menu.target?;
    let panel = menu.entity?;
    let root = world.get::<ChildOf>(target)?.parent();
    if !world.get::<crate::edit_mode::EditMode>(root)?.enabled {
        return None;
    }
    let sand = crate::inspection::bounds(world, target)?;
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
    let Some(sand) = crate::inspection::bounds(world, target) else {
        return;
    };
    let size =
        crate::inspection::bounds(world, panel).map_or(Vec2::new(216.0, 48.0), |rect| rect.size());
    let position = menu_position(viewport, sand, size);
    let mut node = world.get_mut::<Node>(panel).unwrap();
    if node.left != px(position.x) || node.top != px(position.y) {
        node.left = px(position.x);
        node.top = px(position.y);
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn controls_follow_screen_bounds_and_choose_the_roomier_side() {
        let viewport = Rect::from_corners(Vec2::new(20.0, 30.0), Vec2::new(820.0, 630.0));
        let size = Vec2::new(216.0, 48.0);
        let top = Rect::from_corners(Vec2::new(200.0, 40.0), Vec2::new(400.0, 140.0));
        let bottom = Rect::from_corners(Vec2::new(200.0, 500.0), Vec2::new(400.0, 600.0));
        assert_eq!(menu_position(viewport, top, size), Vec2::new(172.0, 118.0));
        assert_eq!(
            menu_position(viewport, bottom, size),
            Vec2::new(172.0, 414.0)
        );
        for sand in [
            top,
            bottom,
            Rect::from_center_size(Vec2::splat(-1000.0), Vec2::splat(100.0)),
            Rect::from_center_size(Vec2::splat(2000.0), Vec2::splat(100.0)),
        ] {
            let position = menu_position(viewport, sand, size);
            assert!(position.cmpge(Vec2::splat(8.0)).all());
            assert!(
                (position + size)
                    .cmple(viewport.size() - Vec2::splat(8.0))
                    .all()
            );
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
        controls_follow_screen_bounds_and_choose_the_roomier_side,
        pinned_projection_keeps_screen_fraction_and_scale_after_viewport_resize,
        layer_commands_reorder_whole_sands_without_crossing_workspaces_or_screen_layer,
    }
}
