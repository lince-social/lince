use super::{
    data::{Catalog, Kind, MAX_OBJECTS, MIN_IMPACT_SPEED, STEP},
    objects::{Part, Thing},
};
use bevy::prelude::*;

struct Contact {
    first: usize,
    second: usize,
    first_part: usize,
    second_part: usize,
    velocity: Vec2,
}

fn swept_distance(relative_start: Vec2, relative_end: Vec2) -> f32 {
    let travel = relative_end - relative_start;
    let amount =
        (-relative_start.dot(travel) / travel.length_squared().max(0.0001)).clamp(0.0, 1.0);
    (relative_start + travel * amount).length()
}

pub(super) fn contacts(things: &mut [Thing], catalog: &Catalog) {
    let mut contacts = Vec::new();
    for first in 0..things.len() {
        let a = &things[first];
        if a.impact_cooldown > 0.0 {
            continue;
        }
        let a_size = catalog.material(a.kind).part_size;
        for (second, b) in things.iter().enumerate().skip(first + 1) {
            if b.impact_cooldown > 0.0 || a.mount.is_some() && b.mount.is_some() {
                continue;
            }
            let b_size = catalog.material(b.kind).part_size;
            let travel = (a.position - a.previous_position).length()
                + (b.position - b.previous_position).length();
            if a.center(a_size).distance(b.center(b_size))
                > a.radius(a_size) + b.radius(b_size) + travel
            {
                continue;
            }
            let mut strongest: Option<Contact> = None;
            for (first_part, part_a) in a.parts.iter().enumerate() {
                if part_a.integrity <= 0.0 {
                    continue;
                }
                let a_now = a.point(part_a.cell, a_size);
                let a_before = a.previous_point(part_a.cell, a_size);
                for (second_part, part_b) in b.parts.iter().enumerate() {
                    if part_b.integrity <= 0.0 {
                        continue;
                    }
                    let b_now = b.point(part_b.cell, b_size);
                    let b_before = b.previous_point(part_b.cell, b_size);
                    let velocity = ((a_now - a_before) - (b_now - b_before)) / STEP;
                    if velocity.length() < MIN_IMPACT_SPEED
                        || swept_distance(a_before - b_before, a_now - b_now)
                            > (a_size + b_size) * 0.65
                    {
                        continue;
                    }
                    if strongest
                        .as_ref()
                        .is_none_or(|old| velocity.length_squared() > old.velocity.length_squared())
                    {
                        strongest = Some(Contact {
                            first,
                            second,
                            first_part,
                            second_part,
                            velocity,
                        });
                    }
                }
            }
            if let Some(contact) = strongest {
                contacts.push(contact);
            }
        }
    }
    for contact in contacts {
        let (left, right) = things.split_at_mut(contact.second);
        let a = &mut left[contact.first];
        let b = &mut right[0];
        if a.impact_cooldown > 0.0 || b.impact_cooldown > 0.0 {
            continue;
        }
        let speed = contact.velocity.length().min(400.0);
        let a_material = catalog.material(a.kind);
        let b_material = catalog.material(b.kind);
        damage(
            a,
            contact.first_part,
            speed * (b_material.impact_transfer + a_material.impact_transfer * a_material.wear),
            -contact.velocity,
            catalog,
        );
        damage(
            b,
            contact.second_part,
            speed * (a_material.impact_transfer + b_material.impact_transfer * b_material.wear),
            contact.velocity,
            catalog,
        );
        a.impact_cooldown = 0.065;
        b.impact_cooldown = 0.065;
        if a.mount.is_none() {
            a.velocity -= contact.velocity * 0.13;
        }
        if b.mount.is_none() {
            b.velocity += contact.velocity * 0.13;
        }
    }
}

pub(super) fn damage(
    thing: &mut Thing,
    hit: usize,
    amount: f32,
    direction: Vec2,
    catalog: &Catalog,
) {
    let Some(hit_part) = thing.parts.get(hit) else {
        return;
    };
    let cell = hit_part.cell;
    let material = catalog.material(thing.kind);
    let size = material.part_size;
    let center = thing.center(size);
    let central = thing.kind == Kind::Bush
        && thing.point(cell, size).distance(center) < thing.radius(size) * 0.38;
    for (index, part) in thing.parts.iter_mut().enumerate() {
        let offset = part.cell - cell;
        let weight = if index == hit {
            1.0
        } else if central && (offset.x == 0 || offset.y == 0) {
            1.2
        } else if thing.kind == Kind::Bush && offset.length_squared() <= 2 {
            0.28
        } else {
            0.0
        };
        part.integrity -= amount * weight;
        if thing.kind == Kind::Bush
            && !central
            && amount >= material.integrity * 0.4
            && offset.length_squared() <= 2
        {
            part.separated = true;
        }
    }
    thing.scarred = thing
        .parts
        .iter()
        .any(|part| part.integrity <= 0.0 || part.separated);
    thing.scatter = direction.normalize_or_zero();
    thing.scatter_strength = if central { 12.0 } else { 62.0 };
}

fn connected(mut parts: Vec<Part>) -> Vec<Vec<Part>> {
    let mut groups = Vec::new();
    while let Some(first) = parts.pop() {
        let mut group = vec![first];
        let mut cursor = 0;
        while cursor < group.len() {
            let cell = group[cursor].cell;
            let mut index = 0;
            while index < parts.len() {
                let offset = parts[index].cell - cell;
                if offset.x.abs() + offset.y.abs() == 1
                    && parts[index].separated == group[cursor].separated
                {
                    group.push(parts.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            cursor += 1;
        }
        for part in &mut group {
            part.separated = false;
        }
        groups.push(group);
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.len()));
    groups
}

pub(super) fn split(things: &mut Vec<Thing>, next_id: &mut u64, catalog: &Catalog) -> bool {
    let mut changed = false;
    let original_count = things.len();
    for index in 0..original_count {
        if !things[index].scarred {
            continue;
        }
        changed = true;
        things[index].parts.retain(|part| part.integrity > 0.0);
        things[index].scarred = false;
        let mut groups = connected(std::mem::take(&mut things[index].parts));
        if groups.is_empty() {
            continue;
        }
        if let Some(mount) = things[index].mount {
            if let Some(attached) = groups
                .iter()
                .position(|group| group.iter().any(|part| part.cell == mount.anchor))
            {
                groups.swap(0, attached);
            } else {
                things[index].mount = None;
            }
        }
        things[index].parts = groups.remove(0);
        let size = catalog.material(things[index].kind).part_size;
        let origin = things[index].center(size);
        let mut fragments = Vec::new();
        for group in groups {
            if things.len() + fragments.len() >= MAX_OBJECTS {
                things[index].parts.extend(group);
                things[index].mount = None;
                continue;
            }
            let thing = &things[index];
            let mut fragment = Thing {
                id: *next_id,
                kind: thing.kind,
                parts: group,
                position: thing.position,
                angle: thing.angle,
                previous_position: thing.position,
                previous_angle: thing.angle,
                velocity: thing.velocity,
                spin: 0.0,
                mount: None,
                thrust: 0.0,
                scarred: false,
                scatter: Vec2::ZERO,
                scatter_strength: 0.0,
                impact_cooldown: 0.08,
            };
            *next_id = (*next_id).wrapping_add(1);
            let radial = (fragment.center(size) - origin).normalize_or_zero();
            fragment.velocity +=
                (thing.scatter + radial * 0.7).normalize_or_zero() * thing.scatter_strength;
            fragment.spin = radial.perp_dot(thing.scatter) * 3.0;
            fragment.recenter(size);
            fragments.push(fragment);
        }
        if things[index].mount.is_none() {
            let scatter = things[index].scatter;
            let strength = things[index].scatter_strength;
            things[index].velocity += scatter * strength * 0.3;
        }
        things[index].recenter(size);
        things.extend(fragments);
    }
    things.retain(|thing| !thing.parts.is_empty());
    changed
}
