// non-lean-yet
// Scope: struct GroupRows, struct LayoutPreparation, fn update, fn update_layout.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 125.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (46).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   164.

use super::*;
use crate::host::performance::{self, Count, Stage};

#[derive(Default)]
struct GroupRows {
    rows: Vec<(Entity, grouping::Cell, f32)>,
    placed: bool,
}

struct LayoutPreparation {
    editors: QueryState<(
        &'static EditableText,
        &'static bevy::text::TextLayoutInfo,
        &'static ComputedNode,
        &'static mut EditorSize,
        &'static mut Node,
        &'static mut TextLayout,
        Option<&'static mut bevy::ui::widget::TextScroll>,
        Option<&'static super::super::presentation::EditorSettings>,
    )>,
    row_query: QueryState<(Entity, &'static Row)>,
    prepared: Vec<(Entity, Entity, usize, std::sync::Arc<Config>)>,
    heights: HashMap<Entity, Vec<f32>>,
    grouped: HashMap<Entity, GroupRows>,
    offsets: HashMap<(Entity, u32), Vec<f32>>,
    grouping: grouping::Layout,
}

impl FromWorld for LayoutPreparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            editors: world.query(),
            row_query: world.query(),
            prepared: Vec::new(),
            heights: HashMap::new(),
            grouped: HashMap::new(),
            offsets: HashMap::new(),
            grouping: grouping::Layout::from_world(world),
        }
    }
}

pub(super) fn update(world: &mut World) {
    world
        .run_system_cached(update_layout)
        .expect("lay out Record rows");
}

fn update_layout(world: &mut World, mut preparation: Local<LayoutPreparation>) {
    let _layout = performance::Span::new(Stage::RecordRowLayout);
    let LayoutPreparation {
        editors,
        row_query,
        prepared,
        heights,
        grouped,
        offsets,
        grouping,
    } = &mut *preparation;
    grouping.begin_frame();
    super::super::placement::begin_frame(world);
    let mut resized = false;
    for (text, layout, computed, mut size, mut node, mut wrapping, scroll, settings) in
        editors.iter_mut(world)
    {
        let settings = settings.map(|settings| &settings.0.0);
        let fixed_width = settings.is_some_and(|settings| settings.contains_key("width"));
        let fixed_height = settings.is_some_and(|settings| settings.contains_key("height"));
        let fixed_wrap = settings.is_some_and(|settings| settings.contains_key("wrap"));
        if size.mode == OverflowMode::GrowRight && !fixed_width && !fixed_wrap {
            let content = text.value();
            if content != size.observed.as_str() {
                size.observed = content.to_string();
                size.measuring = true;
                wrapping.linebreak = bevy::text::LineBreak::NoWrap;
                resized = true;
                continue;
            }
        }
        let measured = layout.size * computed.inverse_scale_factor();
        if size.mode == OverflowMode::GrowDown
            && measured.y > 0.0
            && computed.content_box().height() * computed.inverse_scale_factor() >= measured.y
            && let Some(mut scroll) = scroll
            && scroll.0 != Vec2::ZERO
        {
            scroll.0 = Vec2::ZERO;
        }
        let inset = (computed.size().y - computed.content_box().height()).max(0.0)
            * computed.inverse_scale_factor();
        let height = (measured.y + inset).ceil().max(size.minimum.y);
        if !fixed_height && height.is_finite() && node.height != px(height) {
            node.height = px(height);
            resized = true;
        }
        if !fixed_width && (size.mode == OverflowMode::ScrollRight || size.measuring) {
            let maximum = if size.mode == OverflowMode::GrowRight {
                size.maximum_width
            } else {
                100_000.0
            };
            let width = measured.x.ceil().max(size.minimum.x).min(maximum);
            if width.is_finite() && node.width != px(width) {
                node.width = px(width);
                resized = true;
            }
        }
        if size.measuring {
            size.measuring = false;
            if !fixed_wrap {
                wrapping.linebreak = bevy::text::LineBreak::WordBoundary;
            }
            resized = true;
        }
    }
    if resized {
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    }
    let prepare = performance::Span::new(Stage::RecordRowPrepare);
    prepared.extend(
        row_query
            .iter(world)
            .map(|(entity, row)| (entity, row.area, row.index, row.config.clone())),
    );
    performance::add(Count::RecordRowsPrepared, prepared.len() as u64);
    let had_rows = !prepared.is_empty();
    for (entity, area, index, config) in prepared.iter() {
        let (count, content_height) = world
            .get::<Children>(*entity)
            .into_iter()
            .flatten()
            .filter(|child| {
                world.get::<Node>(**child).is_some_and(|node| {
                    node.display != Display::None && node.position_type != PositionType::Absolute
                })
            })
            .filter_map(|child| world.get::<ComputedNode>(*child))
            .fold((0usize, 0.0f32), |(count, height), node| {
                (
                    count + 1,
                    height + node.size().y * node.inverse_scale_factor(),
                )
            });
        let height = content_height + 24.0 + (count.saturating_sub(1) as f32 * 8.0);
        let height = config
            .viewport_height
            .or_else(|| config.fiote.then_some(config.width * 0.75))
            .unwrap_or(height)
            .min(config.max_height.unwrap_or(f32::MAX));
        if let Some(cell) = world.get::<grouping::Cell>(*entity).copied() {
            grouped
                .entry(*area)
                .or_default()
                .rows
                .push((*entity, cell, height.max(48.0)));
            continue;
        }
        let bands = heights.entry(*area).or_default();
        let band = index / config.columns;
        if bands.len() <= band {
            bands.resize(band + 1, 0.0);
        }
        let value = &mut bands[band];
        *value = value.max(height.max(48.0));
    }
    drop(prepare);
    for (entity, area, index, config) in prepared.drain(..) {
        if let Some(group) = grouped.get_mut(&area)
            && !group.placed
            && !group.rows.is_empty()
        {
            grouping.apply(world, area, &config, &group.rows);
            group.placed = true;
        }
        if world.get::<grouping::Cell>(entity).is_some() {
            continue;
        }
        let Some(influence) = world.get::<InfluenceArea>(area) else {
            continue;
        };
        let origin = DVec2::from_array(influence.center) - DVec2::from_array(influence.size) * 0.5;
        let group = index / config.columns;
        let bands = heights.get(&area).map(Vec::as_slice).unwrap_or_default();
        let height = bands.get(group).copied().unwrap_or(160.0);
        let offsets = offsets.entry((area, config.gap.to_bits())).or_default();
        if offsets.is_empty() {
            let mut offset = 0.0;
            offsets.extend(bands.iter().map(|height| {
                let current = offset;
                offset += height + config.gap;
                current
            }));
        }
        let y = offsets.get(group).copied().unwrap_or(0.0);
        let inset = if config.group_with_source { 0.0 } else { 12.0 };
        let position = if config.group_with_source && index == 0 {
            DVec2::from_array(influence.center)
        } else {
            origin
                + DVec2::new(
                    f64::from(
                        inset
                            + (index % config.columns) as f32 * (config.width + config.gap)
                            + config.width / 2.0,
                    ),
                    f64::from(inset + y + height / 2.0),
                )
        };
        let size = Vec2::new(config.width, height);
        place(world, entity, position, size);
        if config.group_with_source && index == 0 {
            crate::native::full_record::fit_source(world, area, entity);
        }
    }
    heights.retain(|_, bands| !bands.is_empty());
    for bands in heights.values_mut() {
        bands.clear();
    }
    grouped.retain(|_, group| !group.rows.is_empty());
    for group in grouped.values_mut() {
        group.rows.clear();
        group.placed = false;
    }
    offsets.retain(|_, bands| !bands.is_empty());
    for bands in offsets.values_mut() {
        bands.clear();
    }
    if !had_rows {
        prepared.shrink_to_fit();
    }
    if heights.is_empty() {
        heights.shrink_to_fit();
    }
    if grouped.is_empty() {
        grouped.shrink_to_fit();
    }
    if offsets.is_empty() {
        offsets.shrink_to_fit();
    }
    grouping.finish_frame();
}
