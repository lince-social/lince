// non-lean-yet
// Scope: enum Mark, struct Drawing, fn retire, fn apply.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   175.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (160, 170).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 157.

use super::{InspectionOverlay, Typography};
use crate::native::{token_style, tokens::Token};
use bevy::prelude::*;

#[derive(PartialEq)]
pub(super) enum Mark {
    Contour(Rect, BorderRadius, bool),
    Line(Vec2, Vec2),
    Label(Vec2, String, bool),
}

impl Mark {
    fn compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Contour(_, _, a), Self::Contour(_, _, b))
            | (Self::Label(_, _, a), Self::Label(_, _, b)) => a == b,
            (Self::Line(..), Self::Line(..)) => true,
            _ => false,
        }
    }

    fn geometry(&self) -> (Node, UiTransform) {
        let mut node = Node {
            position_type: PositionType::Absolute,
            ..default()
        };
        let mut transform = UiTransform::default();
        match self {
            Self::Contour(rect, radius, _) => {
                node.left = px(rect.min.x);
                node.top = px(rect.min.y);
                node.width = px(rect.width());
                node.height = px(rect.height());
                node.border = UiRect::all(px(1));
                node.border_radius = *radius;
            }
            Self::Line(start, end) => {
                let delta = *end - *start;
                let length = delta.length();
                let center = (*start + *end) * 0.5;
                node.left = px(center.x - length * 0.5);
                node.top = px(center.y - 0.5);
                node.width = px(length);
                node.height = px(1);
                transform.rotation = Rot2::radians(delta.y.atan2(delta.x));
            }
            Self::Label(position, _, _) => {
                node.left = px(position.x.max(0.0));
                node.top = px(position.y.max(0.0));
                node.max_width = px(230);
                node.padding = UiRect::all(px(3));
            }
        }
        (node, transform)
    }

    fn spawn(&self, world: &mut World) -> Entity {
        let (node, transform) = self.geometry();
        let entity = world
            .spawn((
                InspectionOverlay,
                node,
                transform,
                Pickable::IGNORE,
                GlobalZIndex(90),
                token_style::border(Token::Connections),
            ))
            .id();
        match self {
            Self::Line(..) => {
                world
                    .entity_mut(entity)
                    .insert(token_style::background(Token::Connections));
            }
            Self::Contour(_, _, true) => {
                world
                    .entity_mut(entity)
                    .insert(token_style::background(Token::ConnectionFill));
            }
            Self::Label(_, value, ghost) => {
                let font = world
                    .resource::<Typography>()
                    .text(if *ghost { 14.0 } else { 11.0 });
                world.entity_mut(entity).insert((
                    Text::new(value),
                    font,
                    token_style::text(if *ghost {
                        Token::Connections
                    } else {
                        Token::Ink
                    }),
                    token_style::background(Token::Surface),
                ));
            }
            _ => {}
        }
        engine::performance::add("inspection-overlays-created", 1);
        entity
    }

    fn update(&self, world: &mut World, entity: Entity) -> bool {
        let mut changed = false;
        let (node, transform) = self.geometry();
        if world.get::<Node>(entity) != Some(&node) {
            world.entity_mut(entity).insert(node);
            changed = true;
        }
        if world.get::<UiTransform>(entity) != Some(&transform) {
            world.entity_mut(entity).insert(transform);
            changed = true;
        }
        if let Self::Label(_, value, _) = self
            && world
                .get::<Text>(entity)
                .is_none_or(|text| text.0 != *value)
        {
            if let Some(mut text) = world.get_mut::<Text>(entity) {
                text.0.clone_from(value);
            } else {
                world.entity_mut(entity).insert(Text::new(value));
            }
            engine::performance::add("inspection-labels-updated", 1);
            changed = true;
        }
        if changed {
            engine::performance::add("inspection-overlays-updated", 1);
        }
        changed
    }
}

#[derive(Resource, Default)]
pub(super) struct Drawing {
    pub(super) marks: Vec<Mark>,
    pub(super) entities: Vec<Entity>,
}

fn retire(world: &mut World, entity: Entity) {
    if let Ok(entity) = world.get_entity_mut(entity) {
        entity.despawn();
        engine::performance::add("inspection-overlays-retired", 1);
    }
}

pub(super) fn apply(world: &mut World, next: &mut Vec<Mark>) {
    let _application = engine::performance::Span::new("inspection-overlay-application");
    let current = world.resource::<Drawing>();
    if current.marks == *next
        && current
            .entities
            .iter()
            .all(|entity| world.get_entity(*entity).is_ok())
    {
        next.clear();
        return;
    }
    world.resource_scope(|world, mut drawing: Mut<Drawing>| {
        let mut changed = false;
        while drawing.entities.len() > next.len() {
            retire(world, drawing.entities.pop().unwrap());
            changed = true;
        }
        for (index, mark) in next.iter().enumerate() {
            let previous = drawing.marks.get(index);
            let entity = drawing.entities.get(index).copied();
            let reusable = entity.is_some_and(|entity| world.get_entity(entity).is_ok())
                && previous.is_some_and(|previous| previous.compatible(mark));
            if reusable {
                if previous != Some(mark) {
                    changed |= mark.update(world, entity.unwrap());
                }
            } else {
                if let Some(entity) = entity {
                    retire(world, entity);
                    drawing.entities[index] = mark.spawn(world);
                } else {
                    drawing.entities.push(mark.spawn(world));
                }
                changed = true;
            }
        }
        std::mem::swap(&mut drawing.marks, next);
        next.clear();
        if changed && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.ring();
        }
    });
}
