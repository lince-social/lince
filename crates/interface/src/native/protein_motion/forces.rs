use super::{DQuat, DVec2, DVec3, Entity, HashMap, Member, Settings};

struct Card {
    point: DVec2,
    half: DVec2,
    force: DVec2,
}

fn planar(point: DVec3) -> DVec2 {
    DVec2::new(point.x, point.z)
}

impl Card {
    fn new(member: &Member, center: DVec3, inverse: DQuat, settings: &Settings) -> Self {
        let orientation = inverse * DQuat::from_array(member.rotation);
        let size = member.size.as_dvec2() * 0.5;
        let half = planar(orientation * DVec3::X).abs() * size.x
            + planar(orientation * DVec3::Z).abs() * size.y;
        let point = planar(inverse * (member.position - center));
        Self {
            point,
            half,
            force: -point * settings.center,
        }
    }
}

/// Return the facing edge distance along the line between centers.
fn edge_radius(half: DVec2, direction: DVec2) -> f64 {
    let axes = direction.abs().max(DVec2::splat(1e-12));
    (half / axes).min_element()
}

fn separate(
    first: &Card,
    second: &Card,
    direction: DVec2,
    distance: f64,
    settings: &Settings,
    clearance_stiffness: f64,
) -> DVec2 {
    let bounds = first.half + second.half;
    let delta = first.point - second.point;
    let overlap = bounds + DVec2::splat(settings.clearance) - delta.abs();
    let mut force = DVec2::ZERO;
    if overlap.min_element() > 0.0 {
        // Resolve along the shallow axis, including the clearance around the card.
        force = if overlap.x < overlap.y {
            DVec2::X * overlap.x * direction.x.signum()
        } else {
            DVec2::Y * overlap.y * direction.y.signum()
        } * clearance_stiffness;
    }
    let scale = (bounds.max_element() + settings.clearance).max(1.0);
    let ratio = distance / scale;
    if ratio < 3.0 && settings.repulsion > 0.0 {
        let weight = (1.0 - ratio / 3.0).powi(2) / ratio.max(0.25).powi(2);
        force += direction * (settings.repulsion * weight).min(100_000.0);
    }
    force
}

pub(super) fn forces(
    members: &[Member],
    links: &[(Entity, Entity)],
    center: DVec3,
    rotation: DQuat,
    settings: &Settings,
    alpha: f64,
) -> Vec<(Entity, DVec3)> {
    let inverse = rotation.inverse();
    let mut cards: Vec<_> = members
        .iter()
        .map(|m| Card::new(m, center, inverse, settings))
        .collect();
    let max_extent = cards
        .iter()
        .map(|c| 2.0 * c.half.max_element())
        .fold(1.0, f64::max);
    // Every interacting pair is in the same cell or one of its eight neighbors.
    // Long links are handled separately, so their cost depends on actual edges.
    // Let clearance cool more slowly than link and centering forces. A hub
    // can then finish opening gaps before its layout goes to sleep.
    let clearance_stiffness = 650.0 / alpha.sqrt();
    let cell_width = 3.0 * (max_extent + settings.clearance);
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for next in 0..cards.len() {
        let cell = (cards[next].point / cell_width).floor().as_i64vec2();
        for x in cell.x.saturating_sub(1)..=cell.x.saturating_add(1) {
            for y in cell.y.saturating_sub(1)..=cell.y.saturating_add(1) {
                let Some(previous) = cells.get(&(x, y)) else {
                    continue;
                };
                for &index in previous {
                    let delta = cards[index].point - cards[next].point;
                    let range = 3.0
                        * ((cards[index].half + cards[next].half).max_element()
                            + settings.clearance)
                            .max(1.0);
                    if delta.length_squared() >= range * range {
                        continue;
                    }
                    let distance = delta.length();
                    let direction = if distance > 1e-9 {
                        delta / distance
                    } else {
                        // Reproducible separation for exactly coincident centers.
                        DVec2::from_angle((index * 17 + next * 31) as f64 * 2.399963229728653)
                    };
                    let force = separate(
                        &cards[index],
                        &cards[next],
                        direction,
                        distance,
                        settings,
                        clearance_stiffness,
                    );
                    cards[index].force += force;
                    cards[next].force -= force;
                }
            }
        }
        cells.entry((cell.x, cell.y)).or_default().push(next);
    }
    let indices: HashMap<_, _> = if links.is_empty() {
        HashMap::new()
    } else {
        members
            .iter()
            .enumerate()
            .map(|(index, m)| (m.entity, index))
            .collect()
    };
    for (from, to) in links {
        let (Some(&a), Some(&b)) = (indices.get(from), indices.get(to)) else {
            continue;
        };
        let delta = cards[b].point - cards[a].point;
        let distance = delta.length();
        let Some(direction) = delta.try_normalize() else {
            continue;
        };
        let rest = edge_radius(cards[a].half, direction)
            + edge_radius(cards[b].half, direction)
            + settings.link_spacing.max(settings.clearance);
        let force = direction * ((distance - rest) * 8.0);
        cards[a].force += force;
        cards[b].force -= force;
    }
    members
        .iter()
        .zip(cards)
        .map(|(member, card)| {
            let force = if member.held {
                DVec3::ZERO
            } else {
                rotation
                    * (DVec3::new(card.force.x, 0.0, card.force.y) * alpha)
                        .clamp_length_max(100_000.0)
            };
            (member.entity, force)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec2;

    #[test]
    fn three_hundred_linked_cards_open_space_before_cooling() {
        let settings = Settings::default();
        let mut members: Vec<_> = (0..300)
            .map(|i| Member {
                entity: Entity::from_raw_u32(i + 1).unwrap(),
                position: DVec3::new(f64::from(i % 20) * 140.0, 0.0, f64::from(i / 20) * 60.0),
                size: Vec2::new(240.0 + (i % 4) as f32 * 20.0, 80.0),
                rotation: DQuat::IDENTITY.to_array(),
                held: false,
                revision: 0,
            })
            .collect();
        members.sort_by_key(|m| m.entity);
        let links: Vec<_> = (1..members.len())
            .map(|i| {
                let parent = if i % 9 == 0 { 0 } else { i - 1 };
                (members[parent].entity, members[i].entity)
            })
            .collect();
        fn overlaps(cards: &[Member]) -> usize {
            cards
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    cards[..i]
                        .iter()
                        .filter(|b| {
                            let delta = planar(a.position - b.position).abs();
                            (delta.cmplt((a.size + b.size).as_dvec2() * 0.5)).all()
                        })
                        .count()
                })
                .sum()
        }
        let initial = overlaps(&members);
        assert!(initial > 300);
        let mut velocities = vec![DVec3::ZERO; members.len()];
        let dt = 1.0 / 60.0;
        let mut alpha = 1.0;
        while alpha >= 0.01 {
            let updates = forces(
                &members,
                &links,
                DVec3::ZERO,
                DQuat::IDENTITY,
                &settings,
                alpha,
            );
            for ((member, velocity), (_, force)) in
                members.iter_mut().zip(&mut velocities).zip(updates)
            {
                *velocity = (*velocity + force * dt) / (1.0 + 4.0 * dt);
                member.position += *velocity * dt;
            }
            alpha *= (-settings.cooling * dt).exp();
        }
        let final_count = overlaps(&members);
        eprintln!("300-card layout overlaps: {initial} initially, {final_count} after cooling");
        assert_eq!(final_count, 0);
    }
}
