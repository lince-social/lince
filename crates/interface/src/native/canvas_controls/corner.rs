// non-lean-yet
// Scope: struct ControlsCorner, struct HideDelay, const HIDE_DELAY, struct CornerGlyph.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 76.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (116, 119, 176, 190, 225, 227, 228, 229, 233, 260, 264, 265, 267, 271, 272, 274,
//   276, 279, 282, 284, 297, 301, 305, 307, 313, 315, 319, 320, 323, 325, 328, 332, 333, 348, 353,
//   354, 356, 359, 362, 367, 370, 372, 395, 396, 399, 403).

use super::CanvasToolbar;
use crate::native::tokens::{ThemeSettings, Token};
use bevy::{
    a11y::AccessibilityNode,
    asset::RenderAssetUsages,
    ecs::query::QueryState,
    input_focus::{InputFocus, tab_navigation::TabIndex},
    picking::hover::Hovered,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use std::collections::{HashMap, HashSet};

#[derive(Component)]
pub(crate) struct ControlsCorner(Entity);

#[derive(Component, Default, Clone, Copy, PartialEq)]
struct HideDelay {
    hovered: bool,
    until: Option<std::time::Instant>,
}

const HIDE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Component)]
struct CornerGlyph;

#[derive(Resource)]
struct CornerImage(Handle<Image>);

impl FromWorld for CornerImage {
    fn from_world(world: &mut World) -> Self {
        let pixels = (0..36)
            .flat_map(|y| (0..36).flat_map(move |x| [255, 255, 255, if y >= x { 255 } else { 0 }]))
            .collect();
        Self(world.resource_mut::<Assets<Image>>().add(Image::new(
            Extent3d {
                width: 36,
                height: 36,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )))
    }
}

pub(crate) struct Corners {
    bars: QueryState<(Entity, &'static CanvasToolbar)>,
    corners: QueryState<(Entity, &'static ControlsCorner)>,
    pending: Vec<(Entity, Entity)>,
    by_bar: HashMap<Entity, Entity>,
    obsolete: Vec<Entity>,
    focused: HashSet<Entity>,
}

impl FromWorld for Corners {
    fn from_world(world: &mut World) -> Self {
        Self {
            bars: world.query(),
            corners: world.query(),
            pending: Vec::new(),
            by_bar: HashMap::new(),
            obsolete: Vec::new(),
            focused: HashSet::new(),
        }
    }
}

fn create_corner(world: &mut World, bar: Entity, view: Entity) -> Entity {
    world.init_resource::<Assets<Image>>();
    world.init_resource::<CornerImage>();
    let image = world.resource::<CornerImage>().0.clone();
    let mut accessibility = AccessibilityNode::from(accesskit::Node::new(accesskit::Role::Button));
    accessibility.set_label("Show controls");
    let corner = world
        .spawn((
            ControlsCorner(bar),
            HideDelay::default(),
            crate::native::inspection::InspectionExcluded,
            bevy::ui_widgets::Button,
            TabIndex(0),
            accessibility,
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                bottom: px(0),
                width: px(22),
                height: px(22),
                ..default()
            },
            GlobalZIndex(19),
            ChildOf(view),
        ))
        .id();
    world.spawn((
        CornerGlyph,
        ImageNode::new(image),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            bottom: px(0),
            width: px(18),
            height: px(18),
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(corner),
    ));
    let index = world
        .get::<Children>(view)
        .unwrap()
        .iter()
        .position(|entity| entity == bar)
        .unwrap();
    world.entity_mut(view).insert_children(index, &[corner]);
    corner
}

pub(crate) fn update(world: &mut World, mut corners: Local<Corners>) {
    update_at(world, std::time::Instant::now(), &mut corners);
}

fn update_at(world: &mut World, now: std::time::Instant, state: &mut Corners) {
    let _update = engine::performance::Span::new("native-toolbar-corners");
    state.pending.clear();
    state
        .pending
        .extend(state.bars.iter(world).map(|(entity, bar)| (entity, bar.0)));
    state.by_bar.clear();
    state.obsolete.clear();
    for (entity, corner) in state.corners.iter(world) {
        if world.get::<CanvasToolbar>(corner.0).is_none() {
            state.obsolete.push(entity);
        } else {
            state.by_bar.entry(corner.0).or_insert(entity);
        }
    }
    let mut redraw = false;
    for entity in state.obsolete.drain(..) {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
            redraw = true;
            nucleus::performance::add("native-toolbar-corners-retired", 1);
        }
    }
    state.focused.clear();
    let mut focus_prepared = false;
    if !state.pending.is_empty() {
        let always_show = world
            .get_resource::<super::ControlsSettings>()
            .is_some_and(|settings| settings.always_show);
        let settings = world.resource::<ThemeSettings>();
        let mut color = settings
            .resolve(Token::ControlsCornerColor, None, &Default::default())
            .0
            .color();
        let transparency = settings
            .resolve(Token::ControlsCornerTransparency, None, &Default::default())
            .0
            .number();
        color.set_alpha(color.alpha() * (1.0 - transparency / 100.0));
        for (bar, view) in state.pending.drain(..) {
            let corner = state
                .by_bar
                .get(&bar)
                .copied()
                .unwrap_or_else(|| create_corner(world, bar, view));
            let hovered = [bar, corner]
                .into_iter()
                .any(|entity| world.get::<Hovered>(entity).is_some_and(|hover| hover.0));
            let mut delay = *world.get::<HideDelay>(corner).unwrap();
            let started = delay.hovered && !hovered;
            if hovered {
                delay.until = None;
            } else if started {
                delay.until = Some(now + HIDE_DELAY);
            }
            delay.hovered = hovered;
            let waiting = delay.until.is_some_and(|until| now < until);
            if !waiting {
                delay.until = None;
            }
            if world
                .get_mut::<HideDelay>(corner)
                .unwrap()
                .set_if_neq(delay)
            {
                nucleus::performance::add("native-toolbar-delay-writes", 1);
            }
            if started && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                wake.after(HIDE_DELAY);
            }
            let expanded = world
                .get::<crate::native::edit_mode::EditMode>(view)
                .is_some_and(|mode| mode.enabled)
                || hovered
                || waiting
                || always_show
                || {
                    if !focus_prepared {
                        let mut focused =
                            world.get_resource::<InputFocus>().and_then(InputFocus::get);
                        while let Some(entity) = focused {
                            if !state.focused.insert(entity) {
                                break;
                            }
                            focused = world.get::<ChildOf>(entity).map(ChildOf::parent);
                        }
                        focus_prepared = true;
                    }
                    state.focused.contains(&bar) || state.focused.contains(&corner)
                };
            let visibility = if expanded {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            redraw |= world
                .get_mut::<Visibility>(bar)
                .unwrap()
                .set_if_neq(visibility);
            let glyph = world.get::<Children>(corner).unwrap()[0];
            if world.get::<ImageNode>(glyph).unwrap().color != color {
                world.get_mut::<ImageNode>(glyph).unwrap().color = color;
            }
            world
                .get_mut::<Visibility>(glyph)
                .unwrap()
                .set_if_neq(if expanded {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
            nucleus::performance::add("native-toolbar-corners-prepared", 1);
        }
    }
    if redraw && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[cfg_attr(test, test)]
    fn corner_reveals_on_hover_and_preserves_keyboard_access() {
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<ThemeSettings>();
        world.init_resource::<InputFocus>();
        let root = world.spawn_empty().id();
        let bar = world.spawn(super::super::toolbar_bundle(root)).id();
        let button = world.spawn(ChildOf(bar)).id();
        world.run_system_once(update).unwrap();
        let corner = world
            .query_filtered::<Entity, With<ControlsCorner>>()
            .single(&world)
            .unwrap();
        let glyph = world.get::<Children>(corner).unwrap()[0];
        for entity in [bar, corner, glyph] {
            let node = world.get::<Node>(entity).unwrap();
            assert_eq!(node.left, px(0));
            assert_eq!(node.right, Val::Auto);
        }
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        assert_eq!(world.get::<Children>(root).unwrap()[0], corner);
        world.entity_mut(corner).insert(Hovered(true));
        world.run_system_once(update).unwrap();
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(*world.get::<Visibility>(glyph).unwrap(), Visibility::Hidden);
        world.entity_mut(corner).insert(Hovered(false));
        world.entity_mut(bar).insert(Hovered(true));
        world.run_system_once(update).unwrap();
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world.entity_mut(bar).insert(Hovered(false));
        let now = std::time::Instant::now();
        let mut state = Corners::from_world(&mut world);
        update_at(&mut world, now, &mut state);
        update_at(
            &mut world,
            now + std::time::Duration::from_millis(999),
            &mut state,
        );
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        update_at(&mut world, now + HIDE_DELAY, &mut state);
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        world
            .resource_mut::<InputFocus>()
            .set(corner, bevy::input_focus::FocusCause::Navigated);
        world.run_system_once(update).unwrap();
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world
            .resource_mut::<InputFocus>()
            .set(button, bevy::input_focus::FocusCause::Navigated);
        world.run_system_once(update).unwrap();
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world.resource_mut::<InputFocus>().clear();
        world.run_system_once(update).unwrap();
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        assert_eq!(world.resource::<Assets<Image>>().len(), 1);
        world.insert_resource(super::super::ControlsSettings { always_show: true });
        world.run_system_once(update).unwrap();
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(*world.get::<Visibility>(glyph).unwrap(), Visibility::Hidden);
        world
            .resource_mut::<super::super::ControlsSettings>()
            .always_show = false;
        world.run_system_once(update).unwrap();
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        assert_eq!(
            world
                .query_filtered::<Entity, With<ControlsCorner>>()
                .iter(&world)
                .count(),
            1
        );
    }

    #[cfg_attr(test, test)]
    fn editing_keeps_edit_pinned_and_other_controls_scrollable() {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        app.world_mut()
            .get_mut::<crate::native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        let bar = super::super::toolbar(app.world_mut(), root);
        app.world_mut()
            .run_system_once(super::super::order_controls)
            .unwrap();
        app.world_mut().run_system_once(update).unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        let node = app.world().get::<Node>(bar).unwrap();
        assert_eq!(node.flex_wrap, FlexWrap::NoWrap);
        assert_eq!(node.max_width, Val::Vw(96.0));
        let children = app.world().get::<Children>(bar).unwrap();
        assert_eq!(
            children[0],
            app.world()
                .get::<crate::native::edit_mode::EditMode>(root)
                .unwrap()
                .toggle
        );
        let scroll = *children.last().unwrap();
        assert_eq!(
            app.world().get::<Node>(scroll).unwrap().overflow,
            Overflow::scroll_x()
        );
    }

    #[cfg_attr(test, test)]
    fn corner_color_and_transparency_follow_saved_tokens() {
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<ThemeSettings>();
        world.init_resource::<InputFocus>();
        let root = world.spawn_empty().id();
        world.spawn(super::super::toolbar_bundle(root));
        world.resource_mut::<ThemeSettings>().global.0.extend([
            (
                Token::ControlsCornerColor,
                crate::native::tokens::TokenValue::Color([255, 0, 0, 255]),
            ),
            (
                Token::ControlsCornerTransparency,
                crate::native::tokens::TokenValue::Number(75.0),
            ),
        ]);
        let saved = serde_json::to_string(world.resource::<ThemeSettings>()).unwrap();
        let restored: ThemeSettings = serde_json::from_str(&saved).unwrap();
        assert!(restored.validate());
        world.insert_resource(restored);
        world.run_system_once(update).unwrap();
        let image = world
            .query_filtered::<&ImageNode, With<CornerGlyph>>()
            .single(&world)
            .unwrap();
        assert_eq!(image.color, Color::srgba(1.0, 0.0, 0.0, 0.25));
        assert!(Token::ControlsCornerTransparency.parse("101").is_none());
        assert!(Token::ControlsCornerTransparency.parse("-1").is_none());
        assert!(Token::ControlsCornerTransparency.parse("NaN").is_none());
    }
    crate::native::laboratory_cases! {
        corner_reveals_on_hover_and_preserves_keyboard_access,
        editing_keeps_edit_pinned_and_other_controls_scrollable,
        corner_color_and_transparency_follow_saved_tokens,
    }
}
