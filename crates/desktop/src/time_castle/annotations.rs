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
}

#[derive(Component)]
struct Layout(Vec<model::Label>);

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
    let font = palette.font * 0.8125;
    let labels = if changed || world.get::<Layout>(owner).is_none() {
        let labels = model::labels(
            &settings,
            &world.get::<View>(owner).unwrap().entries,
            now,
            size.to_array(),
            font,
            palette.gap,
        );
        world.entity_mut(owner).insert(Layout(labels.clone()));
        labels
    } else {
        world.get::<Layout>(owner).unwrap().0.clone()
    };
    let mut existing: HashMap<_, _> = previous
        .iter()
        .filter_map(|entity| {
            world
                .get::<Annotation>(*entity)
                .map(|annotation| (annotation.id.clone(), *entity))
        })
        .collect();
    for label in &labels {
        let entry = world.get::<View>(owner).unwrap().entries[label.occurrence.index].clone();
        let entity = existing.remove(&entry.id).unwrap_or_else(|| {
            let entity = world
                .spawn((
                    AnnotationOf(owner),
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
                    ActionButton::new(owner, crate::actions![ui::Select(vec![entry.id.clone()])]),
                ))
                .id();
            let title = crate::edit_mode::label(world, entity, "", font);
            let time = crate::edit_mode::label(world, entity, "", font * 0.9);
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
                id: entry.id.clone(),
                title,
                time,
                bar,
            });
            entity
        });
        let offset = bevy::math::DQuat::from_array(spatial.rotation)
            * bevy::math::DVec3::new(
                f64::from(label.rect[0] + label.rect[2] * 0.5),
                0.0,
                f64::from(label.rect[1] + label.rect[3] * 0.5),
            );
        let position = item.position + bevy::math::DVec2::new(offset.x, offset.z);
        let mut placement = spatial;
        placement.elevation += offset.y;
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
            .contains(&entry.id);
        let color = palette.event(label.occurrence.lane, selected);
        world
            .get_mut::<TextColor>(title)
            .unwrap()
            .set_if_neq(TextColor(palette.ink));
        world
            .get_mut::<TextColor>(time)
            .unwrap()
            .set_if_neq(TextColor(palette.muted));
        for (entity, size) in [(title, font), (time, font * 0.9)] {
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
    }
    for entity in existing.into_values() {
        world.despawn(entity);
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            entities
                .iter()
                .all(|entity| world.get::<Node>(*entity).unwrap().overflow == Overflow::visible())
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
