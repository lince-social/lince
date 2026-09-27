use crate::{canvas::CanvasItem, topology::Spatial, workspace::WorkspaceMember};
use bevy::{
    math::{DQuat, DVec2, DVec3},
    prelude::*,
};

#[derive(Component, Clone, Debug)]
pub struct ArrowSand {
    pub from: Entity,
    pub to: Entity,
    pub label: String,
}

#[derive(Component)]
struct Parts {
    shaft: Entity,
    tips: [Entity; 2],
    label: Entity,
}

pub struct ArrowSandPlugin;

impl Plugin for ArrowSandPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update
                .after(crate::physics::SimulateWorkspaces)
                .before(crate::topology::presentation::synchronize),
        );
    }
}

pub fn spawn(world: &mut World, from: Entity, to: Entity, label: String) -> Option<Entity> {
    let root = world.get::<ChildOf>(from)?.parent();
    let workspace = *world.get::<WorkspaceMember>(from)?;
    if world.get::<ChildOf>(to)?.parent() != root || world.get::<WorkspaceMember>(to)? != &workspace
    {
        return None;
    }
    let entity = world
        .spawn((
            ArrowSand {
                from,
                to,
                label: label.clone(),
            },
            crate::sand::InBox(root),
            ChildOf(root),
            workspace,
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::new(1.0, 32.0),
            },
            Node::default(),
            Pickable::IGNORE,
            crate::icons::Tooltip(label.clone()),
        ))
        .id();
    let shaft = segment(world, entity);
    let tips = [segment(world, entity), segment(world, entity)];
    let text = crate::edit_mode::label(world, entity, &label, 12.0);
    world.entity_mut(text).insert((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: percent(10),
            max_width: percent(80),
            overflow: Overflow::clip(),
            ..default()
        },
        Pickable::IGNORE,
    ));
    world.entity_mut(entity).insert(Parts {
        shaft,
        tips,
        label: text,
    });
    Some(entity)
}

fn segment(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node::default(),
            crate::token_style::background(crate::tokens::Token::Accent),
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id()
}

fn line(world: &mut World, entity: Entity, from: Vec2, to: Vec2) {
    let delta = to - from;
    let center = (from + to) * 0.5;
    let node = Node {
        position_type: PositionType::Absolute,
        left: px(center.x - delta.length() * 0.5),
        top: px(center.y - 1.0),
        width: px(delta.length()),
        height: px(2),
        ..default()
    };
    let transform = UiTransform {
        rotation: Rot2::radians(delta.y.atan2(delta.x)),
        ..default()
    };
    world.get_mut::<Node>(entity).unwrap().set_if_neq(node);
    if world.get::<UiTransform>(entity) != Some(&transform) {
        world.entity_mut(entity).insert(transform);
    }
}

fn corners(world: &World, entity: Entity) -> Option<[DVec3; 4]> {
    let item = world.get::<CanvasItem>(entity)?;
    let spatial = crate::topology::spatial(world, entity);
    let scale = world
        .get::<crate::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| scale.0);
    let half = item.size.as_dvec2() * f64::from(scale) * 0.5;
    if !half.is_finite() || half.min_element() <= 0.0 || !item.position.is_finite() {
        return None;
    }
    Some(
        [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[x, y]| {
            spatial.position(item.position)
                + spatial.rotation() * DVec3::new(x * half.x, 0.0, y * half.y)
        }),
    )
}

fn closest_segments(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> (DVec3, DVec3) {
    let u = b - a;
    let v = d - c;
    let w = a - c;
    let aa = u.length_squared();
    let bb = u.dot(v);
    let cc = v.length_squared();
    let dd = u.dot(w);
    let ee = v.dot(w);
    let denominator = aa * cc - bb * bb;
    let mut s = if denominator > aa * cc * 1e-12 {
        ((bb * ee - cc * dd) / denominator).clamp(0.0, 1.0)
    } else {
        let start = ((c - a).dot(u) / aa).min((d - a).dot(u) / aa).max(0.0);
        let end = ((c - a).dot(u) / aa).max((d - a).dot(u) / aa).min(1.0);
        ((start + end) * 0.5).clamp(0.0, 1.0)
    };
    let mut t = (bb * s + ee) / cc;
    if t < 0.0 {
        t = 0.0;
        s = (-dd / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((bb - dd) / aa).clamp(0.0, 1.0);
    }
    (a + u * s, c + v * t)
}

pub(crate) fn endpoints(world: &World, from: Entity, to: Entity) -> Option<(DVec3, DVec3)> {
    if from == to {
        return None;
    }
    let first = corners(world, from)?;
    let second = corners(world, to)?;
    let contains = |corners: &[DVec3; 4], point: DVec3| {
        let u = corners[1] - corners[0];
        let v = corners[3] - corners[0];
        let offset = point - corners[0];
        offset.dot(u.cross(v).normalize()).abs() < 1e-8
            && (0.0..=u.length_squared()).contains(&offset.dot(u))
            && (0.0..=v.length_squared()).contains(&offset.dot(v))
    };
    if first.iter().any(|point| contains(&second, *point))
        || second.iter().any(|point| contains(&first, *point))
    {
        return None;
    }
    let mut nearest = None;
    let mut distance = f64::INFINITY;
    let mut centrality = f64::INFINITY;
    let first_center = (first[0] + first[2]) * 0.5;
    let second_center = (second[0] + second[2]) * 0.5;
    for a in 0..4 {
        for b in 0..4 {
            let pair =
                closest_segments(first[a], first[(a + 1) % 4], second[b], second[(b + 1) % 4]);
            let next = pair.0.distance_squared(pair.1);
            let centered =
                pair.0.distance_squared(first_center) + pair.1.distance_squared(second_center);
            if next < distance - 1e-8 || ((next - distance).abs() <= 1e-8 && centered < centrality)
            {
                distance = next;
                centrality = centered;
                nearest = Some(pair);
            }
        }
    }
    nearest.filter(|(a, b)| a.is_finite() && b.is_finite() && a.distance_squared(*b) > 1e-8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(world: &mut World, root: Entity, position: DVec2, size: Vec2) -> Entity {
        world
            .spawn((
                CanvasItem { position, size },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id()
    }

    #[test]
    fn arrow_contacts_follow_nearest_perimeters_after_moves_resizes_and_rotation() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        let root = world.spawn_empty().id();
        let first = record(&mut world, root, DVec2::ZERO, Vec2::new(200.0, 100.0));
        let second = record(
            &mut world,
            root,
            DVec2::new(500.0, 0.0),
            Vec2::new(200.0, 100.0),
        );
        assert_eq!(
            endpoints(&world, first, second),
            Some((DVec3::new(100.0, 0.0, 0.0), DVec3::new(400.0, 0.0, 0.0)))
        );
        let arrow = spawn(&mut world, first, second, "depends-on".into()).unwrap();
        update(&mut world);
        assert!(
            world
                .get::<crate::canvas_selection::SandGroup>(arrow)
                .is_none()
        );
        assert!(
            world
                .get::<crate::canvas_selection::SandGroup>(first)
                .is_none()
        );
        world.get_mut::<CanvasItem>(second).unwrap().position = DVec2::new(0.0, 500.0);
        world.get_mut::<CanvasItem>(first).unwrap().size.y = 200.0;
        assert_eq!(
            endpoints(&world, first, second),
            Some((DVec3::new(0.0, 0.0, 100.0), DVec3::new(0.0, 0.0, 450.0)))
        );
        update(&mut world);
        let spatial = world.get::<Spatial>(arrow).unwrap();
        assert!((spatial.rotation() * DVec3::X - DVec3::Z).length() < 1e-8);
        world.entity_mut(second).insert(Spatial {
            rotation: DQuat::from_rotation_y(std::f64::consts::FRAC_PI_4).to_array(),
            elevation: 80.0,
            ..default()
        });
        let (a, b) = endpoints(&world, first, second).unwrap();
        assert!(a.is_finite() && b.is_finite());
        let local = crate::topology::spatial(&world, second)
            .local_point(world.get::<CanvasItem>(second).unwrap().position, b);
        assert!(local.y.abs() < 1e-8);
        assert!((local.x.abs() - 100.0).abs() < 1e-8 || (local.z.abs() - 50.0).abs() < 1e-8);
        world.despawn(first);
        update(&mut world);
        assert!(world.get_entity(arrow).is_err());
    }

    #[test]
    fn attachments_reject_other_workspaces_and_invalid_bounds() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let first = record(&mut world, root, DVec2::ZERO, Vec2::splat(100.0));
        let second = record(&mut world, root, DVec2::splat(200.0), Vec2::splat(100.0));
        world.entity_mut(second).insert(WorkspaceMember(2));
        assert!(spawn(&mut world, first, second, "link".into()).is_none());
        world.get_mut::<CanvasItem>(second).unwrap().position = DVec2::ZERO;
        world.get_mut::<CanvasItem>(second).unwrap().size = Vec2::splat(40.0);
        assert!(endpoints(&world, first, second).is_none());
        world.get_mut::<CanvasItem>(first).unwrap().size.x = f32::NAN;
        assert!(endpoints(&world, first, second).is_none());
        assert!(endpoints(&world, second, second).is_none());
    }
}

pub(crate) fn update(world: &mut World) {
    let arrows: Vec<_> = world
        .query::<(Entity, &ArrowSand)>()
        .iter(world)
        .map(|(entity, arrow)| (entity, arrow.clone()))
        .collect();
    for (entity, arrow) in arrows {
        let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            continue;
        };
        let workspace = world.get::<WorkspaceMember>(entity).copied();
        if [arrow.from, arrow.to].iter().any(|end| {
            world.get::<ChildOf>(*end).map(ChildOf::parent) != Some(parent)
                || world.get::<WorkspaceMember>(*end).copied() != workspace
        }) {
            world.despawn(entity);
            continue;
        }
        let Some((from, to)) = endpoints(world, arrow.from, arrow.to) else {
            if world.get::<Node>(entity).unwrap().display != Display::None {
                world.get_mut::<Node>(entity).unwrap().display = Display::None;
            }
            continue;
        };
        if [arrow.from, arrow.to].iter().any(|end| {
            world
                .get::<crate::protein_area::placement::Pending>(*end)
                .is_some()
        }) {
            continue;
        }
        if world.get::<Node>(entity).unwrap().display != Display::Flex {
            world.get_mut::<Node>(entity).unwrap().display = Display::Flex;
        }
        let delta = to - from;
        let length = delta.length() as f32;
        let center = (from + to) * 0.5;
        let item = CanvasItem {
            position: DVec2::new(center.x, center.z),
            size: Vec2::new(length, 32.0),
        };
        if world
            .get::<CanvasItem>(entity)
            .is_none_or(|old| old.position != item.position || old.size != item.size)
        {
            world.entity_mut(entity).insert(item);
        }
        let spatial = Spatial {
            elevation: center.y,
            rotation: DQuat::from_rotation_arc(DVec3::X, delta.normalize()).to_array(),
            depth: Some(0.01),
            world_pinned: false,
        };
        if world.get::<Spatial>(entity) != Some(&spatial) {
            world.entity_mut(entity).insert(spatial);
        }
        let Some(parts) = world.get::<Parts>(entity) else {
            continue;
        };
        let (shaft, tips, label) = (parts.shaft, parts.tips, parts.label);
        line(world, shaft, Vec2::new(0.0, 16.0), Vec2::new(length, 16.0));
        for (tip, sign) in tips.into_iter().zip([-1.0, 1.0]) {
            line(
                world,
                tip,
                Vec2::new(length, 16.0),
                Vec2::new(length - 12.0_f32.min(length * 0.4), 16.0 + 7.0 * sign),
            );
        }
        world
            .get_mut::<Text>(label)
            .unwrap()
            .set_if_neq(Text::new(&arrow.label));
    }
}
