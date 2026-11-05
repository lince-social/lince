use bevy::prelude::*;

#[derive(Component)]
pub struct ScrollSand;

const BAR_WIDTH: f32 = 6.0;

#[derive(Component)]
struct ScrollCredits;

#[derive(Component)]
struct OverlayBar;

#[derive(Component)]
struct Decorated;

const CREDITS: &[crate::native::credits::Attribution] = &[crate::native::credits::Attribution {
    name: "Bevy",
    author: "Bevy contributors",
    license: crate::native::credits::BEVY_LICENSE,
}];

pub fn attach(world: &mut World, entity: Entity) {
    if world.get::<ScrollSand>(entity).is_none() {
        world.entity_mut(entity).insert(ScrollSand);
        world
            .entity_mut(entity)
            .entry::<ScrollPosition>()
            .or_default();
    }
    if !world.get::<Children>(entity).is_some_and(|children| {
        children
            .iter()
            .any(|child| world.get::<ScrollCredits>(child).is_some())
    }) {
        world.spawn((
            ScrollCredits,
            crate::native::sand_store::SandCredits(CREDITS),
            ChildOf(entity),
        ));
    }
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        node.overflow = Overflow::scroll_y();
    }
}

#[derive(Component)]
struct Scrollbar {
    viewport: Entity,
    thumb: Entity,
}

#[derive(Component, Clone, Copy)]
struct ScrollThumb(Entity);

pub(crate) fn decorate(world: &mut World, viewport: Entity) {
    attach(world, viewport);
    if world
        .query::<&Scrollbar>()
        .iter(world)
        .any(|bar| bar.viewport == viewport)
    {
        return;
    }
    let track = bar(world, viewport, viewport);
    world.entity_mut(track).insert((
        OverlayBar,
        bevy::ui::IgnoreScroll(bevy::math::BVec2::TRUE),
        ZIndex(1),
    ));
    let mut node = world.get_mut::<Node>(track).unwrap();
    node.position_type = PositionType::Absolute;
    node.right = px(2);
    node.top = px(0);
    node.display = Display::None;
    if world.get::<Decorated>(viewport).is_none() {
        world.entity_mut(viewport).insert(Decorated);
        let mut node = world.get_mut::<Node>(viewport).unwrap();
        let padding = match node.padding.right {
            Val::Px(value) => value,
            _ => 0.0,
        };
        node.padding.right = px(padding + BAR_WIDTH + 4.0);
    }
}

pub(crate) fn decoration(world: &World, entity: Entity) -> bool {
    world.get::<OverlayBar>(entity).is_some() || world.get::<ScrollCredits>(entity).is_some()
}

pub(crate) fn bounded(world: &mut World, parent: Entity, height: f32) -> Entity {
    let frame = world
        .spawn((
            Node {
                width: percent(100),
                height: px(height),
                min_height: px(80),
                flex_shrink: 0.0,
                column_gap: px(4),
                overflow: Overflow::clip(),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let viewport = world
        .spawn((
            Node {
                min_width: px(0),
                flex_grow: 1.0,
                flex_basis: px(0),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(frame),
        ))
        .id();
    attach(world, viewport);
    bar(world, frame, viewport);
    viewport
}

fn bar(world: &mut World, parent: Entity, viewport: Entity) -> Entity {
    let track = world
        .spawn((
            Node {
                width: px(BAR_WIDTH),
                height: percent(100),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
        ))
        .id();
    let mut accessible = accesskit::Node::new(accesskit::Role::ScrollBar);
    accessible.set_label("Scroll content");
    let thumb = world
        .spawn((
            bevy::ui_widgets::Button,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                min_height: px(24),
                ..default()
            },
            ChildOf(track),
            ScrollThumb(track),
            bevy::input_focus::tab_navigation::TabIndex(0),
            bevy::a11y::AccessibilityNode::from(accessible),
            crate::native::token_style::background(crate::native::tokens::Token::Accent),
            crate::native::icons::Tooltip(
                "Drag to scroll; arrow keys, Page Up/Down, Home and End".into(),
            ),
        ))
        .id();
    world
        .entity_mut(track)
        .insert(Scrollbar { viewport, thumb })
        .observe(scrollbar_click);
    world
        .entity_mut(thumb)
        .observe(scrollbar_drag)
        .observe(|mut event: On<Pointer<DragStart>>| event.propagate(false))
        .observe(|mut event: On<Pointer<DragEnd>>| event.propagate(false))
        .observe(scrollbar_key);
    track
}

pub(crate) fn fill(world: &mut World, parent: Entity) -> Entity {
    let viewport = bounded(world, parent, 300.0);
    let frame = world.get::<ChildOf>(viewport).unwrap().parent();
    let mut node = world.get_mut::<Node>(frame).unwrap();
    node.height = percent(100);
    node.min_height = px(0);
    node.flex_grow = 1.0;
    node.flex_shrink = 1.0;
    viewport
}

fn geometry(node: &ComputedNode) -> (f32, f32) {
    let scale = node.inverse_scale_factor();
    let height = node.size().y * scale;
    let maximum = ((node.content_size.y - node.size().y) * scale).max(0.0);
    (height, maximum)
}

fn track_length(node: &ComputedNode, overlay: bool) -> f32 {
    if overlay {
        node.content_box().height().max(0.0) * node.inverse_scale_factor()
    } else {
        node.size().y * node.inverse_scale_factor()
    }
}

pub(crate) fn update_bars(world: &mut World) {
    let mut changed = false;
    let bars = world
        .query::<(Entity, &Scrollbar)>()
        .iter(world)
        .map(|(track, bar)| (track, bar.viewport, bar.thumb))
        .collect::<Vec<_>>();
    for (track, viewport, thumb) in bars {
        if world.get_entity(viewport).is_err() {
            world.despawn(track);
            continue;
        }
        let Some(computed) = world.get::<ComputedNode>(viewport) else {
            continue;
        };
        let (height, maximum) = geometry(computed);
        let overlay = world.get::<OverlayBar>(track).is_some();
        let length = track_length(computed, overlay);
        if overlay {
            let top = px((computed.content_box().min.y + computed.size().y * 0.5)
                * computed.inverse_scale_factor());
            let mut node = world.get_mut::<Node>(track).unwrap();
            if node.height != px(length) || node.top != top {
                node.height = px(length);
                node.top = top;
                changed = true;
            }
        }
        if height <= 0.0 {
            if world
                .get::<Node>(track)
                .is_some_and(|node| node.display != Display::None)
            {
                world.get_mut::<Node>(track).unwrap().display = Display::None;
                changed = true;
            }
            continue;
        }
        let thumb_height = (length * height / (height + maximum)).max(24.0).min(length);
        let offset = world
            .get::<ScrollPosition>(viewport)
            .map_or(0.0, |scroll| scroll.0.y)
            .clamp(0.0, maximum);
        if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
            if scroll.0.y != offset {
                scroll.0.y = offset;
                changed = true;
            }
        }
        let mut node = world.get_mut::<Node>(thumb).unwrap();
        let top = px(if maximum > 0.0 {
            offset / maximum * (length - thumb_height)
        } else {
            0.0
        });
        let display = if maximum > 0.0 {
            Display::Flex
        } else {
            Display::None
        };
        if node.height != px(thumb_height) {
            node.height = px(thumb_height);
            changed = true;
        }
        if node.top != top {
            node.top = top;
            changed = true;
        }
        if node.display != display {
            node.display = display;
            changed = true;
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(thumb) {
            if accessibility.numeric_value() != Some(offset as f64) {
                accessibility.set_numeric_value(offset as f64);
            }
            if accessibility.min_numeric_value() != Some(0.0) {
                accessibility.set_min_numeric_value(0.0);
            }
            if accessibility.max_numeric_value() != Some(maximum as f64) {
                accessibility.set_max_numeric_value(maximum as f64);
            }
        }
        if let Some(mut node) = world.get_mut::<Node>(track) {
            if node.display != display {
                node.display = display;
                changed = true;
            }
        }
    }
    if changed && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

fn move_thumb(world: &mut World, track: Entity, delta: f32) {
    let Some(bar) = world.get::<Scrollbar>(track) else {
        return;
    };
    let viewport = bar.viewport;
    let Some(node) = world.get::<ComputedNode>(viewport) else {
        return;
    };
    let (height, maximum) = geometry(node);
    let length = track_length(node, world.get::<OverlayBar>(track).is_some());
    let thumb = (length * height / (height + maximum)).max(24.0).min(length);
    if length <= thumb || maximum <= 0.0 {
        return;
    }
    if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
        scroll.0.y = (scroll.0.y + delta * maximum / (length - thumb)).clamp(0.0, maximum);
    }
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

fn scrollbar_drag(
    mut event: On<Pointer<Drag>>,
    thumbs: Query<&ScrollThumb>,
    transforms: Query<&UiGlobalTransform>,
    mut commands: Commands,
) {
    if event.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok(thumb) = thumbs.get(event.entity) else {
        return;
    };
    event.propagate(false);
    let track = thumb.0;
    let delta = transforms.get(track).map_or(event.delta.y, |transform| {
        transform.inverse().transform_vector2(event.delta).y
    });
    commands.queue(move |world: &mut World| move_thumb(world, track, delta));
}

fn scrollbar_click(
    mut event: On<Pointer<Click>>,
    bars: Query<&Scrollbar>,
    thumbs: Query<&ScrollThumb>,
    mut commands: Commands,
) {
    if event.button != bevy::picking::pointer::PointerButton::Primary
        || bars.get(event.entity).is_err()
    {
        return;
    }
    event.propagate(false);
    if thumbs.get(event.original_event_target()).is_ok() {
        return;
    }
    let track = event.entity;
    let position = event.pointer_location.position;
    commands.queue(move |world: &mut World| {
        let Some(bar) = world.get::<Scrollbar>(track) else {
            return;
        };
        let viewport = bar.viewport;
        let Some(node) = world.get::<ComputedNode>(viewport) else {
            return;
        };
        let (_, maximum) = geometry(node);
        let height = track_length(node, world.get::<OverlayBar>(track).is_some());
        if height <= 0.0 {
            return;
        }
        let Some(transform) = world.get::<UiGlobalTransform>(track) else {
            return;
        };
        let y = transform.inverse().transform_point2(position).y + height * 0.5;
        if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
            scroll.0.y = (y / height * maximum).clamp(0.0, maximum);
        }
    });
}

fn scrollbar_key(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    thumbs: Query<&ScrollThumb>,
    bars: Query<&Scrollbar>,
    mut commands: Commands,
) {
    if !event.input.state.is_pressed() {
        return;
    }
    let Ok(thumb) = thumbs.get(event.focused_entity) else {
        return;
    };
    let Ok(bar) = bars.get(thumb.0) else { return };
    let viewport = bar.viewport;
    let key = event.input.key_code;
    if !matches!(
        key,
        KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
    ) {
        return;
    }
    event.propagate(false);
    commands.queue(move |world: &mut World| {
        let Some(node) = world.get::<ComputedNode>(viewport) else {
            return;
        };
        let (height, maximum) = geometry(node);
        if let Some(mut scroll) = world.get_mut::<ScrollPosition>(viewport) {
            scroll.0.y = match key {
                KeyCode::Home => 0.0,
                KeyCode::End => maximum,
                KeyCode::ArrowUp => scroll.0.y - 32.0,
                KeyCode::ArrowDown => scroll.0.y + 32.0,
                KeyCode::PageUp => scroll.0.y - height,
                _ => scroll.0.y + height,
            }
            .clamp(0.0, maximum);
        }
    });
}

pub(crate) fn reveal(
    world: &mut World,
    viewport: Entity,
    target: Entity,
    inset: f32,
) -> Option<bool> {
    let panel = world.get::<ComputedNode>(viewport)?;
    let node = world.get::<ComputedNode>(target)?;
    if panel.size().y <= 0.0 || node.size().y <= 0.0 {
        return None;
    }
    let mut current = target;
    let mut contained = false;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(parent) = world.get::<ChildOf>(current) else {
            break;
        };
        current = parent.parent();
        if current == viewport {
            contained = true;
            break;
        }
    }
    if !contained || world.get::<Node>(viewport)?.display == Display::None {
        return None;
    }
    let scale = panel.inverse_scale_factor();
    if !scale.is_finite() || scale <= 0.0 || !inset.is_finite() {
        return None;
    }
    let inset = inset.max(0.0) / scale;
    let center = world.get::<UiGlobalTransform>(viewport)?.translation.y;
    let start = center - panel.size().y * 0.5 + inset;
    let end = center + panel.size().y * 0.5 - inset;
    let top = world.get::<UiGlobalTransform>(target)?.translation.y - node.size().y * 0.5;
    let bottom = top + node.size().y;
    let delta = if top < start || node.size().y > end - start {
        top - start
    } else if bottom > end {
        bottom - end
    } else {
        0.0
    };
    let max = (panel.content_size.y - panel.size.y + panel.scrollbar_size.y).max(0.0) * scale;
    let mut scroll = world.get_mut::<ScrollPosition>(viewport)?;
    let next = (scroll.0.y + delta * scale).clamp(0.0, max);
    let changed = (next - scroll.0.y).abs() > 0.5;
    if changed {
        scroll.0.y = next;
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine2;

    #[test]
    fn decorating_existing_panels_preserves_geometry_scroll_and_single_slim_bar() {
        let mut world = World::new();
        let viewport = world
            .spawn((
                Node {
                    width: px(376),
                    height: px(400),
                    padding: UiRect::all(px(16)),
                    ..default()
                },
                ScrollPosition(Vec2::new(0.0, 72.0)),
                ComputedNode {
                    size: Vec2::new(376.0, 400.0),
                    content_size: Vec2::new(376.0, 1600.0),
                    ..default()
                },
            ))
            .id();
        decorate(&mut world, viewport);
        decorate(&mut world, viewport);
        let node = world.get::<Node>(viewport).unwrap();
        assert_eq!((node.width, node.height), (px(376), px(400)));
        assert_eq!(node.padding.right, px(26));
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 72.0);
        assert_eq!(world.query::<&Scrollbar>().iter(&world).count(), 1);
        let (track, thumb) = world
            .query::<(Entity, &Scrollbar)>()
            .iter(&world)
            .find(|(_, bar)| bar.viewport == viewport)
            .map(|(track, bar)| (track, bar.thumb))
            .unwrap();
        assert_eq!(world.get::<Node>(track).unwrap().width, px(6));
        assert_eq!(world.get::<ChildOf>(track).unwrap().parent(), viewport);
        assert_eq!(
            world.get::<bevy::ui::IgnoreScroll>(track).unwrap().0,
            bevy::math::BVec2::TRUE
        );
        update_bars(&mut world);
        move_thumb(&mut world, track, 10000.0);
        update_bars(&mut world);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 1200.0);
        assert!(world.get::<Node>(thumb).unwrap().top != px(0));
        world
            .get_mut::<ComputedNode>(viewport)
            .unwrap()
            .content_size
            .y = 400.0;
        update_bars(&mut world);
        assert_eq!(world.get::<Node>(track).unwrap().display, Display::None);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 0.0);
    }

    #[test]
    fn dragging_the_thumb_reaches_both_ends_at_scaled_sizes_and_clamps_after_resize() {
        let mut world = World::new();
        let parent = world.spawn(Node::default()).id();
        let viewport = bounded(&mut world, parent, 100.0);
        world.entity_mut(viewport).insert(ComputedNode {
            size: Vec2::new(300.0, 200.0),
            content_size: Vec2::new(300.0, 2000.0),
            inverse_scale_factor: 0.5,
            ..default()
        });
        update_bars(&mut world);
        let (track, thumb) = world
            .query::<(Entity, &Scrollbar)>()
            .iter(&world)
            .find(|(_, bar)| bar.viewport == viewport)
            .map(|(track, bar)| (track, bar.thumb))
            .unwrap();
        assert_eq!(world.get::<Node>(thumb).unwrap().height, px(24));
        move_thumb(&mut world, track, 38.0);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 450.0);
        update_bars(&mut world);
        assert_eq!(world.get::<Node>(thumb).unwrap().top, px(38));
        move_thumb(&mut world, track, 1000.0);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 900.0);
        move_thumb(&mut world, track, -1000.0);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 0.0);
        move_thumb(&mut world, track, 1000.0);
        world
            .get_mut::<ComputedNode>(viewport)
            .unwrap()
            .content_size
            .y = 200.0;
        update_bars(&mut world);
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 0.0);
        assert_eq!(world.get::<Node>(track).unwrap().display, Display::None);
    }

    #[test]
    fn revealing_a_selected_descendant_scrolls_once_without_changing_input_focus() {
        let mut world = World::new();
        world.init_resource::<bevy::input_focus::InputFocus>();
        let input = world.spawn_empty().id();
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(input, bevy::input_focus::FocusCause::Navigated);
        let viewport = world
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(300.0, 100.0),
                    content_size: Vec2::new(300.0, 500.0),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
                UiGlobalTransform::default(),
                ScrollPosition::default(),
            ))
            .id();
        let target = world
            .spawn((
                ComputedNode {
                    size: Vec2::new(280.0, 20.0),
                    ..default()
                },
                UiGlobalTransform::from(Affine2::from_translation(Vec2::new(0.0, 180.0))),
                ChildOf(viewport),
            ))
            .id();
        assert_eq!(reveal(&mut world, viewport, target, 8.0), Some(true));
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 78.0);
        world
            .entity_mut(target)
            .insert(UiGlobalTransform::from(Affine2::from_translation(
                Vec2::new(0.0, 24.0),
            )));
        assert_eq!(reveal(&mut world, viewport, target, 8.0), Some(false));
        assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, 78.0);
        world.entity_mut(target).remove::<ChildOf>();
        assert_eq!(reveal(&mut world, viewport, target, 8.0), None);
        assert_eq!(
            world.resource::<bevy::input_focus::InputFocus>().get(),
            Some(input)
        );
    }

    #[test]
    fn wheel_over_content_scrolls_once_and_stops_at_both_ends() {
        let mut app = App::new();
        app.add_plugins(crate::native::layout::LayoutPlugin);
        let world = app.world_mut();
        let viewport = world
            .spawn((
                Node::default(),
                ComputedNode {
                    size: Vec2::new(520.0, 640.0),
                    content_size: Vec2::new(520.0, 1600.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
            ))
            .id();
        attach(world, viewport);
        let content = world.spawn((Node::default(), ChildOf(viewport))).id();
        for (delta, expected) in [(-90.0, 90.0), (-2000.0, 960.0), (2000.0, 0.0)] {
            world.trigger(Pointer::new(
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
                    hit: bevy::picking::backend::HitData::new(viewport, 0.0, None, None),
                },
                content,
            ));
            assert_eq!(world.get::<ScrollPosition>(viewport).unwrap().0.y, expected);
        }
    }
}
