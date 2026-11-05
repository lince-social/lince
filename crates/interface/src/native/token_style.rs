use crate::native::{
    canvas::CanvasItem,
    sand_store::{SandKind, StoredSand},
    sand_text::SandText,
    tokens::{SandStyleKind, ThemeSettings, Token, TokenOverrides, TokenValue},
    workspace::RecordPlacement,
};
use bevy::{
    ecs::{query::QueryState, system::SystemState},
    prelude::*,
};

pub use lince_interface::style::{
    BackgroundToken, BorderToken, CursorToken, OutlineToken, TextToken, background, border, text,
};

mod presentation;
pub(crate) mod resolver;
pub(crate) use resolver::Tokens;

#[derive(Component, Clone, Copy)]
pub(crate) struct AppliedSize(pub Vec2);

pub fn kind(world: &World, entity: Entity) -> Option<SandStyleKind> {
    if world.get::<crate::native::fiote::Fiote>(entity).is_some() {
        return Some(SandStyleKind::Fiote);
    }
    if world
        .get::<crate::native::map_castle::MapSand>(entity)
        .is_some()
    {
        return Some(SandStyleKind::Square);
    }
    if world
        .get::<crate::native::time_castle::TimeSettings>(entity)
        .is_some()
    {
        return Some(SandStyleKind::TimeCastle);
    }
    if let Some(preview) = world.get::<crate::native::sand_store::PreviewKind>(entity) {
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
        return Some(stored_kind(sand.kind));
    }
    world
        .get::<RecordPlacement>(entity)
        .map(|_| SandStyleKind::Record)
        .or_else(|| {
            (world.get::<CanvasItem>(entity).is_some()
                && world
                    .get::<crate::native::protein_area::RecordBinding>(entity)
                    .is_some())
            .then_some(
                if world.get::<crate::native::castle::Castle>(entity).is_some() {
                    SandStyleKind::Record
                } else {
                    SandStyleKind::Square
                },
            )
        })
}

fn stored_kind(kind: SandKind) -> SandStyleKind {
    match kind {
        #[cfg(feature = "stuff")]
        SandKind::Freedoom => SandStyleKind::Square,
        SandKind::Square
        | SandKind::Overview
        | SandKind::Operation
        | SandKind::AccessControl
        | SandKind::Visibility
        | SandKind::Trust
        | SandKind::HandCamera
        | SandKind::Video
        | SandKind::Sync
        | SandKind::Terminal
        | SandKind::Organ
        | SandKind::Configuration
        | SandKind::Ontology
        | SandKind::Todo => SandStyleKind::Square,
        SandKind::WorkTimer => SandStyleKind::TimeCastle,
        SandKind::Text => SandStyleKind::Text,
        SandKind::EditableText => SandStyleKind::EditableText,
    }
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
    let changed = overrides(world, entity) != values;
    if let Some(mut text) = world.get_mut::<SandText>(entity) {
        if text.tokens != values {
            text.tokens = values;
        }
    } else if let Some(mut current) = world.get_mut::<TokenOverrides>(entity) {
        current.set_if_neq(values);
    } else {
        world.entity_mut(entity).insert(values);
    }
    if changed {
        crate::native::protein_area::presentation::styles_changed(world, entity);
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
    if let Some(value) =
        crate::native::relation_castle::styles::override_value(world, entity, token)
    {
        return (value, "Relation color rule");
    }
    resolve_base(world, entity, token)
}

pub(crate) fn resolve_base(
    world: &World,
    entity: Entity,
    token: Token,
) -> (TokenValue, &'static str) {
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
        if let Some(source) = world.get::<crate::native::sand_store::PreviewSource>(current) {
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
        crate::native::token_metrics::install(app);
        app.register_required_components::<crate::native::icons::IconStyle, AppliedIconColors>();
        app.init_resource::<ThemeSettings>()
            .add_systems(
                PostUpdate,
                crate::native::token_metrics::icons
                    .after(ApplyTokenStyles)
                    .before(crate::native::icons::SyncIcons),
            )
            .add_systems(
                PostUpdate,
                crate::native::token_metrics::layout
                    .after(crate::native::sand::StyleButtons)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (
                    sizes,
                    (presentation::apply, presentation::icons),
                    presentation::clear,
                )
                    .chain()
                    .in_set(ApplyTokenStyles)
                    .before(bevy::ui::UiSystems::Prepare),
            );
    }
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApplyTokenStyles;

#[derive(Default)]
struct Sizes {
    query: Option<QueryState<(Entity, &'static CanvasItem, Option<&'static AppliedSize>)>>,
    tokens: Option<SystemState<Tokens<'static, 'static>>>,
    values: Vec<(Entity, Vec2, Option<Vec2>, Vec2)>,
}

fn sizes(world: &mut World, mut state: Local<Sizes>) {
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeSandSizes);
    let Sizes {
        query,
        tokens,
        values: sands,
    } = &mut *state;
    let query = query.get_or_insert_with(|| world.query());
    sands.clear();
    sands.extend(
        query
            .iter(world)
            .filter(|(entity, _, _)| kind(world, *entity).is_some())
            .map(|(entity, item, applied)| {
                (entity, item.size, applied.map(|size| size.0), Vec2::ZERO)
            }),
    );
    for &(entity, size, previous, _) in sands.iter() {
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
    }
    let tokens = tokens.get_or_insert_with(|| SystemState::new(world));
    {
        let mut tokens = tokens.get(world).unwrap();
        tokens.refresh();
        for &(entity, _, _, _) in sands.iter() {
            tokens.prepare(entity, [Token::Width, Token::Height]);
        }
        tokens.finish_preparing();
        for (entity, _, _, next) in sands.iter_mut() {
            *next = Vec2::new(
                tokens.resolve(*entity, Token::Width).0.number(),
                tokens.resolve(*entity, Token::Height).0.number(),
            );
        }
    }
    for &(entity, size, previous, next) in sands.iter() {
        if size != next {
            world.get_mut::<CanvasItem>(entity).unwrap().size = next;
        }
        if previous != Some(next) {
            world.entity_mut(entity).insert(AppliedSize(next));
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
struct AppliedIconColors {
    colors: [Color; 3],
    bound: [bool; 3],
}

impl Default for AppliedIconColors {
    fn default() -> Self {
        let defaults = crate::native::icons::IconStyle::default();
        Self {
            colors: [defaults.color, defaults.background, defaults.border_color],
            bound: [true; 3],
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::native::{
        container::BoxRoot, sand_store::spawn_sand, theme::Typography, tokens::ColorScheme,
    };
    use bevy::math::DVec2;

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
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
                crate::native::sand::Square,
                crate::native::sand_store::PreviewKind(SandStyleKind::Square),
                crate::native::sand_store::PreviewSource(a),
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
                crate::native::icons::IconStyle {
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
                .get::<crate::native::icons::IconStyle>(icon)
                .unwrap()
                .color,
            Color::BLACK
        );
    }

    #[cfg_attr(test, test)]
    fn image_glyphs_follow_ink_when_switching_themes() {
        let (mut app, root) = fixture();
        let glyph = app
            .world_mut()
            .spawn((ImageNode::default(), TextToken(Token::Ink), ChildOf(root)))
            .id();
        for scheme in [ColorScheme::Light, ColorScheme::Dark, ColorScheme::Light] {
            app.world_mut().resource_mut::<ThemeSettings>().scheme = scheme;
            app.update();
            let color = app.world().get::<ImageNode>(glyph).unwrap().color;
            assert_eq!(color, Token::Ink.default_value(scheme).color());
            assert_ne!(color, Token::Surface.default_value(scheme).color());
        }
    }

    #[cfg_attr(test, test)]
    fn styling_publishes_complete_theme_colors_before_same_tick_consumers() {
        #[derive(Resource, Default)]
        struct Observed(usize, Color);

        let (mut app, root) = fixture();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let sand = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
        app.world_mut()
            .entity_mut(sand)
            .remove::<BorderColor>()
            .insert((OutlineToken(Token::Accent), Outline::default()));
        let icon = app
            .world_mut()
            .spawn(crate::native::icons::IconStyle::default())
            .id();
        let glyph = app
            .world_mut()
            .spawn((ImageNode::default(), TextToken(Token::Ink)))
            .id();
        app.init_resource::<Observed>().add_systems(
            PostUpdate,
            (move |world: &mut World| {
                let expected = world.resource::<Observed>().1;
                assert_eq!(world.get::<BorderColor>(sand).unwrap().top, expected);
                assert_eq!(world.get::<Outline>(sand).unwrap().color, expected);
                assert_eq!(
                    world
                        .get::<crate::native::icons::IconStyle>(icon)
                        .unwrap()
                        .color,
                    expected
                );
                assert_eq!(world.get::<ImageNode>(glyph).unwrap().color, expected);
                world.resource_mut::<Observed>().0 += 1;
            })
            .after(ApplyTokenStyles),
        );
        for color in [[17, 61, 203, 255], [210, 33, 19, 0], [40, 190, 80, 255]] {
            let value = TokenValue::Color(color);
            app.world_mut().resource_mut::<Observed>().1 = value.color();
            for token in [Token::SandBorder, Token::Accent, Token::Ink] {
                app.world_mut()
                    .resource_mut::<ThemeSettings>()
                    .global
                    .set(token, value);
            }
            app.update();
        }
        assert_eq!(app.world().resource::<Observed>().0, 3);
    }

    crate::native::laboratory_cases! {
        image_glyphs_follow_ink_when_switching_themes,
        renders_global_type_and_individual_styles_and_preserves_later_resizing,
        nested_text_inherits_its_parent_and_can_override_it,
        styling_stays_unchanged_while_idle_and_keeps_explicit_icon_colors,
        styling_publishes_complete_theme_colors_before_same_tick_consumers,
    }
}
