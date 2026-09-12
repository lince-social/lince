use crate::{
    canvas::CanvasItem,
    sand_store::{SandKind, StoredSand},
    sand_text::SandText,
    tokens::{SandStyleKind, ThemeSettings, Token, TokenOverrides, TokenValue},
    workspace::RecordPlacement,
};
use bevy::{prelude::*, text::TextCursorStyle};

#[derive(Component, Clone, Copy)]
pub struct BackgroundToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct TextToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct BorderToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct OutlineToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct CursorToken(pub Token);
#[derive(Component, Clone, Copy)]
pub(crate) struct AppliedSize(pub Vec2);

pub fn background(token: Token) -> impl Bundle {
    (
        BackgroundColor(token.default_value(Default::default()).color()),
        BackgroundToken(token),
    )
}

pub fn text(token: Token) -> impl Bundle {
    (
        TextColor(token.default_value(Default::default()).color()),
        TextToken(token),
    )
}

pub fn border(token: Token) -> impl Bundle {
    (
        BorderColor::all(token.default_value(Default::default()).color()),
        BorderToken(token),
    )
}

pub fn kind(world: &World, entity: Entity) -> Option<SandStyleKind> {
    if let Some(preview) = world.get::<crate::sand_store::PreviewKind>(entity) {
        return Some(preview.0);
    }
    if let Some(text) = world.get::<SandText>(entity) {
        return Some(if text.editable {
            SandStyleKind::EditableText
        } else {
            SandStyleKind::Text
        });
    }
    if let Some(sand) = world.get::<StoredSand>(entity) {
        return Some(match sand.kind {
            SandKind::Square => SandStyleKind::Square,
            SandKind::Text => SandStyleKind::Text,
            SandKind::EditableText => SandStyleKind::EditableText,
        });
    }
    world
        .get::<RecordPlacement>(entity)
        .map(|_| SandStyleKind::Record)
}

pub fn overrides(world: &World, entity: Entity) -> TokenOverrides {
    world
        .get::<TokenOverrides>(entity)
        .cloned()
        .or_else(|| {
            world
                .get::<SandText>(entity)
                .map(|text| text.tokens.clone())
        })
        .unwrap_or_default()
}

pub fn set_overrides(world: &mut World, entity: Entity, values: TokenOverrides) {
    if let Some(mut text) = world.get_mut::<SandText>(entity) {
        if text.tokens != values {
            text.tokens = values;
        }
    } else if let Some(mut current) = world.get_mut::<TokenOverrides>(entity) {
        current.set_if_neq(values);
    } else {
        world.entity_mut(entity).insert(values);
    }
}

pub fn inherited_kind(world: &World, entity: Entity) -> Option<SandStyleKind> {
    let mut ancestor = Some(entity);
    while let Some(current) = ancestor {
        if let Some(kind) = kind(world, current) {
            return Some(kind);
        }
        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    None
}

pub fn resolve(world: &World, entity: Entity, token: Token) -> (TokenValue, &'static str) {
    let mut ancestor = Some(entity);
    let mut style_kind = None;
    while let Some(current) = ancestor {
        style_kind = style_kind.or_else(|| kind(world, current));
        let values = world
            .get::<TokenOverrides>(current)
            .or_else(|| world.get::<SandText>(current).map(|text| &text.tokens));
        if let Some(value) = values.and_then(|values| values.0.get(&token)) {
            return (
                *value,
                if current == entity {
                    "This Sand"
                } else {
                    "Parent Sand"
                },
            );
        }
        if let Some(source) = world.get::<crate::sand_store::PreviewSource>(current) {
            return resolve(world, source.0, token);
        }
        ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    world
        .resource::<ThemeSettings>()
        .resolve(token, style_kind, &TokenOverrides::default())
}

pub struct TokenStylePlugin;

impl Plugin for TokenStylePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ThemeSettings>()
            .add_systems(
                PostUpdate,
                crate::token_metrics::icons
                    .after(ApplyTokenStyles)
                    .before(crate::icons::SyncIcons),
            )
            .add_systems(
                PostUpdate,
                crate::token_metrics::layout
                    .after(crate::sand::StyleButtons)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                apply
                    .in_set(ApplyTokenStyles)
                    .before(bevy::ui::UiSystems::Prepare),
            );
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApplyTokenStyles;

fn apply(world: &mut World) {
    let sands: Vec<_> = world
        .query::<(Entity, &CanvasItem, Option<&AppliedSize>)>()
        .iter(world)
        .filter(|(entity, _, _)| kind(world, *entity).is_some())
        .map(|(entity, item, applied)| (entity, item.size, applied.map(|size| size.0)))
        .collect();
    for (entity, size, previous) in sands {
        let mut values = overrides(world, entity);
        let baseline = previous.unwrap_or_else(|| {
            let defaults = ThemeSettings::default();
            Vec2::new(
                defaults
                    .resolve(Token::Width, kind(world, entity), &Default::default())
                    .0
                    .number(),
                defaults
                    .resolve(Token::Height, kind(world, entity), &Default::default())
                    .0
                    .number(),
            )
        });
        {
            for (axis, token) in [Token::Width, Token::Height].into_iter().enumerate() {
                if size[axis] != baseline[axis] {
                    values.set(token, TokenValue::Number(size[axis]));
                }
            }
        }
        set_overrides(world, entity, values);
        let next = Vec2::new(
            resolve(world, entity, Token::Width).0.number(),
            resolve(world, entity, Token::Height).0.number(),
        );
        if size != next {
            world.get_mut::<CanvasItem>(entity).unwrap().size = next;
        }
        if previous != Some(next) {
            world.entity_mut(entity).insert(AppliedSize(next));
        }
        let background = resolve(world, entity, Token::SandBackground).0.color();
        let border = resolve(world, entity, Token::SandBorder).0.color();
        let radius = resolve(world, entity, Token::Roundness).0.number();
        let width = resolve(world, entity, Token::BorderWidth).0.number();
        if let Some(mut color) = world.get_mut::<BackgroundColor>(entity) {
            color.set_if_neq(BackgroundColor(background));
        }
        if let Some(mut color) = world.get_mut::<BorderColor>(entity) {
            color.set_if_neq(BorderColor::all(border));
        } else {
            world.entity_mut(entity).insert(BorderColor::all(border));
        }
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let next_radius = BorderRadius::all(px(radius));
            let next_border = UiRect::all(px(width));
            if node.border_radius != next_radius || node.border != next_border {
                node.border_radius = next_radius;
                node.border = next_border;
            }
        }
    }
    let previews: Vec<_> = world
        .query_filtered::<Entity, With<crate::sand_store::PreviewKind>>()
        .iter(world)
        .collect();
    for entity in previews {
        let radius = resolve(world, entity, Token::Roundness).0.number();
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let radius = BorderRadius::all(px(radius));
            if node.border_radius != radius {
                node.border_radius = radius;
            }
        }
    }
    let bindings: Vec<_> = world
        .query_filtered::<(
            Entity,
            Option<&BackgroundToken>,
            Option<&TextToken>,
            Option<&BorderToken>,
            Option<&OutlineToken>,
            Option<&CursorToken>,
        ), Or<(
            With<BackgroundToken>,
            With<TextToken>,
            With<BorderToken>,
            With<OutlineToken>,
            With<CursorToken>,
        )>>()
        .iter(world)
        .map(|(entity, bg, text, border, outline, cursor)| {
            (
                entity,
                bg.copied(),
                text.copied(),
                border.copied(),
                outline.copied(),
                cursor.copied(),
            )
        })
        .collect();
    for (entity, bg, text, border, outline, cursor) in bindings {
        let icon = world.get::<crate::icons::IconButton>(entity).is_some();
        if let Some(binding) = bg.filter(|_| !icon) {
            let token =
                if world.get::<CanvasItem>(entity).is_some() && kind(world, entity).is_some() {
                    Token::SandBackground
                } else {
                    binding.0
                };
            let value = resolve(world, entity, token).0.color();
            if let Some(mut color) = world.get_mut::<BackgroundColor>(entity) {
                color.set_if_neq(BackgroundColor(value));
            }
        }
        if let Some(binding) = text {
            let token = if inherited_kind(world, entity).is_some() {
                Token::SandInk
            } else {
                binding.0
            };
            let value = resolve(world, entity, token).0.color();
            if let Some(mut color) = world.get_mut::<TextColor>(entity) {
                color.set_if_neq(TextColor(value));
            }
        }
        if let Some(binding) = border.filter(|_| !icon) {
            let value = resolve(world, entity, binding.0).0.color();
            if let Some(mut color) = world.get_mut::<BorderColor>(entity) {
                color.set_if_neq(BorderColor::all(value));
            }
        }
        if let Some(binding) = outline {
            let value = resolve(world, entity, binding.0).0.color();
            if let Some(mut outline) = world.get_mut::<Outline>(entity) {
                if outline.color != value {
                    outline.color = value;
                }
            }
        }
        if let Some(binding) = cursor {
            let value = resolve(world, entity, binding.0).0.color();
            if let Some(mut cursor) = world.get_mut::<TextCursorStyle>(entity) {
                if cursor.color != value {
                    cursor.color = value;
                }
            }
        }
    }
    let icons: Vec<_> = world
        .query::<(Entity, &crate::icons::IconStyle, Option<&AppliedIconColors>)>()
        .iter(world)
        .map(|(entity, style, previous)| (entity, *style, previous.copied()))
        .collect();
    for (entity, style, previous) in icons {
        let defaults = crate::icons::IconStyle::default();
        let mut previous = previous.unwrap_or(AppliedIconColors {
            colors: [defaults.color, defaults.background, defaults.border_color],
            bound: [true; 3],
        });
        let mut next = [style.color, style.background, style.border_color];
        for (index, token) in [Token::Ink, Token::Surface, Token::Accent]
            .into_iter()
            .enumerate()
        {
            previous.bound[index] &= next[index] == previous.colors[index];
            if previous.bound[index] {
                next[index] = resolve(world, entity, token).0.color();
            }
        }
        if next != [style.color, style.background, style.border_color] {
            let mut style = world.get_mut::<crate::icons::IconStyle>(entity).unwrap();
            style.color = next[0];
            style.background = next[1];
            style.border_color = next[2];
        }
        previous.colors = next;
        if let Some(mut current) = world.get_mut::<AppliedIconColors>(entity) {
            current.set_if_neq(previous);
        } else {
            world.entity_mut(entity).insert(previous);
        }
    }
    if world.contains_resource::<ClearColor>() {
        let value = world
            .resource::<ThemeSettings>()
            .resolve(Token::CanvasBackground, None, &TokenOverrides::default())
            .0
            .color();
        let mut clear = world.resource_mut::<ClearColor>();
        if clear.0 != value {
            clear.0 = value;
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
struct AppliedIconColors {
    colors: [Color; 3],
    bound: [bool; 3],
}

pub(crate) mod tests {
    use super::*;
    use crate::{
        container::BoxRoot, sand_store::spawn_sand, theme::Typography, tokens::ColorScheme,
    };
    use bevy::math::DVec2;

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .add_plugins(TokenStylePlugin);
        let root = app.world_mut().spawn(BoxRoot).id();
        (app, root)
    }

    #[cfg_attr(test, test)]
    fn renders_global_type_and_individual_styles_and_preserves_later_resizing() {
        let (mut app, root) = fixture();
        let a = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
        let b = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
        app.update();
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Width, TokenValue::Number(400.0));
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .kinds
            .entry(SandStyleKind::Square)
            .or_default()
            .set(Token::Roundness, TokenValue::Number(12.0));
        let mut values = TokenOverrides::default();
        values.set(Token::SandBackground, TokenValue::Color([10, 20, 30, 255]));
        values.set(Token::Accent, TokenValue::Color([30, 20, 10, 255]));
        set_overrides(app.world_mut(), a, values);
        let preview = app
            .world_mut()
            .spawn((
                crate::sand::Square,
                crate::sand_store::PreviewKind(SandStyleKind::Square),
                crate::sand_store::PreviewSource(a),
                background(Token::SandBackground),
            ))
            .id();
        app.update();
        assert_eq!(app.world().get::<CanvasItem>(a).unwrap().size.x, 400.0);
        assert_eq!(
            app.world().get::<Node>(a).unwrap().border_radius,
            BorderRadius::all(px(12))
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(a).unwrap().0,
            Color::srgb_u8(10, 20, 30)
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(preview).unwrap().0,
            Color::srgb_u8(10, 20, 30)
        );
        assert_eq!(
            app.world().get::<Outline>(a).unwrap().color,
            Color::srgb_u8(30, 20, 10)
        );
        app.world_mut().get_mut::<CanvasItem>(a).unwrap().size.x = 500.0;
        app.update();
        app.world_mut().resource_mut::<ThemeSettings>().scheme = ColorScheme::Light;
        app.world_mut()
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Width, TokenValue::Number(600.0));
        app.update();
        assert_eq!(app.world().get::<CanvasItem>(a).unwrap().size.x, 500.0);
        assert_eq!(app.world().get::<CanvasItem>(b).unwrap().size.x, 600.0);
        assert_eq!(
            app.world().get::<BackgroundColor>(a).unwrap().0,
            Color::srgb_u8(10, 20, 30)
        );
        assert_eq!(
            app.world().get::<BackgroundColor>(b).unwrap().0,
            Token::SandBackground
                .default_value(ColorScheme::Light)
                .color()
        );
        let mut values = overrides(app.world(), a);
        values.0.remove(&Token::Width);
        set_overrides(app.world_mut(), a, values);
        app.update();
        assert_eq!(app.world().get::<CanvasItem>(a).unwrap().size.x, 600.0);
    }

    #[cfg_attr(test, test)]
    fn nested_text_inherits_its_parent_and_can_override_it() {
        let (mut app, root) = fixture();
        let sand = spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::EditableText,
            "Hello",
            DVec2::ZERO,
        );
        let text = app
            .world()
            .get::<StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        let parent_color = TokenValue::Color([100, 50, 20, 255]);
        let own_color = TokenValue::Color([20, 50, 100, 255]);
        let mut values = TokenOverrides::default();
        values.set(Token::SandInk, parent_color);
        set_overrides(app.world_mut(), sand, values);
        app.update();
        assert_eq!(
            app.world().get::<TextColor>(text).unwrap().0,
            parent_color.color()
        );
        assert_eq!(resolve(app.world(), text, Token::SandInk).1, "Parent Sand");
        let mut values = TokenOverrides::default();
        values.set(Token::SandInk, own_color);
        set_overrides(app.world_mut(), text, values);
        app.world_mut().resource_mut::<ThemeSettings>().scheme = ColorScheme::Light;
        app.update();
        assert_eq!(
            app.world().get::<TextColor>(text).unwrap().0,
            own_color.color()
        );
    }

    #[cfg_attr(test, test)]
    fn styling_stays_unchanged_while_idle_and_keeps_explicit_icon_colors() {
        let (mut app, root) = fixture();
        let sand = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
        let icon = app
            .world_mut()
            .spawn((
                crate::icons::IconStyle {
                    color: Color::BLACK,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        app.update();
        let changes = app
            .world()
            .entity(sand)
            .get_ref::<TokenOverrides>()
            .unwrap()
            .last_changed();
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(
            app.world()
                .entity(sand)
                .get_ref::<TokenOverrides>()
                .unwrap()
                .last_changed(),
            changes
        );
        app.world_mut().resource_mut::<ThemeSettings>().scheme = ColorScheme::Light;
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::icons::IconStyle>(icon)
                .unwrap()
                .color,
            Color::BLACK
        );
    }

    crate::laboratory_cases! {
        renders_global_type_and_individual_styles_and_preserves_later_resizing,
        nested_text_inherits_its_parent_and_can_override_it,
        styling_stays_unchanged_while_idle_and_keeps_explicit_icon_colors,
    }
}
