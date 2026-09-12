use crate::{token_style::resolve, tokens::Token};
use bevy::prelude::*;

#[derive(Component, Clone, Copy)]
pub(crate) struct WidthToken(pub Token, pub bool);

#[derive(Component, Clone, Copy)]
pub(crate) struct RadiusToken(pub Token);

#[derive(Clone, Copy, Default)]
struct Metric {
    base: f32,
    applied: f32,
    ready: bool,
}

impl Metric {
    fn scale(&mut self, current: f32, ratio: f32) -> f32 {
        if !self.ready || current != self.applied {
            self.base = current;
            self.ready = true;
        }
        self.applied = self.base * ratio;
        self.applied
    }

    fn length(&mut self, current: Val, ratio: f32) -> Val {
        match current {
            Val::Px(value) => px(self.scale(value, ratio)),
            other => other,
        }
    }
}

#[derive(Component, Clone, Default)]
struct Metrics {
    spacing: [Metric; 2],
    padding: [Metric; 4],
    font: Metric,
}

#[derive(Component, Clone, Default)]
struct IconMetrics([Metric; 3]);

pub(crate) fn icons(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<crate::icons::IconStyle>>()
        .iter(world)
        .collect();
    for entity in entities {
        let mut metrics = world
            .get::<IconMetrics>(entity)
            .cloned()
            .unwrap_or_default();
        let ratios = [Token::IconSize, Token::IconPadding, Token::IconRoundness].map(|token| {
            resolve(world, entity, token).0.number()
                / token.default_value(Default::default()).number()
        });
        let style = *world.get::<crate::icons::IconStyle>(entity).unwrap();
        let size = metrics.0[0].scale(style.size, ratios[0]);
        let padding = metrics.0[1].scale(style.padding, ratios[1]);
        let radius = metrics.0[2].scale(style.radius, ratios[2]);
        if (size, padding, radius) != (style.size, style.padding, style.radius) {
            let mut style = world.get_mut::<crate::icons::IconStyle>(entity).unwrap();
            style.size = size;
            style.padding = padding;
            style.radius = radius;
        }
        world.entity_mut(entity).insert(metrics);
    }
}

pub(crate) fn layout(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Node>>()
        .iter(world)
        .collect();
    for entity in entities {
        let mut metrics = world.get::<Metrics>(entity).cloned().unwrap_or_default();
        let gap = resolve(world, entity, Token::Spacing).0.number() / 8.0;
        let padding = resolve(world, entity, Token::Padding).0.number() / 8.0;
        let font = resolve(world, entity, Token::FontSize).0.number() / 16.0;
        let width = world
            .get::<WidthToken>(entity)
            .map(|binding| (resolve(world, entity, binding.0).0.number(), binding.1));
        let radius = world
            .get::<RadiusToken>(entity)
            .map(|binding| resolve(world, entity, binding.0).0.number());
        if world.get::<crate::icons::IconButton>(entity).is_none() {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let column_gap = metrics.spacing[0].length(node.column_gap, gap);
            let row_gap = metrics.spacing[1].length(node.row_gap, gap);
            let next = UiRect {
                left: metrics.padding[0].length(node.padding.left, padding),
                right: metrics.padding[1].length(node.padding.right, padding),
                top: metrics.padding[2].length(node.padding.top, padding),
                bottom: metrics.padding[3].length(node.padding.bottom, padding),
            };
            if node.column_gap != column_gap {
                node.column_gap = column_gap;
            }
            if node.row_gap != row_gap {
                node.row_gap = row_gap;
            }
            if node.padding != next {
                node.padding = next;
            }
        }
        if let Some((value, maximum)) = width {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let property = if maximum {
                &mut node.max_width
            } else {
                &mut node.width
            };
            if *property != px(value) {
                *property = px(value);
            }
        }
        if let Some(value) = radius {
            let mut node = world.get_mut::<Node>(entity).unwrap();
            let radius = BorderRadius::all(px(value));
            if node.border_radius != radius {
                node.border_radius = radius;
            }
        }
        if let Some(mut text) = world.get_mut::<TextFont>(entity)
            && let FontSize::Px(value) = text.font_size
        {
            let next = metrics.font.scale(value, font);
            if next != value {
                text.font_size = FontSize::Px(next);
            }
        }
        world.entity_mut(entity).insert(metrics);
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::tokens::{ThemeSettings, TokenValue};

    #[cfg_attr(test, test)]
    fn spacing_padding_and_type_scale_without_compounding_or_losing_authored_changes() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<ThemeSettings>();
        let entity = world
            .spawn((
                Node {
                    column_gap: px(4),
                    padding: UiRect::all(px(12)),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
            ))
            .id();
        layout(&mut world);
        for token in [Token::Spacing, Token::Padding, Token::FontSize] {
            world.resource_mut::<ThemeSettings>().global.set(
                token,
                TokenValue::Number(token.default_value(Default::default()).number() * 2.0),
            );
        }
        for _ in 0..20 {
            layout(&mut world);
        }
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(8));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(24))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(40.0)
        );
        world.get_mut::<Node>(entity).unwrap().column_gap = px(6);
        layout(&mut world);
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(12));
        world.resource_mut::<ThemeSettings>().global.0.clear();
        layout(&mut world);
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(6));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(12))
        );
    }

    crate::laboratory_cases! {
        spacing_padding_and_type_scale_without_compounding_or_losing_authored_changes,
    }
}
