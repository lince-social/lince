use super::*;
use crate::native::icons::{IconButton, IconStyle};
use bevy::{ecs::query::QueryData, text::TextCursorStyle};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct StyleData {
    entity: Entity,
    background: Option<&'static BackgroundToken>,
    text: Option<&'static TextToken>,
    border: Option<&'static BorderToken>,
    outline: Option<&'static OutlineToken>,
    cursor: Option<&'static CursorToken>,
    background_color: Option<&'static mut BackgroundColor>,
    text_color: Option<&'static mut TextColor>,
    border_color: Option<&'static mut BorderColor>,
    outline_color: Option<&'static mut Outline>,
    cursor_color: Option<&'static mut TextCursorStyle>,
    image: Option<&'static mut ImageNode>,
    node: Option<&'static mut Node>,
    icon: Has<IconButton>,
    item: Has<CanvasItem>,
    preview: Has<crate::native::sand_store::PreviewKind>,
}

type Styled = Or<(
    With<BackgroundToken>,
    With<TextToken>,
    With<BorderToken>,
    With<OutlineToken>,
    With<CursorToken>,
    With<CanvasItem>,
    With<crate::native::sand_store::PreviewKind>,
)>;

fn background_token(tokens: &Tokens, entity: Entity, item: bool, binding: Token) -> Token {
    if item && tokens.kind(entity).is_some() && !tokens.has_use(entity, "background") {
        Token::SandBackground
    } else {
        binding
    }
}

fn text_token(tokens: &Tokens, entity: Entity, binding: Token) -> Token {
    if tokens.inherited_kind(entity).is_some() && !tokens.has_use(entity, "text") {
        Token::SandInk
    } else {
        binding
    }
}

pub(super) fn apply(
    mut tokens: Tokens,
    mut queries: ParamSet<(
        Query<StyleData, Styled>,
        Query<
            Entity,
            Or<(
                Changed<BackgroundToken>,
                Changed<TextToken>,
                Changed<BorderToken>,
                Changed<OutlineToken>,
                Changed<CursorToken>,
                Changed<BackgroundColor>,
                Changed<TextColor>,
                Changed<BorderColor>,
                Changed<Outline>,
                Changed<TextCursorStyle>,
                Changed<ImageNode>,
                Changed<Node>,
                Added<IconButton>,
            )>,
        >,
    )>,
    mut commands: Commands,
) {
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeStyles);
    tokens.refresh();
    let changed: std::collections::HashSet<_> = queries.p1().iter().collect();
    let mut styles = queries.p0();
    let mut pending = Vec::new();
    for style in styles.iter() {
        let entity = style.entity;
        if !tokens.invalidated(entity) && !changed.contains(&entity) {
            continue;
        }
        pending.push(entity);
        if style.item && tokens.kind(entity).is_some() {
            tokens.prepare(
                entity,
                [
                    Token::SandBackground,
                    Token::SandBorder,
                    Token::Roundness,
                    Token::BorderWidth,
                ],
            );
        }
        if style.preview {
            tokens.prepare(entity, [Token::Roundness]);
        }
        if let Some(binding) = style
            .background
            .filter(|_| !style.icon || tokens.has_use(entity, "background"))
        {
            let token = background_token(&tokens, entity, style.item, binding.0);
            tokens.prepare(entity, [token]);
        }
        if let Some(binding) = style.text {
            tokens.prepare_kind(entity);
            let token = text_token(&tokens, entity, binding.0);
            tokens.prepare(entity, [token]);
        }
        if let Some(binding) = style
            .border
            .filter(|_| !style.icon || tokens.has_use(entity, "border"))
        {
            tokens.prepare(entity, [binding.0]);
        }
        if let Some(binding) = style.outline {
            tokens.prepare(entity, [binding.0]);
        }
        if let Some(binding) = style.cursor {
            tokens.prepare(entity, [binding.0]);
        }
    }
    tokens.finish_preparing();
    for entity in pending {
        let Ok(mut style) = styles.get_mut(entity) else {
            continue;
        };
        let entity = style.entity;
        if style.item && tokens.kind(entity).is_some() {
            let clock = tokens.clock(entity);
            let background = if clock {
                Color::NONE
            } else {
                tokens
                    .resolve(entity, Token::SandBackground)
                    .0
                    .linear_color()
            };
            let border =
                BorderColor::all(tokens.resolve(entity, Token::SandBorder).0.linear_color());
            if let Some(color) = style.background_color.as_mut() {
                color.set_if_neq(BackgroundColor(background));
            }
            if let Some(color) = style.border_color.as_mut() {
                color.set_if_neq(border);
            } else {
                commands.entity(entity).insert(border);
            }
            if let Some(node) = style.node.as_mut() {
                let radius =
                    BorderRadius::all(px(tokens.resolve(entity, Token::Roundness).0.number()));
                let border = UiRect::all(px(if clock {
                    0.0
                } else {
                    tokens.resolve(entity, Token::BorderWidth).0.number()
                }));
                if node.border_radius != radius {
                    node.border_radius = radius;
                }
                if node.border != border {
                    node.border = border;
                }
            }
        }
        if style.preview
            && let Some(node) = style.node.as_mut()
        {
            let radius = BorderRadius::all(px(tokens.resolve(entity, Token::Roundness).0.number()));
            if node.border_radius != radius {
                node.border_radius = radius;
            }
        }
        if let Some(binding) = style
            .background
            .filter(|_| !style.icon || tokens.has_use(entity, "background"))
        {
            let value = tokens
                .resolve(
                    entity,
                    background_token(&tokens, entity, style.item, binding.0),
                )
                .0
                .linear_color();
            if let Some(color) = style.background_color.as_mut() {
                color.set_if_neq(BackgroundColor(value));
            }
        }
        if let Some(binding) = style.text {
            let value = tokens
                .resolve(entity, text_token(&tokens, entity, binding.0))
                .0
                .linear_color();
            if let Some(color) = style.text_color.as_mut() {
                color.set_if_neq(TextColor(value));
            }
            if let Some(image) = style.image.as_mut()
                && image.color != value
            {
                image.color = value;
            }
        }
        if let Some(binding) = style
            .border
            .filter(|_| !style.icon || tokens.has_use(entity, "border"))
        {
            let value = BorderColor::all(tokens.resolve(entity, binding.0).0.linear_color());
            if let Some(color) = style.border_color.as_mut() {
                color.set_if_neq(value);
            } else if style.item && tokens.kind(entity).is_some() {
                commands.entity(entity).insert(value);
            }
        }
        if let Some(binding) = style.outline
            && let Some(outline) = style.outline_color.as_mut()
        {
            let value = tokens.resolve(entity, binding.0).0.linear_color();
            if outline.color != value {
                outline.color = value;
            }
        }
        if let Some(binding) = style.cursor
            && let Some(cursor) = style.cursor_color.as_mut()
        {
            let value = tokens.resolve(entity, binding.0).0.linear_color();
            if cursor.color != value {
                cursor.color = value;
            }
        }
    }
}

pub(super) fn icons(
    mut tokens: Tokens,
    mut icons: Query<(Entity, &mut IconStyle, &mut AppliedIconColors)>,
) {
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeIconColors);
    tokens.refresh();
    for (entity, style, previous) in icons.iter() {
        for (index, (token, color)) in [Token::Ink, Token::Surface, Token::Accent]
            .into_iter()
            .zip([style.color, style.background, style.border_color])
            .enumerate()
        {
            if previous.bound[index] && color == previous.colors[index] {
                tokens.prepare(entity, [token]);
            }
        }
    }
    tokens.finish_preparing();
    for (entity, mut style, mut previous) in &mut icons {
        let mut next = *previous;
        let mut colors = [style.color, style.background, style.border_color];
        for (index, token) in [Token::Ink, Token::Surface, Token::Accent]
            .into_iter()
            .enumerate()
        {
            next.bound[index] &= colors[index] == next.colors[index];
            if next.bound[index] {
                colors[index] = tokens.resolve(entity, token).0.color();
            }
        }
        if colors != [style.color, style.background, style.border_color] {
            style.color = colors[0];
            style.background = colors[1];
            style.border_color = colors[2];
        }
        next.colors = colors;
        previous.set_if_neq(next);
    }
}

pub(super) fn clear(theme: Res<ThemeSettings>, clear: Option<ResMut<ClearColor>>) {
    if let Some(mut clear) = clear {
        let value = theme
            .resolve(Token::CanvasBackground, None, &TokenOverrides::default())
            .0
            .color();
        if clear.0 != value {
            clear.0 = value;
        }
    }
}
