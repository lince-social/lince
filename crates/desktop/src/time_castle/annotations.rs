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
        let labels = model::labels_with_metrics(
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
    let mut neighbors = Neighbors::default();
    for entity in &previous {
        if let Some(annotation) = world.get::<Annotation>(*entity) {
            neighbors.insert(*entity, annotation.label.rect);
        }
    }
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
            let mut target = if band.retiring {
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
            let held = crate::canvas_pan::dragged(world) == Some(entity)
                || world
                    .get_resource::<crate::topology::input::PointerState>()
                    .is_some_and(|pointer| pointer.drag.is_some_and(|(card, _)| card == entity));
            let held_position = held.then(|| {
                let card = world.get::<CanvasItem>(entity).unwrap();
                let elevation = world
                    .get::<crate::topology::Spatial>(entity)
                    .map_or(0.0, |card| card.elevation - spatial.elevation);
                let offset = bevy::math::DQuat::from_array(spatial.rotation).inverse()
                    * bevy::math::DVec3::new(
                        card.position.x - item.position.x,
                        elevation,
                        card.position.y - item.position.y,
                    );
                [
                    offset.x as f32 - label.rect[2] * 0.5,
                    offset.z as f32 - label.rect[3] * 0.5,
                ]
            });
            if settings.card_physics && settings.floating_cards && !band.retiring {
                let annotation = world.get::<Annotation>(entity).unwrap();
                let rect = [
                    annotation.spring.position[0],
                    annotation.spring.position[1],
                    label.rect[2],
                    label.rect[3],
                ];
                let rest = rest_position(
                    rect,
                    Vec2::new(label.anchor[0], label.anchor[2]),
                    size.min_element() * 0.4,
                );
                let push = neighbors.push(entity, rect, palette.gap);
                target = repel_clock(
                    rest + push,
                    Vec2::new(label.rect[2], label.rect[3]) * 0.5,
                    size.min_element() * 0.4,
                )
                .to_array();
            }
            let mut annotation = world.get_mut::<Annotation>(entity).unwrap();
            if let Some(position) = held_position {
                annotation.spring = lince_interface::motion::Spring::new(position);
                active = true;
            } else if settings.card_physics && settings.floating_cards {
                annotation.spring.position[0] += displacement.x as f32;
                annotation.spring.position[1] += displacement.z as f32;
                active |= advance_card(&mut annotation.spring, target, seconds);
            } else {
                annotation.spring = lince_interface::motion::Spring::new(target);
            }
            label.rect[0] = annotation.spring.position[0];
            label.rect[1] = annotation.spring.position[1];
        }
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

#[derive(Default)]
struct Neighbors(HashMap<(i32, i32), Vec<(Entity, [f32; 4])>>);

fn rest_position(rect: [f32; 4], anchor: Vec2, radius: f32) -> Vec2 {
    let half = Vec2::new(rect[2], rect[3]) * 0.5;
    let center = Vec2::new(rect[0], rect[1]) + half;
    let direction = (center - anchor)
        .try_normalize()
        .unwrap_or_else(|| anchor.try_normalize().unwrap_or(Vec2::Y));
    let target = anchor + direction * (half.dot(direction.abs()) + 30.0);
    repel_clock(target - half, half, radius)
}

fn repel_clock(position: Vec2, half: Vec2, radius: f32) -> Vec2 {
    let mut target = position + half;
    for _ in 0..8 {
        let nearest = Vec2::ZERO.clamp(target - half, target + half).length();
        if nearest >= radius + 18.0 {
            break;
        }
        target += target.try_normalize().unwrap_or(Vec2::Y) * (radius + 18.0 - nearest + 0.01);
    }
    target - half
}

fn advance_card(
    spring: &mut lince_interface::motion::Spring<2>,
    target: [f32; 2],
    seconds: f32,
) -> bool {
    if spring
        .position
        .iter()
        .zip(target)
        .all(|(position, target)| (position - target).abs() <= 0.1)
        && spring
            .velocity
            .iter()
            .all(|velocity| velocity.abs() <= 0.25)
    {
        spring.velocity = [0.0; 2];
        return false;
    }
    spring.advance_with_frequency(target, seconds, 6.0)
}

impl Neighbors {
    fn cells(rect: [f32; 4], gap: f32) -> Vec<(i32, i32)> {
        let min = Vec2::new(rect[0] - gap, rect[1] - gap) / 256.0;
        let max = Vec2::new(rect[0] + rect[2] + gap, rect[1] + rect[3] + gap) / 256.0;
        (min.x.floor() as i32..=max.x.floor() as i32)
            .flat_map(|x| (min.y.floor() as i32..=max.y.floor() as i32).map(move |y| (x, y)))
            .collect()
    }

    fn insert(&mut self, entity: Entity, rect: [f32; 4]) {
        for cell in Self::cells(rect, 0.0) {
            self.0.entry(cell).or_default().push((entity, rect));
        }
    }

    fn push(&self, entity: Entity, rect: [f32; 4], gap: f32) -> Vec2 {
        let center = Vec2::new(rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
        let mut seen = std::collections::HashSet::new();
        let mut push = Vec2::ZERO;
        for cell in Self::cells(rect, gap) {
            for (other, bounds) in self.0.get(&cell).into_iter().flatten() {
                if *other == entity || !seen.insert(*other) {
                    continue;
                }
                let other_center =
                    Vec2::new(bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5);
                let delta = center - other_center;
                let overlap = Vec2::new(
                    (rect[2] + bounds[2]) * 0.5 + gap,
                    (rect[3] + bounds[3]) * 0.5 + gap,
                ) - delta.abs();
                if overlap.min_element() <= 0.0 {
                    continue;
                }
                let direction = delta.try_normalize().unwrap_or_else(|| {
                    let angle =
                        ((entity.to_bits().min(other.to_bits()) % 1024) as f32 + 0.5) * 2.399_963_1;
                    Vec2::new(angle.cos(), angle.sin())
                        * if entity.to_bits() < other.to_bits() {
                            1.0
                        } else {
                            -1.0
                        }
                });
                let distance = (overlap.x / direction.x.abs().max(0.001))
                    .min(overlap.y / direction.y.abs().max(0.001));
                push += direction * distance * 0.65;
            }
        }
        push.clamp_length_max(200.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_cards_repel_in_two_dimensions_without_camera_constraints() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let mut neighbors = Neighbors::default();
        neighbors.insert(a, [1000.0, -1200.0, 200.0, 48.0]);
        neighbors.insert(b, [1000.0, -1200.0, 200.0, 48.0]);
        let push = neighbors.push(a, [1000.0, -1200.0, 200.0, 48.0], 8.0);
        assert!(push.x.abs() > 0.1 && push.y.abs() > 0.1);
        assert!((push + neighbors.push(b, [1000.0, -1200.0, 200.0, 48.0], 8.0)).length() < 0.001);
        assert_eq!(
            neighbors.push(a, [1600.0, -1200.0, 200.0, 48.0], 8.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn cards_are_attracted_to_their_anchor_and_repelled_from_the_clock() {
        let anchor = Vec2::new(174.0, 0.0);
        let far = rest_position([1000.0, -24.0, 200.0, 48.0], anchor, 168.0);
        assert!((far.x - anchor.x - 30.0).abs() < 0.01);
        let close = rest_position([-100.0, -24.0, 200.0, 48.0], Vec2::new(0.0, -174.0), 168.0);
        assert!(
            Vec2::ZERO
                .clamp(close, close + Vec2::new(200.0, 48.0))
                .length()
                >= 186.0
        );
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
        let now = 1_800_000_000_000;
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
        for _ in 0..1200 {
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
            if !world.get::<motion::Motion>(owner).unwrap().active {
                break;
            }
        }
        assert!(!world.get::<motion::Motion>(owner).unwrap().active);
        let card = entities[0];
        let original = world.get::<CanvasItem>(card).unwrap().position;
        let dragged = original + bevy::math::DVec2::new(80.0, -90.0);
        let clock_position = world.get::<CanvasItem>(owner).unwrap().position;
        world.get_mut::<CanvasItem>(card).unwrap().position = dragged;
        world.init_resource::<crate::topology::input::PointerState>();
        world
            .resource_mut::<crate::topology::input::PointerState>()
            .drag = Some((card, bevy::math::DVec3::ZERO));
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        assert!((world.get::<CanvasItem>(card).unwrap().position - dragged).length() < 0.01);
        world
            .resource_mut::<crate::topology::input::PointerState>()
            .drag = None;
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        let released = world.get::<CanvasItem>(card).unwrap().position;
        assert!(released.distance(dragged) < 10.0);
        assert!(released.distance(original) > 60.0);
        assert_eq!(
            world.get::<CanvasItem>(owner).unwrap().position,
            clock_position
        );
        let rotation = bevy::math::DQuat::from_rotation_x(0.4);
        world.entity_mut(owner).insert(crate::topology::Spatial {
            rotation: rotation.to_array(),
            ..default()
        });
        let offset = rotation * bevy::math::DVec3::new(320.0, 0.0, -150.0);
        let rotated_drag = clock_position + bevy::math::DVec2::new(offset.x, offset.z);
        world.get_mut::<CanvasItem>(card).unwrap().position = rotated_drag;
        world
            .get_mut::<crate::topology::Spatial>(card)
            .unwrap()
            .elevation = offset.y;
        world
            .resource_mut::<crate::topology::input::PointerState>()
            .drag = Some((card, bevy::math::DVec3::ZERO));
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            false,
            &palette,
        );
        assert!((world.get::<CanvasItem>(card).unwrap().position - rotated_drag).length() < 0.01);
        assert!(
            (world
                .get::<crate::topology::Spatial>(card)
                .unwrap()
                .elevation
                - offset.y)
                .abs()
                < 0.01
        );
        world
            .resource_mut::<crate::topology::input::PointerState>()
            .drag = None;
        world.get_mut::<TimeSettings>(owner).unwrap().0.card_physics = false;
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        let before_camera = world.get::<CanvasItem>(card).unwrap().position;
        world.entity_mut(root).insert(crate::canvas::CanvasView {
            center: bevy::math::DVec2::splat(20_000.0),
            zoom: 3.0,
            ..default()
        });
        update(
            &mut world,
            owner,
            now,
            Vec2::splat(420.0),
            true,
            true,
            &palette,
        );
        assert_eq!(
            world.get::<CanvasItem>(card).unwrap().position,
            before_camera
        );
        world.despawn(owner);
        assert!(
            entities
                .iter()
                .all(|entity| world.get_entity(*entity).is_err())
        );
        assert!(world.get_entity(root).is_ok());
    }
}
