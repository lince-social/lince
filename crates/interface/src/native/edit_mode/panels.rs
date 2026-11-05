use crate::native::actions::Action;
use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Edit,
    Item,
}

impl Kind {
    fn index(self) -> usize {
        match self {
            Self::Edit => 0,
            Self::Item => 1,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    position: Option<Vec2>,
    minimized: bool,
}

#[derive(Component, Default)]
struct Memory([State; 2]);

#[derive(Component)]
pub(crate) struct Header;

#[derive(Component)]
struct Panel {
    root: Entity,
    kind: Kind,
    body: Option<Entity>,
    toggle: Option<(Entity, Entity)>,
}

#[derive(Clone, Copy)]
struct Minimize;

impl Action for Minimize {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Panel>(owner) else {
            return;
        };
        let (root, kind) = (panel.root, panel.kind);
        let Some(mut memory) = world.get_mut::<Memory>(root) else {
            return;
        };
        let state = &mut memory.0[kind.index()];
        state.minimized = !state.minimized;
        wake(world);
    }
}

fn wake(world: &World) {
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) fn attach(world: &mut World, owner: Entity, root: Entity, kind: Kind) -> Entity {
    if world.get::<Memory>(root).is_none() {
        world.entity_mut(root).insert(Memory::default());
    }
    let header = world
        .spawn((
            Header,
            Node {
                width: percent(100),
                min_height: px(32),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                column_gap: px(6),
                padding: UiRect::right(px(if kind == Kind::Edit { 40 } else { 0 })),
                ..default()
            },
            crate::native::icons::Tooltip("Drag to move this panel on the screen".into()),
            ChildOf(owner),
        ))
        .id();
    let title = super::label(
        world,
        header,
        if kind == Kind::Edit {
            "Edit"
        } else {
            "Item controls"
        },
        14.0,
    );
    world.entity_mut(title).insert((
        Pickable::IGNORE,
        Node {
            flex_grow: 1.0,
            min_width: px(0),
            ..default()
        },
    ));
    world.entity_mut(header).observe(
        move |mut event: On<Pointer<Drag>>, mut commands: Commands| {
            if event.button == PointerButton::Primary {
                let delta = event.delta;
                commands.queue(move |world: &mut World| move_panel(world, owner, delta));
            }
            event.propagate(false);
        },
    );
    world
        .entity_mut(header)
        .observe(|mut event: On<Pointer<DragStart>>| event.propagate(false));
    world
        .entity_mut(header)
        .observe(|mut event: On<Pointer<DragEnd>>| event.propagate(false));
    let (body, toggle) = if kind == Kind::Item {
        let toggle = crate::native::layout::desktop::input_settings::button(
            world, header, owner, "−", Minimize,
        );
        {
            let mut node = world.get_mut::<Node>(toggle.0).unwrap();
            node.width = Val::Auto;
            node.padding = UiRect::axes(px(6), px(4));
        }
        world.get_mut::<Node>(toggle.1).unwrap().width = Val::Auto;
        world
            .get_mut::<bevy::a11y::AccessibilityNode>(toggle.0)
            .unwrap()
            .set_label("Minimize");
        world
            .entity_mut(toggle.0)
            .insert(crate::native::icons::Tooltip(
                "Minimize item controls".into(),
            ));
        let body = crate::native::sand_panel::column(world, owner);
        world.entity_mut(body).insert((
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 1.0,
                min_height: px(0),
                row_gap: px(4),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ));
        crate::native::scroll_sand::decorate(world, body);
        (Some(body), Some(toggle))
    } else {
        (None, None)
    };
    world.entity_mut(owner).insert(Panel {
        root,
        kind,
        body,
        toggle,
    });
    body.unwrap_or(owner)
}

fn move_panel(world: &mut World, owner: Entity, delta: Vec2) {
    if !delta.is_finite() {
        return;
    }
    let Some(panel) = world.get::<Panel>(owner) else {
        return;
    };
    let (root, kind) = (panel.root, panel.kind);
    let scale = world
        .get::<ComputedNode>(root)
        .map_or(1.0, ComputedNode::inverse_scale_factor);
    let Some(node) = world.get::<Node>(owner) else {
        return;
    };
    let (Val::Px(left), Val::Px(top)) = (node.left, node.top) else {
        return;
    };
    let Some(mut memory) = world.get_mut::<Memory>(root) else {
        return;
    };
    let state = &mut memory.0[kind.index()];
    state.position = Some(state.position.unwrap_or(Vec2::new(left, top)) + delta * scale);
    wake(world);
}

fn constrained(position: Vec2, size: Vec2, bounds: Rect) -> Vec2 {
    position.clamp(bounds.min, (bounds.max - size).max(bounds.min))
}

pub(crate) fn layout(world: &mut World) {
    let panels: Vec<_> = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .map(|(entity, panel)| (entity, panel.root, panel.kind, panel.body, panel.toggle))
        .collect();
    for &(owner, root, kind, body, toggle) in &panels {
        if world
            .get::<Node>(owner)
            .is_none_or(|node| node.display == Display::None)
        {
            continue;
        }
        let Some(computed) = world.get::<ComputedNode>(root) else {
            continue;
        };
        let scale = computed.inverse_scale_factor();
        let size = computed.size() * scale;
        if !size.is_finite() || size.min_element() <= 0.0 {
            continue;
        }
        let mut usable = Rect::from_corners(Vec2::ZERO, size);
        if let Some(viewport) = world.get::<crate::native::layout::desktop::HostViewport>(root)
            && let Some(area) = viewport.0.usable
        {
            let origin = viewport.0.bounds.origin();
            usable = usable.intersect(Rect::from_corners(
                Vec2::new(
                    (area.origin()[0] - origin[0]) as f32,
                    (area.origin()[1] - origin[1]) as f32,
                ),
                Vec2::new(
                    (area.origin()[0] + area.size()[0] - origin[0]) as f32,
                    (area.origin()[1] + area.size()[1] - origin[1]) as f32,
                ),
            ));
        }
        if !usable.size().is_finite() || usable.size().min_element() <= 0.0 {
            continue;
        }
        let (_, maximum) =
            super::panel_horizontal(size.x, size.x, 0.0, [usable.min.x, usable.max.x]);
        let margin = ((usable.width() - maximum) * 0.5).max(0.0);
        usable.min += Vec2::splat(margin);
        usable.max = (usable.max - Vec2::splat(margin)).max(usable.min + Vec2::ONE);
        if let Some(mode) = world.get::<super::EditMode>(root)
            && let (Some(button), Some(root_transform)) = (
                crate::native::inspection::bounds(world, mode.toggle),
                world.get::<UiGlobalTransform>(root),
            )
        {
            let root_top = root_transform.translation * scale - size * 0.5;
            let bottom = button.min.y - root_top.y - 8.0;
            if bottom > usable.min.y + 32.0 {
                usable.max.y = usable.max.y.min(bottom);
            }
        }
        let state = world
            .get::<Memory>(root)
            .map_or(State::default(), |memory| memory.0[kind.index()]);
        let other = panels
            .iter()
            .find(|&&(entity, other_root, other_kind, _, _)| {
                entity != owner
                    && other_root == root
                    && other_kind != kind
                    && world
                        .get::<Node>(entity)
                        .is_some_and(|node| node.display != Display::None)
            });
        let other_width = other.and_then(|(entity, _, other_kind, _, _)| {
            let memory = world.get::<Memory>(root)?;
            (state.position.is_none() && memory.0[other_kind.index()].position.is_none()).then(
                || {
                    if memory.0[other_kind.index()].minimized {
                        180.0
                    } else if *other_kind == Kind::Item {
                        300.0
                    } else {
                        match world.get::<Node>(*entity).map(|node| node.width) {
                            Some(Val::Px(width)) => width,
                            _ => 300.0,
                        }
                    }
                },
            )
        });
        let preferred = if state.minimized {
            180.0
        } else if kind == Kind::Item {
            300.0
        } else {
            match world.get::<Node>(owner).map(|node| node.width) {
                Some(Val::Px(width)) => width,
                _ => 376.0,
            }
        };
        let max_width = other_width
            .map_or(usable.width(), |other| {
                let available = (usable.width() - 12.0).max(2.0);
                if preferred + other <= available {
                    preferred
                } else {
                    available * preferred / (preferred + other)
                }
            })
            .max(1.0);
        let width = preferred.min(max_width);
        let height = world
            .get::<ComputedNode>(owner)
            .map(|node| node.unrounded_size().y * node.inverse_scale_factor())
            .filter(|height| *height > 0.0)
            .unwrap_or(usable.height());
        let height = if kind == Kind::Edit {
            (usable.height() * 0.85).min(760.0)
        } else if state.minimized {
            56.0_f32.min(usable.height())
        } else {
            height.min(usable.height())
        };
        let default = Vec2::new(
            if kind == Kind::Item {
                usable.max.x - width
            } else {
                usable.min.x
            },
            usable.min.y,
        );
        let position = constrained(
            state.position.unwrap_or(default),
            Vec2::new(width, height),
            usable,
        );
        let mut node = world.get::<Node>(owner).unwrap().clone();
        node.left = px(position.x);
        node.top = px(position.y);
        node.right = Val::Auto;
        node.bottom = Val::Auto;
        node.max_width = px(max_width);
        node.max_height = px(usable.height());
        if kind == Kind::Edit {
            node.height = px(height);
        }
        if kind == Kind::Item {
            node.width = px(if state.minimized { 180 } else { 300 });
            node.height = if state.minimized { px(56) } else { Val::Auto };
        }
        if *world.get::<Node>(owner).unwrap() != node {
            *world.get_mut::<Node>(owner).unwrap() = node;
            wake(world);
        }
        if let Some(body) = body {
            let display = if state.minimized {
                Display::None
            } else {
                Display::Flex
            };
            if world.get::<Node>(body).unwrap().display != display {
                world.get_mut::<Node>(body).unwrap().display = display;
            }
        }
        if let Some((button, label)) = toggle {
            let caption = if state.minimized {
                "Restore"
            } else {
                "Minimize"
            };
            let symbol = if state.minimized { "+" } else { "−" };
            if world
                .get::<Text>(label)
                .is_some_and(|text| text.0 != symbol)
            {
                crate::native::layout::desktop::input_settings::text(world, label, symbol);
                if let Some(mut accessibility) =
                    world.get_mut::<bevy::a11y::AccessibilityNode>(button)
                {
                    accessibility.set_label(caption);
                }
                world
                    .entity_mut(button)
                    .insert(crate::native::icons::Tooltip(format!(
                        "{caption} item controls"
                    )));
            }
        }
        if state.position.is_some_and(|previous| previous != position) {
            world.get_mut::<Memory>(root).unwrap().0[kind.index()].position = Some(position);
        }
    }
}

#[cfg(test)]
mod tests;
