use crate::{canvas::CanvasItem, workspace::WorkspaceMember};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};

pub const MAX_AREAS: usize = 256;
pub const MAX_POINTS: usize = 128;
pub const MAX_RULES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    Square,
    Circle,
    Drawn,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AreaShape {
    Square,
    Circle,
    Polygon(Vec<[f64; 2]>),
}

impl AreaShape {
    pub fn kind(&self) -> ShapeKind {
        match self {
            Self::Square => ShapeKind::Square,
            Self::Circle => ShapeKind::Circle,
            Self::Polygon(_) => ShapeKind::Drawn,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Attract,
    Repel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReachMode {
    #[default]
    Limited,
    Unlimited,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReachShape {
    Square,
    #[default]
    FollowShape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reach {
    pub mode: ReachMode,
    pub shape: ReachShape,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum AttractionTarget {
    #[default]
    Center,
    Point([f64; 2]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Property {
    Title,
    Slug,
    Kind,
    Quantity,
    Identity,
}

impl Property {
    pub const ALL: [Self; 5] = [
        Self::Title,
        Self::Slug,
        Self::Kind,
        Self::Quantity,
        Self::Identity,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Slug => "Slug",
            Self::Kind => "Kind",
            Self::Quantity => "Quantity",
            Self::Identity => "Record identity",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Title => "head",
            Self::Slug => "slug",
            Self::Kind => "kind",
            Self::Quantity => "quantity",
            Self::Identity => "uid",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropertyRule {
    pub property: Property,
    pub value: String,
}

impl PropertyRule {
    pub fn validate(&self) -> bool {
        self.value.len() <= 4096
            && !self.value.trim().is_empty()
            && (self.property != Property::Quantity
                || self.value.parse::<f64>().is_ok_and(f64::is_finite))
    }

    pub fn matches(&self, record: &RecordProperties) -> bool {
        if !self.validate() {
            return false;
        }
        let Some(value) = record.0.get(self.property.key()) else {
            return false;
        };
        if self.property == Property::Quantity {
            value
                .as_f64()
                .zip(self.value.parse::<f64>().ok())
                .is_some_and(|(actual, expected)| actual.is_finite() && actual == expected)
        } else {
            value.as_str() == Some(self.value.as_str())
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InfluenceArea {
    pub id: String,
    pub name: String,
    pub center: [f64; 2],
    pub size: [f64; 2],
    pub depth: f64,
    pub target: AttractionTarget,
    pub shape: AreaShape,
    pub direction: Direction,
    pub strength: f64,
    #[serde(default)]
    pub reach: Reach,
    pub rules: Vec<PropertyRule>,
    pub match_all: bool,
    #[serde(default)]
    pub changes: crate::area_mutation::AreaChanges,
    #[serde(default)]
    pub protein: Option<crate::protein_area::Config>,
    #[serde(default)]
    pub filter: Option<crate::protein_area::Config>,
    pub sorting: Option<crate::area_effects::Sorting>,
    pub immunity: crate::area_effects::Immunity,
    pub scale: f32,
    pub force_mode: crate::area_effects::ForceMode,
}

impl InfluenceArea {
    pub fn new(shape: AreaShape, center: DVec2, size: DVec2) -> Self {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).expect("area identity");
        Self {
            id: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
            name: "Area of influence".into(),
            center: center.to_array(),
            size: size.to_array(),
            depth: size.min_element(),
            target: AttractionTarget::Center,
            shape,
            direction: Direction::Attract,
            strength: 0.0,
            reach: Reach::default(),
            rules: Vec::new(),
            match_all: true,
            changes: Default::default(),
            protein: None,
            filter: None,
            sorting: None,
            immunity: Default::default(),
            scale: 1.0,
            force_mode: Default::default(),
        }
    }

    pub fn validate(&self) -> bool {
        let size = DVec2::from_array(self.size);
        self.id.len() == 32
            && self.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !self.name.trim().is_empty()
            && self.name.chars().count() <= 80
            && DVec2::from_array(self.center).is_finite()
            && size.is_finite()
            && size.min_element() >= 1.0
            && size.max_element() <= 100_000.0
            && self.depth.is_finite()
            && (1.0..=100_000.0).contains(&self.depth)
            && match self.target {
                AttractionTarget::Center => true,
                AttractionTarget::Point(offset) => {
                    DVec2::from_array(offset).is_finite()
                        && (DVec2::from_array(self.center) + DVec2::from_array(offset)).is_finite()
                }
            }
            && (0.0..=1_000_000.0).contains(&self.strength)
            && self.reach.radius.is_finite()
            && (0.0..=100_000.0).contains(&self.reach.radius)
            && self.rules.len() <= MAX_RULES
            && self.rules.iter().all(PropertyRule::validate)
            && self.changes.validate()
            && self.scale.is_finite()
            && (0.05..=20.0).contains(&self.scale)
            && self.sorting.as_ref().is_none_or(|sort| {
                sort.strength.is_finite() && (0.0..=1_000_000.0).contains(&sort.strength)
            })
            && self
                .protein
                .as_ref()
                .is_none_or(crate::protein_area::Config::valid)
            && self
                .filter
                .as_ref()
                .is_none_or(crate::protein_area::Config::valid)
            && match &self.shape {
                AreaShape::Square | AreaShape::Circle => self.size[0] == self.size[1],
                AreaShape::Polygon(points) => valid_polygon(points),
            }
    }

    pub fn contains(&self, point: DVec2) -> bool {
        let point = (point - DVec2::from_array(self.center)) / DVec2::from_array(self.size);
        if !point.is_finite() || point.abs().max_element() > 0.5 {
            return false;
        }
        match &self.shape {
            AreaShape::Square => true,
            AreaShape::Circle => point.length_squared() <= 0.25,
            AreaShape::Polygon(points) => polygon_contains(points, point),
        }
    }

    pub fn force(&self, point: DVec2, record: &RecordProperties) -> DVec2 {
        self.force_for_match(point, self.matches(record))
    }

    pub(crate) fn force_for_match(&self, point: DVec2, matches: bool) -> DVec2 {
        if self.strength == 0.0 || !self.reaches(point) || !matches {
            return DVec2::ZERO;
        }
        let delta = self.target_position() - point;
        let direction = delta.try_normalize().unwrap_or(DVec2::ZERO);
        direction
            * self.strength
            * if self.direction == Direction::Attract {
                1.0
            } else {
                -1.0
            }
    }

    pub fn target_position(&self) -> DVec2 {
        let center = DVec2::from_array(self.center);
        if let AttractionTarget::Point(offset) = self.target {
            return center + DVec2::from_array(offset);
        }
        if self.contains(center) {
            return center;
        }
        let AreaShape::Polygon(points) = &self.shape else {
            return center;
        };
        let size = DVec2::from_array(self.size);
        let mut levels: Vec<_> = points.iter().map(|p| p[1]).collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        let mut best = None;
        for y in std::iter::once(0.0).chain(levels.windows(2).map(|p| (p[0] + p[1]) * 0.5)) {
            let mut crossings: Vec<_> = points
                .windows(2)
                .filter_map(|edge| {
                    let [a, b] = [edge[0], edge[1]];
                    ((a[1] > y) != (b[1] > y))
                        .then(|| a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]))
                })
                .collect();
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.chunks_exact(2) {
                let offset = DVec2::new((pair[0] + pair[1]) * 0.5, y) * size;
                if best.is_none_or(|old: DVec2| offset.length_squared() < old.length_squared()) {
                    best = Some(offset);
                }
            }
        }
        center + best.unwrap_or(DVec2::ZERO)
    }

    pub fn reaches(&self, point: DVec2) -> bool {
        if !point.is_finite() {
            return false;
        }
        if self.reach.mode == ReachMode::Unlimited {
            return true;
        }
        let local = point - DVec2::from_array(self.center);
        let size = DVec2::from_array(self.size);
        if self.reach.shape == ReachShape::Square {
            return local.abs().max_element() <= size.max_element() * 0.5 + self.reach.radius;
        }
        self.signed_distance(point) <= self.reach.radius
    }

    pub(crate) fn signed_distance(&self, point: DVec2) -> f64 {
        let local = point - DVec2::from_array(self.center);
        let size = DVec2::from_array(self.size);
        match &self.shape {
            AreaShape::Square => {
                let delta = local.abs() - size * 0.5;
                delta.max(DVec2::ZERO).length() + delta.max_element().min(0.0)
            }
            AreaShape::Circle => local.length() - size.x * 0.5,
            AreaShape::Polygon(points) => {
                let distance = points
                    .windows(2)
                    .map(|edge| {
                        let start = DVec2::from_array(edge[0]) * size;
                        let end = DVec2::from_array(edge[1]) * size;
                        let delta = end - start;
                        let along =
                            ((local - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                        local.distance(start + delta * along)
                    })
                    .fold(f64::INFINITY, f64::min);
                if self.contains(point) {
                    -distance
                } else {
                    distance
                }
            }
        }
    }

    pub fn matches(&self, record: &RecordProperties) -> bool {
        !self.rules.is_empty()
            && if self.match_all {
                self.rules.iter().all(|rule| rule.matches(record))
            } else {
                self.rules.iter().any(|rule| rule.matches(record))
            }
    }

    pub fn outline(&self) -> Vec<DVec2> {
        let points = match &self.shape {
            AreaShape::Square => vec![
                DVec2::new(-0.5, -0.5),
                DVec2::new(0.5, -0.5),
                DVec2::new(0.5, 0.5),
                DVec2::new(-0.5, 0.5),
                DVec2::new(-0.5, -0.5),
            ],
            AreaShape::Circle => (0..=64)
                .map(|index| {
                    let angle = std::f64::consts::TAU * f64::from(index) / 64.0;
                    DVec2::new(angle.cos(), angle.sin()) * 0.5
                })
                .collect(),
            AreaShape::Polygon(points) => points.iter().copied().map(DVec2::from_array).collect(),
        };
        points
            .into_iter()
            .map(|point| DVec2::from_array(self.center) + point * DVec2::from_array(self.size))
            .collect()
    }

    pub fn drawn(points: &[DVec2]) -> Option<Self> {
        if !(3..MAX_POINTS).contains(&points.len()) || points.iter().any(|point| !point.is_finite())
        {
            return None;
        }
        let min = points.iter().copied().reduce(DVec2::min)?;
        let max = points.iter().copied().reduce(DVec2::max)?;
        let center = (min + max) * 0.5;
        let size = max - min;
        let mut normalized: Vec<_> = points
            .iter()
            .map(|point| ((*point - center) / size).to_array())
            .collect();
        normalized.push(normalized[0]);
        let area = Self::new(AreaShape::Polygon(normalized), center, size);
        area.validate().then_some(area)
    }
}

fn cross(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a)
}

fn on_segment(a: DVec2, b: DVec2, point: DVec2) -> bool {
    cross(a, b, point).abs() <= 1e-10
        && point.cmpge(a.min(b) - DVec2::splat(1e-10)).all()
        && point.cmple(a.max(b) + DVec2::splat(1e-10)).all()
}

fn intersects(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
        || (cross(a, b, c) * cross(a, b, d) < 0.0 && cross(c, d, a) * cross(c, d, b) < 0.0)
}

fn valid_polygon(points: &[[f64; 2]]) -> bool {
    if !(4..=MAX_POINTS).contains(&points.len()) || points.first() != points.last() {
        return false;
    }
    let points: Vec<_> = points.iter().copied().map(DVec2::from_array).collect();
    if points
        .iter()
        .any(|point| !point.is_finite() || point.abs().max_element() > 0.5)
    {
        return false;
    }
    let edges: Vec<_> = points.windows(2).collect();
    let mut area = 0.0;
    for (i, edge) in edges.iter().enumerate() {
        if edge[0].distance_squared(edge[1]) < 1e-12 {
            return false;
        }
        area += edge[0].perp_dot(edge[1]);
        for (j, other) in edges.iter().enumerate().skip(i + 1) {
            if j == i + 1 || (i == 0 && j == edges.len() - 1) {
                continue;
            }
            if intersects(edge[0], edge[1], other[0], other[1]) {
                return false;
            }
        }
    }
    area.abs() > 1e-8
}

fn polygon_contains(points: &[[f64; 2]], point: DVec2) -> bool {
    let mut inside = false;
    for edge in points.windows(2) {
        let a = DVec2::from_array(edge[0]);
        let b = DVec2::from_array(edge[1]);
        if on_segment(a, b, point) {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct RecordProperties(pub serde_json::Value);

#[derive(Clone, Debug, PartialEq)]
pub struct AreaForce {
    pub area: Entity,
    pub force: DVec2,
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AreaForces(pub Vec<AreaForce>);

impl AreaForces {
    pub fn total(&self) -> DVec2 {
        self.0.iter().map(|force| force.force).sum()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedArea {
    #[serde(default)]
    pub placement: crate::sand_placement::Placement,
    pub workspace: u64,
    pub area: InfluenceArea,
}

pub fn spawn_area(
    world: &mut World,
    root: Entity,
    workspace: u64,
    area: InfluenceArea,
) -> Option<Entity> {
    if !area.validate() {
        return None;
    }
    let elevation = world
        .get::<crate::topology::view::View>(root)
        .map_or(0.0, |view| view.plane);
    let depth = (area.depth != area.size[0].min(area.size[1])).then_some(area.depth);
    Some(
        world
            .spawn((
                CanvasItem {
                    position: DVec2::from_array(area.center),
                    size: DVec2::from_array(area.size).as_vec2(),
                },
                area,
                crate::topology::Spatial {
                    elevation,
                    depth,
                    ..default()
                },
                Pickable::IGNORE,
                ZIndex(-2),
                WorkspaceMember(workspace),
                ChildOf(root),
            ))
            .id(),
    )
}

pub struct AreasPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComputeAreaForces;

impl Plugin for AreasPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::area_input::AreaInputPlugin,
            crate::area_target::AreaTargetPlugin,
            crate::area_drawing::AreaDrawingPlugin,
            crate::area_mutation::AreaMutationPlugin,
        ))
        .add_systems(
            PostUpdate,
            sync_geometry.after(crate::actions::ApplyActions),
        )
        .add_systems(
            PostUpdate,
            forces
                .after(sync_geometry)
                .in_set(ComputeAreaForces)
                .after(crate::actions::ApplyActions),
        )
        .add_systems(
            PostUpdate,
            crate::area_panel::autosave
                .after(bevy::text::EditableTextSystems)
                .before(crate::actions::ApplyActions),
        );
    }
}

fn sync_geometry(
    mut areas: Query<(
        &mut InfluenceArea,
        &mut CanvasItem,
        Option<&crate::topology::Spatial>,
    )>,
) {
    for (mut area, mut item, placement) in &mut areas {
        let position = DVec2::from_array(area.center);
        let size = DVec2::from_array(area.size).as_vec2();
        let depth = placement
            .and_then(|placement| placement.depth)
            .unwrap_or_else(|| area.size[0].min(area.size[1]));
        if area.depth != depth {
            area.depth = depth;
        }
        if item.position != position || item.size != size {
            item.position = position;
            item.size = size;
        }
    }
}

pub(crate) fn forces(world: &mut World) {
    crate::area_effects::update(world);
}

pub(crate) mod tests {
    use super::*;
    use crate::{sand_placement::Pinned, workspace::Workspaces};
    use serde_json::json;

    #[cfg_attr(test, test)]
    fn area_depth_tracks_resizing_until_manually_fixed_and_can_return_to_automatic() {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let owner = spawn_area(
            &mut world,
            root,
            1,
            InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0)),
        )
        .unwrap();
        world.get_mut::<InfluenceArea>(owner).unwrap().size = [200.0; 2];
        world.run_system_cached(sync_geometry).unwrap();
        assert_eq!(world.get::<InfluenceArea>(owner).unwrap().depth, 200.0);
        world
            .get_mut::<crate::topology::Spatial>(owner)
            .unwrap()
            .depth = Some(35.0);
        world.get_mut::<InfluenceArea>(owner).unwrap().size = [300.0; 2];
        world.get_mut::<CanvasItem>(owner).unwrap().size = Vec2::splat(300.0);
        world.run_system_cached(sync_geometry).unwrap();
        assert_eq!(world.get::<InfluenceArea>(owner).unwrap().depth, 35.0);
        world
            .get_mut::<crate::topology::Spatial>(owner)
            .unwrap()
            .depth = None;
        world.run_system_cached(sync_geometry).unwrap();
        assert_eq!(world.get::<InfluenceArea>(owner).unwrap().depth, 300.0);
        let snapshot = SavedArea {
            placement: crate::sand_placement::Placement::capture(&world, owner),
            workspace: 1,
            area: world.get::<InfluenceArea>(owner).unwrap().clone(),
        };
        let snapshot: SavedArea =
            serde_json::from_value(serde_json::to_value(snapshot).unwrap()).unwrap();
        assert_eq!(snapshot.area.depth, 300.0);
        assert_eq!(snapshot.placement.spatial.depth, None);
    }
    fn area() -> InfluenceArea {
        let mut area = InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(200.0));
        area.strength = 100.0;
        area.rules = vec![PropertyRule {
            property: Property::Quantity,
            value: "-3".into(),
        }];
        area
    }

    #[cfg_attr(test, test)]
    fn new_shapes_have_no_force_or_record_changes_until_configured() {
        for shape in [
            AreaShape::Square,
            AreaShape::Circle,
            AreaShape::Polygon(vec![[-0.5, -0.5], [0.5, -0.5], [0.0, 0.5], [-0.5, -0.5]]),
        ] {
            let mut area = InfluenceArea::new(shape, DVec2::ZERO, DVec2::splat(200.0));
            assert!(area.validate());
            assert_eq!(area.strength, 0.0);
            assert!(area.rules.is_empty());
            assert_eq!(area.changes, Default::default());
            let record = RecordProperties(json!({"quantity": -3}));
            let point = DVec2::new(10.0, 0.0);
            assert_eq!(area.force(point, &record), DVec2::ZERO);
            area.rules.push(PropertyRule {
                property: Property::Quantity,
                value: "-3".into(),
            });
            assert_eq!(area.force(point, &record), DVec2::ZERO);
            area.strength = 100.0;
            assert!(area.force(point, &record).x < 0.0);
            let restored: InfluenceArea =
                serde_json::from_value(serde_json::to_value(&area).unwrap()).unwrap();
            assert_eq!(restored, area);
        }
    }

    #[cfg_attr(test, test)]
    fn perimeters_reject_open_crossed_repeated_flat_and_oversized_shapes() {
        let points = vec![
            [-0.5, -0.5],
            [0.5, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
            [-0.5, -0.5],
        ];
        assert!(valid_polygon(&points));
        assert!(!valid_polygon(&points[..4]));
        let mut crossed = points.clone();
        crossed.swap(1, 2);
        assert!(!valid_polygon(&crossed));
        let mut repeated = points.clone();
        repeated[2] = repeated[1];
        assert!(!valid_polygon(&repeated));
        assert!(!valid_polygon(&[
            [-0.5, 0.0],
            [0.0, 0.0],
            [0.5, 0.0],
            [-0.5, 0.0]
        ]));
        let mut outside = points;
        outside[1][0] = 2.0;
        assert!(!valid_polygon(&outside));
        assert!(!valid_polygon(&vec![[0.0; 2]; MAX_POINTS + 1]));
        let mut invalid = area();
        invalid.strength = f64::NAN;
        assert!(!invalid.validate());
        invalid.strength = -1.0;
        assert!(!invalid.validate());
        invalid.strength = 100.0;
        invalid.size[0] = 100.0;
        assert!(!invalid.validate());
        invalid.size = [f64::INFINITY; 2];
        assert!(!invalid.validate());
    }

    #[cfg_attr(test, test)]
    fn concave_outline_matches_its_interior_including_edges_at_large_coordinates() {
        let offset = DVec2::new(1e9, -1e9);
        let points: Vec<_> = [
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 40.0],
            [40.0, 40.0],
            [40.0, 100.0],
            [0.0, 100.0],
        ]
        .into_iter()
        .map(|point| offset + DVec2::from_array(point))
        .collect();
        let area = InfluenceArea::drawn(&points).unwrap();
        for point in [[0.0, 0.0], [20.0, 80.0], [80.0, 20.0], [40.0, 40.0]] {
            assert!(area.contains(offset + DVec2::from_array(point)));
        }
        assert!(!area.contains(offset + DVec2::splat(80.0)));
        assert!(!area.contains(DVec2::NAN));
        assert_eq!(area.outline().first(), area.outline().last());
    }

    #[cfg_attr(test, test)]
    fn properties_and_direction_produce_finite_constant_forces_only_inside() {
        let record =
            RecordProperties(json!({"uid":"r_test", "head":"Work", "quantity":-3, "slug":null}));
        let mut area = area();
        assert_eq!(
            area.force(DVec2::new(50.0, 0.0), &record),
            DVec2::new(-100.0, 0.0)
        );
        assert_eq!(
            area.force(DVec2::new(100.0, 0.0), &record),
            DVec2::new(-100.0, 0.0)
        );
        assert_eq!(area.force(DVec2::new(100.001, 0.0), &record), DVec2::ZERO);
        assert_eq!(area.force(DVec2::splat(80.0), &record), DVec2::ZERO);
        assert_eq!(area.force(DVec2::ZERO, &record), DVec2::ZERO);
        area.direction = Direction::Repel;
        assert_eq!(
            area.force(DVec2::new(50.0, 0.0), &record),
            DVec2::new(100.0, 0.0)
        );
        area.rules.push(PropertyRule {
            property: Property::Slug,
            value: "null".into(),
        });
        assert_eq!(area.force(DVec2::X, &record), DVec2::ZERO);
        area.match_all = false;
        assert_eq!(area.force(DVec2::X, &record), DVec2::new(100.0, 0.0));
        area.rules[0].value = "-2".into();
        assert_eq!(area.force(DVec2::X, &record), DVec2::ZERO);
        area.rules.clear();
        assert_eq!(area.force(DVec2::X, &record), DVec2::ZERO);
    }

    #[cfg_attr(test, test)]
    fn reach_expands_the_perimeter_without_changing_local_membership() {
        let mut area = area();
        let record = RecordProperties(json!({"quantity": -3}));
        let outside = DVec2::new(125.0, 0.0);
        assert!(!area.reaches(outside));
        area.reach.radius = 25.0;
        assert!(area.reaches(outside));
        assert!(!area.contains(outside));
        assert_eq!(area.force(outside, &record), DVec2::new(-100.0, 0.0));
        assert!(!area.reaches(DVec2::new(125.001, 0.0)));
        assert!(!area.reaches(DVec2::splat(100.0)));
        area.reach.shape = ReachShape::Square;
        assert!(area.reaches(DVec2::splat(125.0)));
        assert!(!area.contains(DVec2::splat(125.0)));
        area.reach.shape = ReachShape::FollowShape;
        area.shape = AreaShape::Square;
        assert!(area.reaches(DVec2::new(115.0, 120.0)));
        assert!(!area.reaches(DVec2::new(115.001, 120.0)));
        area.reach.mode = ReachMode::Unlimited;
        assert!(area.reaches(DVec2::new(1e8, -1e8)));
        assert!(!area.reaches(DVec2::NAN));
        assert_eq!(
            area.force(outside, &RecordProperties(json!({"quantity": -2}))),
            DVec2::ZERO
        );
        let restored: InfluenceArea =
            serde_json::from_value(serde_json::to_value(&area).unwrap()).unwrap();
        assert_eq!(restored, area);
        area.reach.mode = ReachMode::Limited;
        assert_eq!(area.reach.radius, 25.0);
        for invalid in [-1.0, f64::NAN, f64::INFINITY, 100_001.0] {
            area.reach.radius = invalid;
            assert!(!area.validate());
        }
    }

    #[cfg_attr(test, test)]
    fn follow_shape_preserves_concavities_and_radius_when_moved_or_resized() {
        let mut area = InfluenceArea::drawn(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 0.0),
            DVec2::new(200.0, 40.0),
            DVec2::new(40.0, 40.0),
            DVec2::new(40.0, 100.0),
            DVec2::new(0.0, 100.0),
        ])
        .unwrap();
        area.reach.radius = 10.0;
        assert!(area.reaches(DVec2::new(120.0, 50.0)));
        assert!(!area.reaches(DVec2::new(120.0, 50.01)));
        assert!(!area.reaches(DVec2::new(80.0, 80.0)));
        let offset = DVec2::new(1e9, -1e9);
        area.center = (DVec2::from_array(area.center) + offset).to_array();
        assert!(area.reaches(offset + DVec2::new(120.0, 50.0)));
        assert!(!area.reaches(offset + DVec2::new(80.0, 80.0)));
        area.reach.shape = ReachShape::Square;
        assert!(area.reaches(offset + DVec2::new(210.0, 160.0)));
        assert!(!area.reaches(offset + DVec2::new(210.01, 160.0)));
        area.size = [400.0, 200.0];
        assert_eq!(area.reach.radius, 10.0);
        assert!(area.reaches(DVec2::from_array(area.center) + DVec2::splat(210.0)));
    }

    #[cfg_attr(test, test)]
    fn unlimited_reach_updates_offscreen_sands_and_keeps_workspace_and_pin_limits() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_systems(Update, forces);
        let root = app.world_mut().spawn(Workspaces::default()).id();
        let other = app.world_mut().spawn(Workspaces::default()).id();
        let source = spawn_area(app.world_mut(), root, 1, area()).unwrap();
        let mut sands = Vec::new();
        for (parent, workspace) in [(root, 1), (root, 2), (other, 1), (root, 1)] {
            sands.push(
                app.world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::new(20_000.0, 0.0),
                            size: Vec2::splat(20.0),
                        },
                        RecordProperties(json!({"quantity": -3})),
                        ChildOf(parent),
                        WorkspaceMember(workspace),
                    ))
                    .id(),
            );
        }
        app.world_mut().entity_mut(sands[3]).insert(Pinned {
            anchor: [0.5; 2],
            scale: 1.0,
        });
        app.update();
        assert!(
            sands
                .iter()
                .all(|sand| app.world().get::<AreaForces>(*sand).unwrap().0.is_empty())
        );
        app.world_mut()
            .get_mut::<InfluenceArea>(source)
            .unwrap()
            .reach
            .mode = ReachMode::Unlimited;
        app.update();
        assert_eq!(
            app.world().get::<AreaForces>(sands[0]).unwrap().total(),
            DVec2::new(-100.0, 0.0)
        );
        assert!(
            sands[1..].iter().all(|sand| app
                .world()
                .get::<AreaForces>(*sand)
                .unwrap()
                .0
                .is_empty())
        );
        app.update();
        assert!(
            !app.world()
                .entity(sands[0])
                .get_ref::<AreaForces>()
                .unwrap()
                .is_changed()
        );
        app.world_mut()
            .get_mut::<InfluenceArea>(source)
            .unwrap()
            .reach
            .mode = ReachMode::Limited;
        app.update();
        assert!(
            app.world()
                .get::<AreaForces>(sands[0])
                .unwrap()
                .0
                .is_empty()
        );
        app.world_mut()
            .get_mut::<InfluenceArea>(source)
            .unwrap()
            .reach
            .radius = 20_000.0;
        app.update();
        assert!(
            !app.world()
                .get::<AreaForces>(sands[0])
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[cfg_attr(test, test)]
    fn contributions_sum_without_motion_and_clear_on_data_pin_workspace_or_area_changes() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_systems(Update, forces);
        let root = app.world_mut().spawn(Workspaces::default()).id();
        let other_root = app.world_mut().spawn(Workspaces::default()).id();
        let first = spawn_area(app.world_mut(), root, 1, area()).unwrap();
        let mut second_area = area();
        second_area.direction = Direction::Repel;
        second_area.strength = 40.0;
        let second = spawn_area(app.world_mut(), root, 1, second_area).unwrap();
        spawn_area(app.world_mut(), root, 2, area()).unwrap();
        spawn_area(app.world_mut(), other_root, 1, area()).unwrap();
        let position = DVec2::new(50.0, 0.0);
        let sand = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position,
                    size: Vec2::splat(20.0),
                },
                RecordProperties(json!({"quantity":-3})),
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id();
        app.update();
        let forces = app.world().get::<AreaForces>(sand).unwrap();
        assert_eq!(forces.0.len(), 2);
        assert_eq!(forces.total(), DVec2::new(-60.0, 0.0));
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().position,
            position
        );
        app.update();
        assert!(
            !app.world()
                .entity(sand)
                .get_ref::<AreaForces>()
                .unwrap()
                .is_changed()
        );
        app.world_mut().entity_mut(sand).insert(Pinned {
            anchor: [0.5; 2],
            scale: 1.0,
        });
        app.update();
        assert_eq!(
            app.world().get::<AreaForces>(sand).unwrap().total(),
            DVec2::ZERO
        );
        app.world_mut().entity_mut(sand).remove::<Pinned>();
        app.update();
        assert_eq!(app.world().get::<AreaForces>(sand).unwrap().0.len(), 2);
        app.world_mut()
            .entity_mut(sand)
            .insert(RecordProperties(json!({"quantity":-2})));
        app.update();
        assert_eq!(
            app.world().get::<AreaForces>(sand).unwrap().total(),
            DVec2::ZERO
        );
        app.world_mut()
            .entity_mut(sand)
            .insert(RecordProperties(json!({"quantity":-3})));
        app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 2;
        app.update();
        assert_eq!(
            app.world().get::<AreaForces>(sand).unwrap().total(),
            DVec2::ZERO
        );
        app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 1;
        app.world_mut().despawn(first);
        app.update();
        assert_eq!(
            app.world().get::<AreaForces>(sand).unwrap().total(),
            DVec2::new(40.0, 0.0)
        );
        app.world_mut().despawn(second);
        app.update();
        assert!(app.world().get::<AreaForces>(sand).unwrap().0.is_empty());
        app.world_mut()
            .entity_mut(sand)
            .remove::<RecordProperties>();
        app.update();
        assert!(app.world().get::<AreaForces>(sand).is_none());
    }

    crate::laboratory_cases! {
        area_depth_tracks_resizing_until_manually_fixed_and_can_return_to_automatic,
        targets_stay_separate_from_boundaries_and_depth_survives_shape_edits,
        reach_expands_the_perimeter_without_changing_local_membership,
        follow_shape_preserves_concavities_and_radius_when_moved_or_resized,
        unlimited_reach_updates_offscreen_sands_and_keeps_workspace_and_pin_limits,
        new_shapes_have_no_force_or_record_changes_until_configured,
        perimeters_reject_open_crossed_repeated_flat_and_oversized_shapes,
        concave_outline_matches_its_interior_including_edges_at_large_coordinates,
        properties_and_direction_produce_finite_constant_forces_only_inside,
        contributions_sum_without_motion_and_clear_on_data_pin_workspace_or_area_changes,
    }

    #[cfg_attr(test, test)]
    fn targets_stay_separate_from_boundaries_and_depth_survives_shape_edits() {
        let mut area = InfluenceArea::drawn(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 0.0),
            DVec2::new(200.0, 40.0),
            DVec2::new(40.0, 40.0),
            DVec2::new(40.0, 100.0),
            DVec2::new(0.0, 100.0),
        ])
        .unwrap();
        assert_eq!(area.depth, 100.0);
        assert!(!area.contains(DVec2::from_array(area.center)));
        assert!(area.contains(area.target_position()));
        let boundary = area.outline();
        area.target = AttractionTarget::Point([400.0, 0.0]);
        area.strength = 10.0;
        let point = DVec2::new(20.0, 20.0);
        assert!(!area.reaches(area.target_position()));
        assert_eq!(area.outline(), boundary);
        assert!(
            area.force_for_match(point, true)
                .dot(area.target_position() - point)
                > 0.0
        );
        area.direction = Direction::Repel;
        assert!(
            area.force_for_match(point, true)
                .dot(area.target_position() - point)
                < 0.0
        );
        assert_eq!(
            area.force_for_match(DVec2::new(80.0, 80.0), true),
            DVec2::ZERO
        );
        let before = area.target_position();
        area.center[0] += 25.0;
        assert_eq!(area.target_position(), before + DVec2::new(25.0, 0.0));
        area.size = [400.0, 200.0];
        assert_eq!(area.depth, 100.0);
        assert_eq!(area.target_position(), before + DVec2::new(25.0, 0.0));
        area.target = AttractionTarget::Center;
        assert!(area.contains(area.target_position()));
        for depth in [0.0, -1.0, f64::NAN, f64::INFINITY, 100001.0] {
            area.depth = depth;
            assert!(!area.validate());
        }
        area.depth = 42.0;
        for offset in [[f64::NAN, 0.0], [0.0, f64::INFINITY]] {
            area.target = AttractionTarget::Point(offset);
            assert!(!area.validate());
        }
    }
}
