use super::*;
use crate::actions::ActionButton;
use crate::canvas::CanvasItem;

#[derive(Component)]
#[relationship(relationship_target = Annotations)]
struct AnnotationOf(Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = AnnotationOf, linked_spawn)]
struct Annotations(Vec<Entity>);

#[derive(Component)]
pub(super) struct Annotation {
    id: String,
    title: Entity,
    time: Entity,
    bar: Entity,
    label: model::Label,
    spring: lince_interface::motion::Spring<2>,
}

#[derive(Component)]
struct Layout {
    labels: Vec<model::Label>,
    position: bevy::math::DVec2,
}

pub(super) fn update(
    world: &mut World,
    owner: Entity,
    now: i64,
    size: Vec2,
    visible: bool,
    changed: bool,
    palette: &palette::Palette,
) -> Vec<model::Label> {
    let previous: Vec<_> = world
        .get::<Annotations>(owner)
        .map(|annotations| annotations.0.clone())
        .unwrap_or_default();
    if let Some((hit, point)) = world
        .get_resource::<crate::topology::input::PointerState>()
        .and_then(|pointer| pointer.hit)
    {
        let id = if hit == owner {
            world
                .get::<crate::topology::presentation::Surface>(owner)
                .and_then(|surface| world.get::<GlobalTransform>(surface.visual))
                .and_then(|transform| {
                    render::nearest_at(
                        world,
                        owner,
                        transform.affine().inverse().transform_point3(point).xz(),
                    )
                })
        } else {
            world
                .get::<Annotation>(hit)
                .filter(|_| {
                    world
                        .get::<AnnotationOf>(hit)
                        .is_some_and(|parent| parent.0 == owner)
                })
                .map(|annotation| annotation.id.clone())
        };
        if let Some(id) = id {
            let local = (hit == owner)
                .then(|| {
                    world
                        .get::<crate::topology::presentation::Surface>(owner)
                        .and_then(|surface| world.get::<GlobalTransform>(surface.visual))
                        .map(|transform| transform.affine().inverse().transform_point3(point))
                })
                .flatten();
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.hovered = Some(id);
            if let Some(point) = local {
                view.hover_point = Some(point);
            }
            view.hover_at = std::time::Instant::now();
        }
    }
    if world
        .get::<View>(owner)
        .unwrap()
        .hover_at
        .elapsed()
        .as_millis()
        > 250
    {
        world.get_mut::<View>(owner).unwrap().hovered = None;
        world.get_mut::<View>(owner).unwrap().hover_point = None;
    }
    if !visible {
        for entity in previous {
            world.despawn(entity);
        }
        world.entity_mut(owner).remove::<Layout>();
        return Vec::new();
    }
    let Some(item) = world.get::<CanvasItem>(owner).cloned() else {
        return Vec::new();
    };
    let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
        return Vec::new();
    };
    let Some(member) = world
        .get::<crate::workspace::WorkspaceMember>(owner)
        .copied()
    else {
        return Vec::new();
    };
    let spatial = world
        .get::<crate::topology::Spatial>(owner)
        .cloned()
        .unwrap_or_default();
    let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
    let unwind = world.get::<View>(owner).unwrap().unwind;
    let spatial_mode = world
        .get::<crate::topology::view::View>(root)
        .is_some_and(|view| view.spatial);
    let hovered = world.get::<View>(owner).unwrap().hovered.clone();
    let displacement = world
        .get::<Layout>(owner)
        .map_or(bevy::math::DVec2::ZERO, |layout| {
            layout.position - item.position
        });
    let displacement = bevy::math::DQuat::from_array(spatial.rotation).inverse()
        * bevy::math::DVec3::new(displacement.x, 0.0, displacement.y);
    let font = palette.font * 0.875;
    let bounds = visible_bounds(world, root, &item, &spatial);
    let mut labels = if changed || world.get::<Layout>(owner).is_none() {
        let face = world
            .get_resource::<crate::theme::Typography>()
            .and_then(|typography| {
                world
                    .get_resource::<Assets<Font>>()
                    .and_then(|fonts| fonts.get(&typography.0))
            })
            .and_then(|font| ttf_parser::Face::parse(font.data.as_ref(), 0).ok());
        let advance = |ch: char| {
            face.as_ref()
                .and_then(|face| {
                    face.glyph_index(ch)
                        .and_then(|glyph| face.glyph_hor_advance(glyph))
                })
                .map_or(
                    font * if ch as u32 >= 0x1100 { 1.0 } else { 0.62 },
                    |width| {
                        f32::from(width) / f32::from(face.as_ref().unwrap().units_per_em()) * font
                    },
                )
        };
        let mut labels = model::labels_with_metrics(
            &settings,
            &world.get::<View>(owner).unwrap().entries,
            now,
            size.to_array(),
            font,
            palette.gap,
            &model::LabelMetrics {
                band_width: palette.width,
                advance: &advance,
            },
        );
        let radius = label_floor(&labels, size, palette.width);
        let mut previous = Vec::new();
        for label in &mut labels {
            if let Some(bounds) = bounds {
                fit(&mut label.rect, &previous, radius, palette.gap, bounds);
            }
            separate(&mut label.rect, &previous, radius, palette.gap);
            previous.push(label.rect);
        }
        world.entity_mut(owner).insert(Layout {
            labels: labels.clone(),
            position: item.position,
        });
        labels
    } else {
        world.get::<Layout>(owner).unwrap().labels.clone()
    };
    world.get_mut::<Layout>(owner).unwrap().position = item.position;
    if !settings.floating_cards {
        labels.retain(|label| hovered.as_ref() == Some(&label.id));
    }
    let mut existing: HashMap<_, _> = previous
        .iter()
        .filter_map(|entity| {
            world
                .get::<Annotation>(*entity)
                .map(|annotation| (annotation.id.clone(), *entity))
        })
        .collect();
    let live: std::collections::HashSet<_> = labels.iter().map(|label| label.id.clone()).collect();
    for entity in &previous {
        let annotation = world.get::<Annotation>(*entity).unwrap();
        if settings.floating_cards
            && !live.contains(&annotation.id)
            && world.get::<motion::Motion>(owner).is_some_and(|motion| {
                motion
                    .bands
                    .get(&annotation.id)
                    .is_some_and(|band| band.retiring)
            })
        {
            labels.push(annotation.label.clone());
        }
    }
    let seconds = world
        .get::<motion::Motion>(owner)
        .map_or(0.0, |motion| motion.seconds);
    let mut rectangles = Vec::new();
    let radius = label_floor(&labels, size, palette.width);
    let mut active = false;
    for label in &mut labels {
        let band = world
            .get::<motion::Motion>(owner)
            .and_then(|motion| motion.bands.get(&label.id))
            .cloned();
        let opacity = band.as_ref().map_or(1.0, motion::Band::opacity);
        if let Some(band) = &band {
            label.anchor = band.anchor(&settings, now, size, palette.width, unwind);
            if !spatial_mode {
                label.anchor[1] = 0.0;
            }
        }
        if !settings.floating_cards {
            if let Some(point) = world.get::<View>(owner).unwrap().hover_point {
                label.anchor = point.to_array();
            }
            label.rect[0] = label.anchor[0] + 16.0;
            label.rect[1] = label.anchor[2] + 16.0;
        }
        let entity = existing.remove(&label.id).unwrap_or_else(|| {
            let entity = world
                .spawn((
                    AnnotationOf(owner),
                    AttachedCard,
                    ChildOf(root),
                    member,
                    spatial.clone(),
                    crate::inspection::InspectionExcluded,
                    CanvasItem {
                        position: item.position,
                        size: Vec2::ONE,
                    },
                    crate::sand::button(0),
                    crate::sand::Borderless,
                    bevy::a11y::AccessibilityNode::default(),
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(7)),
                        row_gap: px(3),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(palette.face),
                    crate::tokens::TokenOverrides(std::collections::BTreeMap::from([
                        (
                            crate::tokens::Token::FontSize,
                            crate::tokens::TokenValue::Number(16.0),
                        ),
                        (
                            crate::tokens::Token::Padding,
                            crate::tokens::TokenValue::Number(8.0),
                        ),
                        (
                            crate::tokens::Token::Spacing,
                            crate::tokens::TokenValue::Number(8.0),
                        ),
                    ])),
                    ActionButton::new(owner, crate::actions![ui::Select(vec![label.id.clone()])]),
                ))
                .id();
            let title = crate::edit_mode::label(world, entity, "", font);
            let time = crate::edit_mode::label(world, entity, "", font);
            for text in [title, time] {
                world
                    .entity_mut(text)
                    .remove::<crate::token_style::TextToken>();
                world.entity_mut(text).insert(TextLayout::no_wrap());
            }
            let bar = world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(6),
                        bottom: px(6),
                        width: px(2),
                        border_radius: BorderRadius::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(palette.event),
                    ChildOf(entity),
                ))
                .id();
            world.entity_mut(entity).insert(Annotation {
                id: label.id.clone(),
                title,
                time,
                bar,
                label: label.clone(),
                spring: lince_interface::motion::Spring::new(if band.is_some() {
                    let direction = Vec2::new(
                        label.rect[0] + label.rect[2] * 0.5,
                        label.rect[1] + label.rect[3] * 0.5,
                    )
                    .normalize_or_zero();
                    [
                        label.rect[0] + direction.x * 32.0,
                        label.rect[1] + direction.y * 32.0,
                    ]
                } else {
                    [label.rect[0], label.rect[1]]
                }),
            });
            entity
        });
        if let Some(band) = &band {
            let direction = Vec2::new(
                label.rect[0] + label.rect[2] * 0.5,
                label.rect[1] + label.rect[3] * 0.5,
            )
            .normalize_or_zero();
            let target = if band.retiring {
                let position = world.get::<Annotation>(entity).unwrap().spring.position;
                world.entity_mut(entity).remove::<ActionButton>();
                [
                    position[0] + direction.x * 4.0,
                    position[1] + direction.y * 4.0,
                ]
            } else {
                if world.get::<ActionButton>(entity).is_none() {
                    world.entity_mut(entity).insert(ActionButton::new(
                        owner,
                        crate::actions![ui::Select(vec![label.id.clone()])],
                    ));
                }
                [label.rect[0], label.rect[1]]
            };
            let mut annotation = world.get_mut::<Annotation>(entity).unwrap();
            if settings.card_physics && settings.floating_cards {
                annotation.spring.position[0] += displacement.x as f32;
                annotation.spring.position[1] += displacement.z as f32;
                active |= annotation
                    .spring
                    .advance_with_frequency(target, seconds, 6.0);
            } else {
                annotation.spring = lince_interface::motion::Spring::new(target);
            }
            label.rect[0] = annotation.spring.position[0];
            label.rect[1] = annotation.spring.position[1];
        }
        if settings.floating_cards
            && let Some(bounds) = bounds
        {
            fit(&mut label.rect, &rectangles, radius, palette.gap, bounds);
        }
        if settings.floating_cards {
            separate(&mut label.rect, &rectangles, radius, palette.gap);
        }
        rectangles.push(label.rect);
        {
            let mut annotation = world.get_mut::<Annotation>(entity).unwrap();
            annotation.spring.position = [label.rect[0], label.rect[1]];
            annotation.label = label.clone();
        }
        let offset = bevy::math::DQuat::from_array(spatial.rotation)
            * bevy::math::DVec3::new(
                f64::from(label.rect[0] + label.rect[2] * 0.5),
                0.0,
                f64::from(label.rect[1] + label.rect[3] * 0.5),
            );
        let position = item.position + bevy::math::DVec2::new(offset.x, offset.z);
        let mut placement = spatial;
        placement.elevation += offset.y;
        if !settings.floating_cards {
            placement.elevation += f64::from(label.anchor[1]);
        }
        let size = Vec2::new(label.rect[2], label.rect[3]);
        if world
            .get::<CanvasItem>(entity)
            .is_none_or(|item| item.position != position || item.size != size)
        {
            world
                .entity_mut(entity)
                .insert(CanvasItem { position, size });
        }
        world
            .get_mut::<crate::topology::Spatial>(entity)
            .unwrap()
            .set_if_neq(placement);
        if world
            .get::<crate::workspace::WorkspaceMember>(entity)
            .is_none_or(|previous| previous.0 != member.0)
        {
            world.entity_mut(entity).insert(member);
        }
        let (title, time, bar) = {
            let annotation = world.get::<Annotation>(entity).unwrap();
            (annotation.title, annotation.time, annotation.bar)
        };
        let timing = &label.time;
        if world.get::<Text>(title).unwrap().0 != label.title
            || world.get::<Text>(time).unwrap().0 != *timing
        {
            world
                .get_mut::<Text>(title)
                .unwrap()
                .set_if_neq(Text::new(&label.title));
            world
                .get_mut::<Text>(time)
                .unwrap()
                .set_if_neq(Text::new(timing));
            world
                .get_mut::<bevy::a11y::AccessibilityNode>(entity)
                .unwrap()
                .set_label(format!(
                    "{} {}",
                    label.title.replace('\n', " "),
                    timing.replace('\n', " ")
                ));
        }
        let selected = world
            .get::<View>(owner)
            .unwrap()
            .selected
            .contains(&label.id);
        let color = palette.event(label.occurrence.lane, selected);
        world
            .get_mut::<TextColor>(title)
            .unwrap()
            .set_if_neq(TextColor(palette.ink));
        world
            .get_mut::<TextColor>(time)
            .unwrap()
            .set_if_neq(TextColor(palette.muted));
        for (entity, size) in [(title, font), (time, font)] {
            let mut text = world.get_mut::<TextFont>(entity).unwrap();
            if text.font_size != FontSize::Px(size) {
                text.font_size = FontSize::Px(size);
            }
        }
        world
            .get_mut::<BackgroundColor>(entity)
            .unwrap()
            .set_if_neq(BackgroundColor(palette.face));
        world
            .get_mut::<BackgroundColor>(bar)
            .unwrap()
            .set_if_neq(BackgroundColor(color));
        world
            .entity_mut(entity)
            .insert(crate::topology::presentation::SurfaceOpacity(opacity));
        if let Some(material) = world
            .get::<crate::topology::presentation::Surface>(entity)
            .map(|surface| surface.material.clone())
            && let Some(mut materials) = world.get_resource_mut::<Assets<StandardMaterial>>()
            && let Some(mut material) = materials.get_mut(&material)
            && material.base_color.alpha() != opacity
        {
            material.base_color = Color::linear_rgba(opacity, opacity, opacity, opacity);
        }
    }
    for entity in existing.into_values() {
        world.despawn(entity);
    }
    if let Some(mut motion) = world.get_mut::<motion::Motion>(owner) {
        motion.active |= active;
    }
    labels
}

fn separate(rect: &mut [f32; 4], previous: &[[f32; 4]], radius: f32, gap: f32) {
    let center = Vec2::new(rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
    let direction = center.try_normalize().unwrap_or(Vec2::Y);
    let nearest = Vec2::new(
        0.0_f32.clamp(rect[0], rect[0] + rect[2]),
        0.0_f32.clamp(rect[1], rect[1] + rect[3]),
    )
    .length();
    if nearest < radius {
        let half = Vec2::new(rect[2], rect[3]) * 0.5;
        let target = direction * (radius + half.length() + gap.max(0.01)) - half;
        rect[0] = target.x;
        rect[1] = target.y;
    }
    for _ in 0..previous.len() {
        let Some(other) = previous.iter().find(|other| {
            rect[0] < other[0] + other[2] + gap
                && rect[0] + rect[2] + gap > other[0]
                && rect[1] < other[1] + other[3] + gap
                && rect[1] + rect[3] + gap > other[1]
        }) else {
            break;
        };
        let clearance = gap
            + (other[0].abs() + other[1].abs() + rect[2] + rect[3] + gap).max(1.0)
                * f32::EPSILON
                * 4.0;
        if direction.x.abs() > direction.y.abs() {
            rect[0] = if direction.x >= 0.0 {
                other[0] + other[2] + clearance
            } else {
                other[0] - rect[2] - clearance
            };
        } else {
            rect[1] = if direction.y >= 0.0 {
                other[1] + other[3] + clearance
            } else {
                other[1] - rect[3] - clearance
            };
        }
    }
}

fn label_floor(labels: &[model::Label], size: Vec2, width: f32) -> f32 {
    size.min_element() * 0.4
        + 28.0
        + width * 0.5
        + labels
            .iter()
            .map(|label| label.occurrence.lane)
            .max()
            .unwrap_or_default() as f32
            * (width + 3.0)
}

fn visible_bounds(
    world: &mut World,
    root: Entity,
    item: &CanvasItem,
    spatial: &crate::topology::Spatial,
) -> Option<Rect> {
    if spatial.rotation != bevy::math::DQuat::IDENTITY.to_array() {
        return None;
    }
    let canvas = world.get::<crate::canvas::CanvasView>(root).copied()?;
    let size = world
        .query::<&Window>()
        .iter(world)
        .next()
        .map(|window| Vec2::new(window.width(), window.height()))?;
    let half = size / canvas.zoom as f32 * 0.5 - Vec2::splat(16.0);
    let center = (canvas.center - item.position).as_vec2();
    (half.min_element() > item.size.max_element() * 0.5
        && center.abs().max_element() < half.min_element())
    .then(|| Rect::from_corners(center - half, center + half))
}

fn fit(rect: &mut [f32; 4], previous: &[[f32; 4]], radius: f32, gap: f32, bounds: Rect) {
    let valid = |candidate: [f32; 4]| {
        let nearest = Vec2::new(
            0.0_f32.clamp(candidate[0], candidate[0] + candidate[2]),
            0.0_f32.clamp(candidate[1], candidate[1] + candidate[3]),
        )
        .length();
        candidate[0] >= bounds.min.x
            && candidate[1] >= bounds.min.y
            && candidate[0] + candidate[2] <= bounds.max.x
            && candidate[1] + candidate[3] <= bounds.max.y
            && nearest >= radius
            && previous.iter().all(|other| {
                candidate[0] >= other[0] + other[2] + gap
                    || candidate[0] + candidate[2] + gap <= other[0]
                    || candidate[1] >= other[1] + other[3] + gap
                    || candidate[1] + candidate[3] + gap <= other[1]
            })
    };
    if valid(*rect) || rect[2] > bounds.width() || rect[3] > bounds.height() {
        return;
    }
    let original = Vec2::new(rect[0], rect[1]);
    let mut xs = vec![
        rect[0].clamp(bounds.min.x, bounds.max.x - rect[2]),
        bounds.min.x,
        bounds.max.x - rect[2],
        -radius - rect[2] - gap,
        radius + gap,
    ];
    let mut ys = vec![
        rect[1].clamp(bounds.min.y, bounds.max.y - rect[3]),
        bounds.min.y,
        bounds.max.y - rect[3],
        -radius - rect[3] - gap,
        radius + gap,
    ];
    for other in previous.iter().rev().take(24) {
        xs.extend([other[0] - rect[2] - gap, other[0] + other[2] + gap]);
        ys.extend([other[1] - rect[3] - gap, other[1] + other[3] + gap]);
    }
    let mut best = None;
    let mut distance = f32::INFINITY;
    for x in xs {
        for y in &ys {
            let candidate = [x, *y, rect[2], rect[3]];
            let squared = Vec2::new(x, *y).distance_squared(original);
            if squared < distance && valid(candidate) {
                best = Some(candidate);
                distance = squared;
            }
        }
    }
    if let Some(best) = best {
        *rect = best;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_card_separation_terminates_at_the_center_and_fractional_edges() {
        let mut centered = [-100.0, -24.0, 200.0, 48.0];
        separate(&mut centered, &[], 210.0, 8.0);
        assert!(centered[1] > 210.0);
        for sign in [-1.0, 1.0] {
            let mut placed = Vec::new();
            for index in 0..100 {
                let mut rect = [sign * 220.1273, index as f32 * 0.1237, 200.0, 48.0];
                separate(&mut rect, &placed, 210.0, 8.0);
                assert!(placed.iter().all(|other: &[f32; 4]| {
                    rect[0] >= other[0] + other[2] + 8.0
                        || rect[0] + rect[2] + 8.0 <= other[0]
                        || rect[1] >= other[1] + other[3] + 8.0
                        || rect[1] + rect[3] + 8.0 <= other[1]
                }));
                placed.push(rect);
            }
        }
    }

    #[test]
    fn cards_fan_out_at_window_edges_without_covering_the_clock_or_each_other() {
        let bounds = Rect::from_corners(Vec2::new(-650.0, -360.0), Vec2::new(650.0, 360.0));
        let mut placed = Vec::new();
        for index in 0..20 {
            let mut rect = [-100.0, 200.0 + index as f32 * 56.0, 200.0, 48.0];
            fit(&mut rect, &placed, 210.0, 8.0, bounds);
            assert!(
                bounds.contains(Vec2::new(rect[0], rect[1]))
                    && bounds.contains(Vec2::new(rect[0] + rect[2], rect[1] + rect[3]))
            );
            let nearest = Vec2::new(
                0.0_f32.clamp(rect[0], rect[0] + rect[2]),
                0.0_f32.clamp(rect[1], rect[1] + rect[3]),
            )
            .length();
            assert!(nearest >= 210.0);
            assert!(
                placed
                    .iter()
                    .all(|other: &[f32; 4]| rect[0] >= other[0] + other[2] + 8.0
                        || rect[0] + rect[2] + 8.0 <= other[0]
                        || rect[1] >= other[1] + other[3] + 8.0
                        || rect[1] + rect[3] + 8.0 <= other[1])
            );
            placed.push(rect);
        }
    }

    #[test]
    fn annotations_follow_a_fixed_clock_reuse_entities_and_close_with_their_owner() {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let root = world.spawn_empty().id();
        let owner = world
            .spawn((
                Node::default(),
                ChildOf(root),
                crate::workspace::WorkspaceMember(1),
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(420.0),
                },
            ))
            .id();
        populate(&mut world, owner);
        let now = chrono::Utc::now().timestamp_millis();
        world.get_mut::<View>(owner).unwrap().entries = (0..4)
            .map(|index| Entry {
                id: format!("event-{index}"),
                record_uid: format!("r:{index}"),
                head: format!("Simultaneous task {index}"),
                quantity: "-1".into(),
                category: model::Category::Timed,
                time: Some(nucleus::schedule::TimeRange {
                    from_ms: now + 60_000,
                    until_ms: None,
                }),
                origin: serde_json::json!({"kind":"manual"}),
                preview: false,
                start_date: None,
                due_date: None,
            })
            .collect();
        let palette = palette::Palette::resolve(&world, owner);
        let labels = update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        assert_eq!(labels.len(), 4);
        let entities = world.get::<Annotations>(owner).unwrap().0.clone();
        let previous = world.get::<CanvasItem>(entities[0]).unwrap().position;
        world.get_mut::<CanvasItem>(owner).unwrap().position.x = 125.0;
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        assert_eq!(world.get::<Annotations>(owner).unwrap().0, entities);
        assert_eq!(
            world.get::<CanvasItem>(entities[0]).unwrap().position.x,
            previous.x + 125.0
        );
        assert_eq!(
            world.get::<CanvasItem>(owner).unwrap().size,
            Vec2::splat(420.0)
        );
        let action = world.get::<ActionButton>(entities[0]).unwrap().clone();
        action.actions.run(&mut world, owner);
        assert_eq!(world.get::<View>(owner).unwrap().selected.len(), 1);
        {
            let mut settings = world.get_mut::<TimeSettings>(owner).unwrap();
            settings.0.floating_cards = false;
            settings.0.card_physics = false;
        }
        let labels = update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        assert!(labels.is_empty());
        {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.hovered = Some("event-1".into());
            view.hover_point = Some(Vec3::new(20.0, 0.0, 30.0));
            view.hover_at = std::time::Instant::now();
        }
        let labels = update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].id, "event-1");
        assert_eq!([labels[0].rect[0], labels[0].rect[1]], [36.0, 46.0]);
        let hover_card = world.get::<Annotations>(owner).unwrap().0[0];
        assert!(world.get::<AttachedCard>(hover_card).is_some());
        assert_eq!(
            world.get::<Annotation>(hover_card).unwrap().spring.velocity,
            [0.0; 2]
        );
        world
            .get_mut::<TimeSettings>(owner)
            .unwrap()
            .0
            .floating_cards = true;
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        let entities = world.get::<Annotations>(owner).unwrap().0.clone();
        assert!(
            entities
                .iter()
                .all(|entity| world.get::<Node>(*entity).unwrap().overflow == Overflow::visible())
        );
        motion::update(&mut world, owner, now, 3_600_000, true);
        let entries = std::mem::take(&mut world.get_mut::<View>(owner).unwrap().entries);
        motion::update(&mut world, owner, now, 3_600_000, true);
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        assert!(
            entities
                .iter()
                .all(|entity| world.get::<ActionButton>(*entity).is_none())
        );
        world.get_mut::<View>(owner).unwrap().entries = entries;
        motion::update(&mut world, owner, now, 3_600_000, true);
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        assert_eq!(world.get::<Annotations>(owner).unwrap().0, entities);
        assert!(
            entities
                .iter()
                .all(|entity| world.get::<ActionButton>(*entity).is_some())
        );
        world.get_mut::<TimeSettings>(owner).unwrap().0.card_physics = true;
        world.get_mut::<CanvasItem>(owner).unwrap().position.x += 4.0;
        world.get_mut::<motion::Motion>(owner).unwrap().seconds = 1.0 / 60.0;
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        assert!(entities.iter().any(|entity| {
            world
                .get::<Annotation>(*entity)
                .unwrap()
                .spring
                .velocity
                .iter()
                .any(|velocity| velocity.abs() > 0.25)
        }));
        for _ in 0..240 {
            world.get_mut::<motion::Motion>(owner).unwrap().active = false;
            update(
                &mut world,
                owner,
                now,
                Vec2::splat(420.0),
                true,
                false,
                &palette,
            );
        }
        assert!(!world.get::<motion::Motion>(owner).unwrap().active);
        world.despawn(owner);
        assert!(
            entities
                .iter()
                .all(|entity| world.get_entity(*entity).is_err())
        );
        assert!(world.get_entity(root).is_ok());
    }
}
