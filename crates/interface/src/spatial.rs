use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn length_squared(self) -> f32 {
        self.x.mul_add(self.x, self.y * self.y)
    }

    fn normalized_or_zero(self) -> Self {
        let length_squared = self.length_squared();
        if length_squared <= f32::EPSILON {
            Self::ZERO
        } else {
            self * length_squared.sqrt().recip()
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, right: Self) -> Self::Output {
        Self::new(self.x + right.x, self.y + right.y)
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, right: Self) {
        self.x += right.x;
        self.y += right.y;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, right: Self) -> Self::Output {
        Self::new(self.x - right.x, self.y - right.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, right: f32) -> Self::Output {
        Self::new(self.x * right, self.y * right)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub minimum: Vec2,
    pub maximum: Vec2,
}

impl Rect {
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.minimum.x
            && point.x <= self.maximum.x
            && point.y >= self.minimum.y
            && point.y <= self.maximum.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForceMode {
    Pull,
    Repel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Right,
    Left,
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Area {
    Force {
        uid: String,
        center: Vec2,
        radius: f32,
        strength: f32,
        mode: ForceMode,
        concept: Option<String>,
    },
    Sort {
        uid: String,
        bounds: Rect,
        direction: SortDirection,
        spacing: f32,
        concept: Option<String>,
    },
    Mutation {
        uid: String,
        bounds: Rect,
        action: String,
        armed: bool,
        concept: Option<String>,
    },
    Immunity {
        uid: String,
        protein_uid: String,
        bounds: Rect,
    },
}

impl Area {
    pub fn uid(&self) -> &str {
        match self {
            Self::Force { uid, .. }
            | Self::Sort { uid, .. }
            | Self::Mutation { uid, .. }
            | Self::Immunity { uid, .. } => uid,
        }
    }

    fn concept(&self) -> Option<&str> {
        match self {
            Self::Force { concept, .. }
            | Self::Sort { concept, .. }
            | Self::Mutation { concept, .. } => concept.as_deref(),
            Self::Immunity { .. } => None,
        }
    }

    fn effective_point(&self) -> Option<Vec2> {
        match self {
            Self::Force { center, .. } => Some(*center),
            Self::Sort { bounds, .. } | Self::Mutation { bounds, .. } => Some(Vec2::new(
                (bounds.minimum.x + bounds.maximum.x) * 0.5,
                (bounds.minimum.y + bounds.maximum.y) * 0.5,
            )),
            Self::Immunity { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySeed {
    pub uid: String,
    pub protein_uid: String,
    pub concept: String,
    pub sort_key: i64,
    pub position: Vec2,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPreview {
    pub visit_uid: String,
    pub area_uid: String,
    pub body_uid: String,
    pub action: String,
    pub armed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepMetrics {
    pub field_nanos: u64,
    pub sort_nanos: u64,
    pub broad_phase_nanos: u64,
    pub narrow_phase_nanos: u64,
    pub integration_nanos: u64,
    pub candidate_pairs: u64,
    pub resolved_pairs: u64,
    pub awake_bodies: u64,
    pub dirty_bodies: u64,
}

pub trait PhysicsAdapter {
    fn name(&self) -> &'static str;
    fn step(&mut self, fixed_seconds: f32) -> StepMetrics;
    fn positions(&self) -> &[Vec2];
    fn velocities(&self) -> &[Vec2];
    fn action_previews(&self) -> &[ActionPreview];
}

#[derive(Clone)]
pub struct FieldSolver {
    ids: Vec<String>,
    proteins: Vec<String>,
    concepts: Vec<String>,
    sort_keys: Vec<i64>,
    positions: Vec<Vec2>,
    velocities: Vec<Vec2>,
    radii: Vec<f32>,
    areas: Vec<Area>,
    centering_strength: f32,
    inside_mutations: BTreeSet<(usize, usize)>,
    action_previews: Vec<ActionPreview>,
    visits: BTreeMap<(usize, usize), u64>,
}

impl FieldSolver {
    pub fn new(
        bodies: Vec<BodySeed>,
        areas: Vec<Area>,
        centering_strength: f32,
    ) -> Result<Self, String> {
        if bodies.is_empty() {
            return Err("field solver needs at least one body".into());
        }
        if !centering_strength.is_finite() || centering_strength < 0.0 {
            return Err("centering strength must be finite and nonnegative".into());
        }
        let mut unique_bodies = BTreeSet::new();
        for body in &bodies {
            if !unique_bodies.insert(body.uid.as_str()) {
                return Err(format!("duplicate body uid {}", body.uid));
            }
            if body.uid.is_empty()
                || body.protein_uid.is_empty()
                || body.concept.is_empty()
                || !body.position.x.is_finite()
                || !body.position.y.is_finite()
                || !body.radius.is_finite()
                || body.radius <= 0.0
            {
                return Err(format!("invalid body {}", body.uid));
            }
        }
        let mut unique_areas = BTreeSet::new();
        for area in &areas {
            if !unique_areas.insert(area.uid()) {
                return Err(format!("duplicate Area uid {}", area.uid()));
            }
            validate_area(area)?;
        }
        Ok(Self {
            ids: bodies.iter().map(|body| body.uid.clone()).collect(),
            proteins: bodies.iter().map(|body| body.protein_uid.clone()).collect(),
            concepts: bodies.iter().map(|body| body.concept.clone()).collect(),
            sort_keys: bodies.iter().map(|body| body.sort_key).collect(),
            positions: bodies.iter().map(|body| body.position).collect(),
            velocities: vec![Vec2::ZERO; bodies.len()],
            radii: bodies.iter().map(|body| body.radius).collect(),
            areas,
            centering_strength,
            inside_mutations: BTreeSet::new(),
            action_previews: Vec::new(),
            visits: BTreeMap::new(),
        })
    }

    pub fn deterministic_fixture(count: usize, seed: u64) -> Self {
        let mut random = DeterministicRandom::new(seed);
        let bodies = (0..count)
            .map(|index| BodySeed {
                uid: format!("body-{index}"),
                protein_uid: if index % 5 == 0 {
                    "protected-records".into()
                } else {
                    "current-records".into()
                },
                concept: if index % 3 == 0 {
                    "urgent".into()
                } else {
                    "ordinary".into()
                },
                sort_key: index as i64,
                position: Vec2::new(random.range(-900.0, 900.0), random.range(-520.0, 520.0)),
                radius: random.range(4.0, 8.0),
            })
            .collect();
        let areas = vec![
            Area::Force {
                uid: "urgent-pull".into(),
                center: Vec2::new(-280.0, 0.0),
                radius: 640.0,
                strength: 42.0,
                mode: ForceMode::Pull,
                concept: Some("urgent".into()),
            },
            Area::Force {
                uid: "ordinary-repel".into(),
                center: Vec2::new(260.0, 40.0),
                radius: 260.0,
                strength: 28.0,
                mode: ForceMode::Repel,
                concept: Some("ordinary".into()),
            },
            Area::Sort {
                uid: "delivery-line".into(),
                bounds: Rect {
                    minimum: Vec2::new(-180.0, -90.0),
                    maximum: Vec2::new(420.0, 90.0),
                },
                direction: SortDirection::Right,
                spacing: 18.0,
                concept: Some("urgent".into()),
            },
            Area::Mutation {
                uid: "complete-preview".into(),
                bounds: Rect {
                    minimum: Vec2::new(300.0, -120.0),
                    maximum: Vec2::new(520.0, 120.0),
                },
                action: "record-change-quantity".into(),
                armed: false,
                concept: None,
            },
            Area::Immunity {
                uid: "protected-boundary".into(),
                protein_uid: "protected-records".into(),
                bounds: Rect {
                    minimum: Vec2::new(-500.0, -300.0),
                    maximum: Vec2::new(500.0, 300.0),
                },
            },
        ];
        Self::new(bodies, areas, 0.35).unwrap()
    }

    pub fn semantic_trace(&self) -> Vec<String> {
        let mut trace = self
            .action_previews
            .iter()
            .map(|preview| {
                format!(
                    "{}:{}:{}:{}",
                    preview.visit_uid, preview.area_uid, preview.body_uid, preview.armed
                )
            })
            .collect::<Vec<_>>();
        trace.push(format!("checksum:{:016x}", self.position_checksum()));
        trace
    }

    pub fn extraction_trace(&self, camera: Rect) -> Vec<String> {
        self.ids
            .iter()
            .zip(&self.positions)
            .filter(|(_, position)| camera.contains(**position))
            .map(|(uid, _)| uid.clone())
            .collect()
    }

    pub fn position_checksum(&self) -> u64 {
        self.positions
            .iter()
            .fold(0xcbf29ce484222325, |hash, point| {
                let hash = (hash ^ u64::from(point.x.to_bits())).wrapping_mul(0x100000001b3);
                (hash ^ u64::from(point.y.to_bits())).wrapping_mul(0x100000001b3)
            })
    }

    pub fn drag_body(&mut self, uid: &str, position: Vec2) -> Result<usize, String> {
        let index = self
            .ids
            .iter()
            .position(|candidate| candidate == uid)
            .ok_or_else(|| format!("unknown body {uid}"))?;
        self.positions[index] = position;
        self.velocities[index] = Vec2::ZERO;
        Ok(1)
    }

    fn apply_fields(&mut self, fixed_seconds: f32) -> u64 {
        let mut dirty = 0;
        for index in 0..self.positions.len() {
            let position = self.positions[index];
            let mut acceleration = position * -self.centering_strength;
            for area in &self.areas {
                let Area::Force {
                    center,
                    radius,
                    strength,
                    mode,
                    ..
                } = area
                else {
                    continue;
                };
                if !self.matches(index, area) || self.blocked_by_immunity(index, area) {
                    continue;
                }
                let offset = *center - position;
                let distance_squared = offset.length_squared();
                if distance_squared > radius * radius {
                    continue;
                }
                let falloff = 1.0 - distance_squared.sqrt() / radius;
                let direction = offset.normalized_or_zero();
                let sign = if *mode == ForceMode::Pull { 1.0 } else { -1.0 };
                acceleration += direction * (*strength * falloff * sign);
            }
            if acceleration.length_squared() > 0.000_001 {
                self.velocities[index] += acceleration * fixed_seconds;
                dirty += 1;
            }
        }
        dirty
    }

    fn apply_sort_constraints(&mut self, fixed_seconds: f32) -> u64 {
        let mut dirty = 0;
        for area_index in 0..self.areas.len() {
            let Area::Sort {
                bounds,
                direction,
                spacing,
                ..
            } = &self.areas[area_index]
            else {
                continue;
            };
            let bounds = *bounds;
            let direction = *direction;
            let spacing = *spacing;
            let mut matching = (0..self.positions.len())
                .filter(|index| {
                    bounds.contains(self.positions[*index])
                        && self.matches(*index, &self.areas[area_index])
                        && !self.blocked_by_immunity(*index, &self.areas[area_index])
                })
                .collect::<Vec<_>>();
            matching.sort_by_key(|index| (self.sort_keys[*index], self.ids[*index].clone()));
            let horizontal = matches!(direction, SortDirection::Right | SortDirection::Left);
            let sign = if matches!(direction, SortDirection::Right | SortDirection::Down) {
                1.0
            } else {
                -1.0
            };
            let start = if horizontal {
                Vec2::new(
                    if sign > 0.0 {
                        bounds.minimum.x + spacing
                    } else {
                        bounds.maximum.x - spacing
                    },
                    (bounds.minimum.y + bounds.maximum.y) * 0.5,
                )
            } else {
                Vec2::new(
                    (bounds.minimum.x + bounds.maximum.x) * 0.5,
                    if sign > 0.0 {
                        bounds.minimum.y + spacing
                    } else {
                        bounds.maximum.y - spacing
                    },
                )
            };
            for (order, index) in matching.into_iter().enumerate() {
                let offset = spacing * order as f32 * sign;
                let target = if horizontal {
                    Vec2::new(start.x + offset, start.y)
                } else {
                    Vec2::new(start.x, start.y + offset)
                };
                self.velocities[index] += (target - self.positions[index]) * (18.0 * fixed_seconds);
                dirty += 1;
            }
        }
        dirty
    }

    fn detect_mutation_entries(&mut self) {
        let mut next_inside = BTreeSet::new();
        for area_index in 0..self.areas.len() {
            let Area::Mutation {
                bounds,
                action,
                armed,
                ..
            } = &self.areas[area_index]
            else {
                continue;
            };
            for body_index in 0..self.positions.len() {
                if !bounds.contains(self.positions[body_index])
                    || !self.matches(body_index, &self.areas[area_index])
                    || self.blocked_by_immunity(body_index, &self.areas[area_index])
                {
                    continue;
                }
                let key = (area_index, body_index);
                next_inside.insert(key);
                if self.inside_mutations.contains(&key) {
                    continue;
                }
                let visit = self.visits.entry(key).or_default();
                *visit += 1;
                self.action_previews.push(ActionPreview {
                    visit_uid: format!(
                        "{}:{}:{}",
                        self.areas[area_index].uid(),
                        self.ids[body_index],
                        *visit
                    ),
                    area_uid: self.areas[area_index].uid().into(),
                    body_uid: self.ids[body_index].clone(),
                    action: action.clone(),
                    armed: *armed,
                });
            }
        }
        self.inside_mutations = next_inside;
    }

    fn collision_pairs(&self, cell_size: f32) -> Vec<(usize, usize)> {
        let mut cells = BTreeMap::<(i32, i32), Vec<usize>>::new();
        for (index, position) in self.positions.iter().enumerate() {
            let key = (
                (position.x / cell_size).floor() as i32,
                (position.y / cell_size).floor() as i32,
            );
            cells.entry(key).or_default().push(index);
        }
        let mut pairs = Vec::new();
        for ((cell_x, cell_y), members) in &cells {
            for left_offset in 0..members.len() {
                for right_offset in (left_offset + 1)..members.len() {
                    pairs.push((members[left_offset], members[right_offset]));
                }
            }
            for (offset_x, offset_y) in [(1, -1), (1, 0), (1, 1), (0, 1)] {
                let Some(neighbours) = cells.get(&(cell_x + offset_x, cell_y + offset_y)) else {
                    continue;
                };
                for left in members {
                    for right in neighbours {
                        pairs.push((*left, *right));
                    }
                }
            }
        }
        pairs
    }

    fn resolve_collisions(&mut self, pairs: &[(usize, usize)]) -> u64 {
        let mut resolved = 0;
        for &(left, right) in pairs {
            let delta = self.positions[right] - self.positions[left];
            let minimum = self.radii[left] + self.radii[right];
            let distance_squared = delta.length_squared();
            if distance_squared >= minimum * minimum {
                continue;
            }
            let direction = if distance_squared <= f32::EPSILON {
                if (left + right) % 2 == 0 {
                    Vec2::new(1.0, 0.0)
                } else {
                    Vec2::new(0.0, 1.0)
                }
            } else {
                delta * distance_squared.sqrt().recip()
            };
            let penetration = minimum - distance_squared.sqrt();
            let correction = direction * (penetration * 0.5);
            self.positions[left] = self.positions[left] - correction;
            self.positions[right] += correction;
            let relative = self.velocities[right] - self.velocities[left];
            let closing = relative.x.mul_add(direction.x, relative.y * direction.y);
            if closing < 0.0 {
                let impulse = direction * (-closing * 0.35);
                self.velocities[left] = self.velocities[left] - impulse;
                self.velocities[right] += impulse;
            }
            resolved += 1;
        }
        resolved
    }

    fn integrate(&mut self, fixed_seconds: f32) -> u64 {
        let mut awake = 0;
        for (position, velocity) in self.positions.iter_mut().zip(&mut self.velocities) {
            *velocity = *velocity * 0.985;
            if velocity.length_squared() > 0.000_004 {
                *position += *velocity * fixed_seconds;
                awake += 1;
            } else {
                *velocity = Vec2::ZERO;
            }
        }
        awake
    }

    fn matches(&self, body_index: usize, area: &Area) -> bool {
        area.concept()
            .is_none_or(|concept| self.concepts[body_index] == concept)
    }

    fn blocked_by_immunity(&self, body_index: usize, area: &Area) -> bool {
        let Some(area_point) = area.effective_point() else {
            return false;
        };
        self.areas.iter().any(|candidate| {
            let Area::Immunity {
                protein_uid,
                bounds,
                ..
            } = candidate
            else {
                return false;
            };
            self.proteins[body_index] == *protein_uid && !bounds.contains(area_point)
        })
    }
}

impl PhysicsAdapter for FieldSolver {
    fn name(&self) -> &'static str {
        "lince-field-solver"
    }

    fn step(&mut self, fixed_seconds: f32) -> StepMetrics {
        let field_started = Instant::now();
        let mut dirty = self.apply_fields(fixed_seconds);
        let field_nanos = elapsed_nanos(field_started);
        let sort_started = Instant::now();
        dirty += self.apply_sort_constraints(fixed_seconds);
        self.detect_mutation_entries();
        let sort_nanos = elapsed_nanos(sort_started);
        let broad_started = Instant::now();
        let pairs = self.collision_pairs(18.0);
        let broad_phase_nanos = elapsed_nanos(broad_started);
        let narrow_started = Instant::now();
        let resolved = self.resolve_collisions(&pairs);
        let narrow_phase_nanos = elapsed_nanos(narrow_started);
        let integration_started = Instant::now();
        let awake = self.integrate(fixed_seconds);
        let integration_nanos = elapsed_nanos(integration_started);
        StepMetrics {
            field_nanos,
            sort_nanos,
            broad_phase_nanos,
            narrow_phase_nanos,
            integration_nanos,
            candidate_pairs: pairs.len() as u64,
            resolved_pairs: resolved,
            awake_bodies: awake,
            dirty_bodies: dirty.min(self.positions.len() as u64),
        }
    }

    fn positions(&self) -> &[Vec2] {
        &self.positions
    }

    fn velocities(&self) -> &[Vec2] {
        &self.velocities
    }

    fn action_previews(&self) -> &[ActionPreview] {
        &self.action_previews
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentFrame {
    pub origin_x: f64,
    pub origin_y: f64,
}

impl ParentFrame {
    pub fn camera_local(self, world_x: f64, world_y: f64) -> Vec2 {
        Vec2::new(
            (world_x - self.origin_x) as f32,
            (world_y - self.origin_y) as f32,
        )
    }

    pub fn rebase(self, local: Vec2, next: Self) -> Vec2 {
        let world_x = self.origin_x + f64::from(local.x);
        let world_y = self.origin_y + f64::from(local.y);
        next.camera_local(world_x, world_y)
    }
}

fn elapsed_nanos(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn validate_area(area: &Area) -> Result<(), String> {
    if area.uid().is_empty() {
        return Err("Area uid must not be empty".into());
    }
    match area {
        Area::Force {
            center,
            radius,
            strength,
            ..
        } if !center.x.is_finite()
            || !center.y.is_finite()
            || !radius.is_finite()
            || *radius <= 0.0
            || !strength.is_finite()
            || *strength < 0.0 =>
        {
            Err(format!("invalid force Area {}", area.uid()))
        }
        Area::Sort {
            bounds, spacing, ..
        } if !valid_rect(*bounds) || !spacing.is_finite() || *spacing <= 0.0 => {
            Err(format!("invalid sort Area {}", area.uid()))
        }
        Area::Mutation { bounds, action, .. } if !valid_rect(*bounds) || action.is_empty() => {
            Err(format!("invalid mutation Area {}", area.uid()))
        }
        Area::Immunity {
            protein_uid,
            bounds,
            ..
        } if protein_uid.is_empty() || !valid_rect(*bounds) => {
            Err(format!("invalid immunity Area {}", area.uid()))
        }
        _ => Ok(()),
    }
}

fn valid_rect(rect: Rect) -> bool {
    rect.minimum.x.is_finite()
        && rect.minimum.y.is_finite()
        && rect.maximum.x.is_finite()
        && rect.maximum.y.is_finite()
        && rect.minimum.x < rect.maximum.x
        && rect.minimum.y < rect.maximum.y
}

struct DeterministicRandom {
    state: u64,
}

impl DeterministicRandom {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn range(&mut self, minimum: f32, maximum: f32) -> f32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let fraction = (self.state >> 40) as f32 / (1_u32 << 24) as f32;
        minimum + (maximum - minimum) * fraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_runs_are_deterministic() {
        let mut first = FieldSolver::deterministic_fixture(300, 42);
        let mut second = FieldSolver::deterministic_fixture(300, 42);
        for _ in 0..180 {
            first.step(1.0 / 60.0);
            second.step(1.0 / 60.0);
        }
        assert_eq!(first.position_checksum(), second.position_checksum());
        assert_eq!(first.semantic_trace(), second.semantic_trace());
    }

    #[test]
    fn camera_visibility_never_changes_semantics() {
        let mut on_camera = FieldSolver::deterministic_fixture(500, 71);
        let mut off_camera = on_camera.clone();
        for _ in 0..240 {
            on_camera.step(1.0 / 60.0);
            off_camera.step(1.0 / 60.0);
            let _ = on_camera.extraction_trace(Rect {
                minimum: Vec2::new(-600.0, -400.0),
                maximum: Vec2::new(600.0, 400.0),
            });
            let _ = off_camera.extraction_trace(Rect {
                minimum: Vec2::new(10_000.0, 10_000.0),
                maximum: Vec2::new(11_000.0, 11_000.0),
            });
        }
        assert_eq!(on_camera.semantic_trace(), off_camera.semantic_trace());
        assert_ne!(
            on_camera.extraction_trace(Rect {
                minimum: Vec2::new(-600.0, -400.0),
                maximum: Vec2::new(600.0, 400.0),
            }),
            off_camera.extraction_trace(Rect {
                minimum: Vec2::new(10_000.0, 10_000.0),
                maximum: Vec2::new(11_000.0, 11_000.0),
            })
        );
    }

    #[test]
    fn mutation_fires_once_per_entry_and_stays_preview_only() {
        let body = BodySeed {
            uid: "record-a".into(),
            protein_uid: "records".into(),
            concept: "ordinary".into(),
            sort_key: 0,
            position: Vec2::new(0.0, 0.0),
            radius: 5.0,
        };
        let area = Area::Mutation {
            uid: "done".into(),
            bounds: Rect {
                minimum: Vec2::new(-10.0, -10.0),
                maximum: Vec2::new(10.0, 10.0),
            },
            action: "record-change-quantity".into(),
            armed: false,
            concept: None,
        };
        let mut solver = FieldSolver::new(vec![body], vec![area], 0.0).unwrap();
        solver.step(1.0 / 60.0);
        solver.step(1.0 / 60.0);
        assert_eq!(solver.action_previews().len(), 1);
        assert!(!solver.action_previews()[0].armed);
        solver.drag_body("record-a", Vec2::new(20.0, 20.0)).unwrap();
        solver.step(1.0 / 60.0);
        solver.drag_body("record-a", Vec2::ZERO).unwrap();
        solver.step(1.0 / 60.0);
        assert_eq!(solver.action_previews().len(), 2);
        assert_ne!(
            solver.action_previews()[0].visit_uid,
            solver.action_previews()[1].visit_uid
        );
    }

    #[test]
    fn parent_frame_rebase_preserves_world_position() {
        let first = ParentFrame {
            origin_x: 6_378_137.0,
            origin_y: -4_200_000.0,
        };
        let next = ParentFrame {
            origin_x: 6_378_240.0,
            origin_y: -4_199_920.0,
        };
        let local = first.camera_local(6_378_155.25, -4_199_981.5);
        let rebased = first.rebase(local, next);
        assert!((rebased.x + 84.75).abs() < 0.001);
        assert!((rebased.y + 61.5).abs() < 0.001);
    }
}
