use super::CanvasToolbar;
use crate::tokens::{ThemeSettings, Token};
use bevy::{
    a11y::AccessibilityNode,
    asset::RenderAssetUsages,
    input_focus::{InputFocus, tab_navigation::TabIndex},
    picking::hover::Hovered,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

#[derive(Component)]
pub(crate) struct ControlsCorner(Entity);

#[derive(Component)]
struct CornerGlyph;

#[derive(Resource)]
struct CornerImage(Handle<Image>);

impl FromWorld for CornerImage {
    fn from_world(world: &mut World) -> Self {
        let pixels = (0..36)
            .flat_map(|y| {
                (0..36).flat_map(move |x| [255, 255, 255, if x + y >= 35 { 255 } else { 0 }])
            })
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

fn contains_focus(world: &World, ancestor: Entity) -> bool {
    let mut entity = world.resource::<InputFocus>().get();
    while let Some(current) = entity {
        if current == ancestor {
            return true;
        }
        entity = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    false
}

pub(super) fn update(world: &mut World) {
    let bars: Vec<_> = world
        .query::<(Entity, &CanvasToolbar)>()
        .iter(world)
        .map(|(entity, bar)| (entity, bar.0))
        .collect();
    for (bar, view) in bars {
        let corner = world
            .query::<(Entity, &ControlsCorner)>()
            .iter(world)
            .find(|(_, corner)| corner.0 == bar)
            .map(|(entity, _)| entity);
        let corner = corner.unwrap_or_else(|| {
            world.init_resource::<CornerImage>();
            let image = world.resource::<CornerImage>().0.clone();
            let mut accessibility =
                AccessibilityNode::from(accesskit::Node::new(accesskit::Role::Button));
            accessibility.set_label("Show controls");
            let corner = world
                .spawn((
                    ControlsCorner(bar),
                    crate::inspection::InspectionExcluded,
                    bevy::ui_widgets::Button,
                    TabIndex(0),
                    accessibility,
                    Hovered::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(0),
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
                    right: px(0),
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
        });
        let expanded = [bar, corner].into_iter().any(|entity| {
            world.get::<Hovered>(entity).is_some_and(|hover| hover.0)
                || contains_focus(world, entity)
        });
        let visibility = if expanded {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let changed = world
            .get_mut::<Visibility>(bar)
            .unwrap()
            .set_if_neq(visibility);
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
        if changed && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_reveals_on_hover_and_preserves_keyboard_access() {
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<ThemeSettings>();
        world.init_resource::<InputFocus>();
        let root = world.spawn_empty().id();
        let bar = world.spawn(super::super::toolbar_bundle(root)).id();
        let button = world.spawn(ChildOf(bar)).id();
        update(&mut world);
        let corner = world
            .query_filtered::<Entity, With<ControlsCorner>>()
            .single(&world)
            .unwrap();
        let glyph = world.get::<Children>(corner).unwrap()[0];
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        assert_eq!(world.get::<Children>(root).unwrap()[0], corner);
        world.entity_mut(corner).insert(Hovered(true));
        update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(*world.get::<Visibility>(glyph).unwrap(), Visibility::Hidden);
        world.entity_mut(corner).insert(Hovered(false));
        world.entity_mut(bar).insert(Hovered(true));
        update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world.entity_mut(bar).insert(Hovered(false));
        update(&mut world);
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        world
            .resource_mut::<InputFocus>()
            .set(corner, bevy::input_focus::FocusCause::Navigated);
        update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world
            .resource_mut::<InputFocus>()
            .set(button, bevy::input_focus::FocusCause::Navigated);
        update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(bar).unwrap(),
            Visibility::Inherited
        );
        world.resource_mut::<InputFocus>().clear();
        update(&mut world);
        assert_eq!(*world.get::<Visibility>(bar).unwrap(), Visibility::Hidden);
        assert_eq!(world.resource::<Assets<Image>>().len(), 1);
        assert_eq!(
            world
                .query_filtered::<Entity, With<ControlsCorner>>()
                .iter(&world)
                .count(),
            1
        );
    }

    #[test]
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
                crate::tokens::TokenValue::Color([255, 0, 0, 255]),
            ),
            (
                Token::ControlsCornerTransparency,
                crate::tokens::TokenValue::Number(75.0),
            ),
        ]);
        let saved = serde_json::to_string(world.resource::<ThemeSettings>()).unwrap();
        let restored: ThemeSettings = serde_json::from_str(&saved).unwrap();
        assert!(restored.validate());
        world.insert_resource(restored);
        update(&mut world);
        let image = world
            .query_filtered::<&ImageNode, With<CornerGlyph>>()
            .single(&world)
            .unwrap();
        assert_eq!(image.color, Color::srgba(1.0, 0.0, 0.0, 0.25));
        assert!(Token::ControlsCornerTransparency.parse("101").is_none());
        assert!(Token::ControlsCornerTransparency.parse("-1").is_none());
        assert!(Token::ControlsCornerTransparency.parse("NaN").is_none());
    }
}
