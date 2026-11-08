use crate::native::{token_style::Tokens, tokens::Token};
use bevy::{ecs::query::QueryData, prelude::*};

#[derive(Component, Clone, Copy)]
pub(crate) struct WidthToken(pub Token, pub bool);

#[derive(Component, Clone, Copy)]
pub(crate) struct RadiusToken(pub Token);

#[derive(Clone, Copy, Default, PartialEq)]
struct Metric {
    base: f32,
    applied: f32,
    ready: bool,
}

impl Metric {
    fn needed(&self, current: Val) -> bool {
        matches!(current, Val::Px(value) if value != 0.0 || self.base != 0.0)
    }

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

#[derive(Component, Clone, Copy, Default, PartialEq)]
struct Metrics {
    spacing: [Metric; 2],
    padding: [Metric; 4],
    font: Metric,
}

#[derive(Component, Clone, Copy, Default, PartialEq)]
struct IconMetrics([Metric; 2]);

pub(crate) fn install(app: &mut App) {
    app.register_required_components::<Node, Metrics>()
        .register_required_components::<crate::native::icons::IconStyle, IconMetrics>();
}

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct IconData {
    entity: Entity,
    style: &'static mut crate::native::icons::IconStyle,
    metrics: &'static mut IconMetrics,
}

pub(crate) fn icons(mut tokens: Tokens, mut icons: Query<IconData>) {
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeIconMetrics);
    tokens.refresh();
    if crate::host::performance::enabled() {
        crate::host::performance::add(crate::host::performance::Count::IconMetricsVisited, icons.iter().len() as u64);
    }
    for icon in icons.iter() {
        tokens.prepare_metric(icon.entity, "icon-size", Token::IconSize);
        tokens.prepare_metric(icon.entity, "icon-padding", Token::IconPadding);
    }
    tokens.finish_preparing();
    let apply = |IconDataItem {
                     entity,
                     mut style,
                     mut metrics,
                 }: IconDataItem| {
        let mut next = *metrics;
        let size_ratio = tokens.metric(entity, "icon-size", Token::IconSize).number()
            / Token::IconSize.default_value(Default::default()).number();
        let padding_ratio = tokens
            .metric(entity, "icon-padding", Token::IconPadding)
            .number()
            / Token::IconPadding
                .default_value(Default::default())
                .number();
        let size = next.0[0].scale(style.size, size_ratio);
        let padding = next.0[1].scale(style.padding, padding_ratio);
        if (size, padding) != (style.size, style.padding) {
            style.size = size;
            style.padding = padding;
        }
        metrics.set_if_neq(next);
    };
    if icons.iter().len() >= 1024 {
        icons.par_iter_mut().for_each(apply);
    } else {
        icons.iter_mut().for_each(apply);
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct LayoutData {
    entity: Entity,
    node: &'static mut Node,
    font: Option<&'static mut TextFont>,
    metrics: &'static mut Metrics,
    width: Option<&'static WidthToken>,
    radius: Option<&'static RadiusToken>,
    icon: Has<crate::native::icons::IconButton>,
}

struct RequiredMetrics {
    spacing: bool,
    padding: bool,
    font: bool,
}

impl RequiredMetrics {
    fn new(node: &Node, metrics: &Metrics, font: Option<&TextFont>, icon: bool) -> Self {
        Self {
            spacing: !icon && (metrics.spacing[0].needed(node.column_gap) || metrics.spacing[1].needed(node.row_gap)),
            padding: !icon && [node.padding.left, node.padding.right, node.padding.top, node.padding.bottom].into_iter().zip(&metrics.padding).any(|(value, metric)| metric.needed(value)),
            font: font.is_some_and(|font| matches!(font.font_size, FontSize::Px(value) if value != 0.0 || metrics.font.base != 0.0)),
        }
    }

    fn any(&self) -> bool {
        self.spacing || self.padding || self.font
    }
}

pub(crate) fn layout(mut tokens: Tokens, mut nodes: Query<LayoutData>) {
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeLayoutMetrics);
    tokens.refresh();
    if crate::host::performance::enabled() {
        crate::host::performance::add(crate::host::performance::Count::NodeMetricsVisited, nodes.iter().len() as u64);
    }
    let mut work = 0;
    for node in nodes.iter() {
        let required = RequiredMetrics::new(node.node, node.metrics, node.font, node.icon);
        if required.any() || node.width.is_some() || node.radius.is_some() {
            work += 1;
        }
        if required.spacing {
            tokens.prepare_metric(node.entity, "spacing", Token::Spacing);
        }
        if required.padding {
            tokens.prepare_metric(node.entity, "padding", Token::Padding);
        }
        if required.font {
            tokens.prepare_metric(node.entity, "font-size", Token::FontSize);
        }
        if let Some(width) = node.width {
            tokens.prepare(node.entity, [width.0]);
        }
        if let Some(radius) = node.radius {
            tokens.prepare(node.entity, [radius.0]);
        }
    }
    tokens.finish_preparing();
    crate::host::performance::add(crate::host::performance::Count::NodeMetricsActive, work as u64);
    let apply = |LayoutDataItem {
                     entity,
                     mut node,
                     font,
                     mut metrics,
                     width,
                     radius,
                     icon,
                 }: LayoutDataItem| {
        let required = RequiredMetrics::new(&node, &metrics, font.as_deref(), icon);
        if !required.any() && width.is_none() && radius.is_none() {
            return;
        }
        let mut next = *metrics;
        if required.spacing {
            let gap = tokens.metric(entity, "spacing", Token::Spacing).number() / 8.0;
            let column_gap = next.spacing[0].length(node.column_gap, gap);
            let row_gap = next.spacing[1].length(node.row_gap, gap);
            if node.column_gap != column_gap {
                node.column_gap = column_gap;
            }
            if node.row_gap != row_gap {
                node.row_gap = row_gap;
            }
        }
        if required.padding {
            let ratio = tokens.metric(entity, "padding", Token::Padding).number() / 8.0;
            let padding = UiRect {
                left: next.padding[0].length(node.padding.left, ratio),
                right: next.padding[1].length(node.padding.right, ratio),
                top: next.padding[2].length(node.padding.top, ratio),
                bottom: next.padding[3].length(node.padding.bottom, ratio),
            };
            if node.padding != padding {
                node.padding = padding;
            }
        }
        if let Some(width) = width {
            let value = px(tokens.resolve(entity, width.0).0.number());
            let current = if width.1 { node.max_width } else { node.width };
            if current != value {
                if width.1 {
                    node.max_width = value;
                } else {
                    node.width = value;
                }
            }
        }
        if let Some(radius) = radius {
            let value = BorderRadius::all(px(tokens.resolve(entity, radius.0).0.number()));
            if node.border_radius != value {
                node.border_radius = value;
            }
        }
        if required.font
            && let Some(mut font) = font
            && let FontSize::Px(value) = font.font_size
        {
            let font_ratio = tokens.metric(entity, "font-size", Token::FontSize).number() / 16.0;
            let size = next.font.scale(value, font_ratio);
            if size != value {
                font.font_size = FontSize::Px(size);
            }
        }
        metrics.set_if_neq(next);
    };
    if work >= 1024 {
        nodes.par_iter_mut().for_each(apply);
    } else {
        nodes.iter_mut().for_each(apply);
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::native::tokens::{ThemeSettings, TokenValue};

    #[test]
    fn initially_empty_metrics_follow_late_fields_bindings_and_reparenting() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        world.register_required_components::<Node, Metrics>();
        let mut overrides = crate::native::tokens::TokenOverrides::default();
        overrides.set(Token::Spacing, TokenValue::Number(16.0));
        overrides.set(Token::Padding, TokenValue::Number(24.0));
        overrides.set(Token::FontSize, TokenValue::Number(32.0));
        let parent = world.spawn(overrides).id();
        let other = world.spawn_empty().id();
        let entity = world.spawn((Node::default(), ChildOf(parent))).id();
        world.run_system_cached(layout).unwrap();
        world.clear_trackers();
        world.run_system_cached(layout).unwrap();
        assert!(!world.entity(entity).get_ref::<Node>().unwrap().is_changed());
        assert!(
            !world
                .entity(entity)
                .get_ref::<Metrics>()
                .unwrap()
                .is_changed()
        );
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.column_gap = px(3);
        node.padding = UiRect::all(px(2));
        drop(node);
        world.entity_mut(entity).insert((
            TextFont {
                font_size: FontSize::Px(20.0),
                ..default()
            },
            crate::native::canvas_host::authoring::AuthoredTokenUses(
                std::collections::BTreeMap::from([("font-size".into(), Token::Spacing)]),
            ),
        ));
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(6));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(6))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(20.0)
        );
        world.entity_mut(entity).insert(ChildOf(other));
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(3));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(2))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(10.0)
        );
        world
            .entity_mut(entity)
            .remove::<crate::native::canvas_host::authoring::AuthoredTokenUses>();
        world.run_system_cached(layout).unwrap();
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(20.0)
        );
    }

    #[test]
    fn zero_theme_ratios_keep_authored_bases_and_zero_bound_controls_still_update() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        world.register_required_components::<Node, Metrics>();
        let entity = world
            .spawn((
                Node {
                    column_gap: px(4),
                    padding: UiRect::all(px(6)),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                crate::native::canvas_host::authoring::AuthoredTokenUses(
                    std::collections::BTreeMap::from([("font-size".into(), Token::Spacing)]),
                ),
            ))
            .id();
        let control = world
            .spawn((
                Node::default(),
                WidthToken(Token::Spacing, false),
                RadiusToken(Token::Padding),
            ))
            .id();
        world.run_system_cached(layout).unwrap();
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Spacing, TokenValue::Number(0.0));
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Padding, TokenValue::Number(0.0));
        for _ in 0..3 {
            world.run_system_cached(layout).unwrap();
        }
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(0));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(0))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(0.0)
        );
        assert_eq!(world.get::<Node>(control).unwrap().width, px(0));
        assert_eq!(
            world.get::<Node>(control).unwrap().border_radius,
            BorderRadius::all(px(0))
        );
        world.get_mut::<Node>(entity).unwrap().column_gap = px(3);
        world.get_mut::<TextFont>(entity).unwrap().font_size = FontSize::Px(10.0);
        world.run_system_cached(layout).unwrap();
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Spacing, TokenValue::Number(16.0));
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Padding, TokenValue::Number(16.0));
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(6));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(12))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(10.0)
        );
        assert_eq!(world.get::<Node>(control).unwrap().width, px(16));
        assert_eq!(
            world.get::<Node>(control).unwrap().border_radius,
            BorderRadius::all(px(16))
        );
    }

    #[cfg_attr(test, test)]
    fn spacing_padding_and_type_scale_without_compounding_or_losing_authored_changes() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<ThemeSettings>();
        world.register_required_components::<Node, Metrics>();
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
        world.run_system_cached(layout).unwrap();
        for token in [Token::Spacing, Token::Padding, Token::FontSize] {
            world.resource_mut::<ThemeSettings>().global.set(
                token,
                TokenValue::Number(token.default_value(Default::default()).number() * 2.0),
            );
        }
        for _ in 0..20 {
            world.run_system_cached(layout).unwrap();
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
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(12));
        world.resource_mut::<ThemeSettings>().global.0.clear();
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(entity).unwrap().column_gap, px(6));
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(12))
        );
    }

    #[cfg_attr(test, test)]
    fn metrics_follow_reparenting_authored_bindings_and_late_nodes_without_dirtying_steady_state() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        world.register_required_components::<Node, Metrics>();
        let mut values = crate::native::tokens::TokenOverrides::default();
        values.set(Token::FontSize, TokenValue::Number(24.0));
        values.set(Token::Padding, TokenValue::Number(16.0));
        let parent = world.spawn(values).id();
        let other = world.spawn_empty().id();
        let entity = world
            .spawn((
                Node {
                    padding: UiRect::all(px(12)),
                    ..default()
                },
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                WidthToken(Token::Width, false),
                ChildOf(parent),
            ))
            .id();
        world.run_system_cached(layout).unwrap();
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(24))
        );
        assert_eq!(world.get::<Node>(entity).unwrap().width, px(248));
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(30.0)
        );
        world.clear_trackers();
        world.run_system_cached(layout).unwrap();
        assert!(!world.entity(entity).get_ref::<Node>().unwrap().is_changed());
        assert!(
            !world
                .entity(entity)
                .get_ref::<TextFont>()
                .unwrap()
                .is_changed()
        );
        assert!(
            !world
                .entity(entity)
                .get_ref::<Metrics>()
                .unwrap()
                .is_changed()
        );
        world.entity_mut(entity).insert(ChildOf(other));
        world.run_system_cached(layout).unwrap();
        assert_eq!(
            world.get::<Node>(entity).unwrap().padding,
            UiRect::all(px(12))
        );
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(20.0)
        );
        world
            .entity_mut(entity)
            .insert(crate::native::canvas_host::authoring::AuthoredTokenUses(
                std::collections::BTreeMap::from([("font-size".into(), Token::Spacing)]),
            ));
        world.run_system_cached(layout).unwrap();
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(10.0)
        );
        world
            .entity_mut(entity)
            .remove::<crate::native::canvas_host::authoring::AuthoredTokenUses>();
        world.run_system_cached(layout).unwrap();
        assert_eq!(
            world.get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(20.0)
        );
        let late = world
            .spawn((
                Node {
                    column_gap: px(7),
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::Spacing, TokenValue::Number(16.0));
        world.run_system_cached(layout).unwrap();
        assert_eq!(world.get::<Node>(late).unwrap().column_gap, px(14));
    }

    #[cfg_attr(test, test)]
    fn icon_metrics_retain_state_and_follow_theme_and_authored_edits() {
        let mut world = World::new();
        world.init_resource::<ThemeSettings>();
        world.register_required_components::<crate::native::icons::IconStyle, IconMetrics>();
        let entity = world.spawn(crate::native::icons::IconStyle::default()).id();
        world.run_system_cached(icons).unwrap();
        world.clear_trackers();
        world.run_system_cached(icons).unwrap();
        assert!(
            !world
                .entity(entity)
                .get_ref::<IconMetrics>()
                .unwrap()
                .is_changed()
        );
        assert!(
            !world
                .entity(entity)
                .get_ref::<crate::native::icons::IconStyle>()
                .unwrap()
                .is_changed()
        );
        world
            .resource_mut::<ThemeSettings>()
            .global
            .set(Token::IconSize, TokenValue::Number(48.0));
        world.run_system_cached(icons).unwrap();
        assert_eq!(
            world
                .get::<crate::native::icons::IconStyle>(entity)
                .unwrap()
                .size,
            48.0
        );
        world
            .get_mut::<crate::native::icons::IconStyle>(entity)
            .unwrap()
            .size = 30.0;
        world.run_system_cached(icons).unwrap();
        assert_eq!(
            world
                .get::<crate::native::icons::IconStyle>(entity)
                .unwrap()
                .size,
            60.0
        );
        world.resource_mut::<ThemeSettings>().global.0.clear();
        world.run_system_cached(icons).unwrap();
        assert_eq!(
            world
                .get::<crate::native::icons::IconStyle>(entity)
                .unwrap()
                .size,
            30.0
        );
    }

    crate::native::laboratory_cases! {
        spacing_padding_and_type_scale_without_compounding_or_losing_authored_changes,
        metrics_follow_reparenting_authored_bindings_and_late_nodes_without_dirtying_steady_state,
        icon_metrics_retain_state_and_follow_theme_and_authored_edits,
    }
}
