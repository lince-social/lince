use crate::{
    canvas::CanvasItem,
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
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
    pub shape: AreaShape,
    pub direction: Direction,
    pub strength: f64,
    pub rules: Vec<PropertyRule>,
    pub match_all: bool,
    #[serde(default)]
    pub changes: crate::area_mutation::AreaChanges,
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
            shape,
            direction: Direction::Attract,
            strength: 100.0,
            rules: Vec::new(),
            match_all: true,
            changes: Default::default(),
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
            && (0.0..=1_000_000.0).contains(&self.strength)
            && self.rules.len() <= MAX_RULES
            && self.rules.iter().all(PropertyRule::validate)
            && self.changes.validate()
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
        if !self.contains(point) || !self.matches(record) {
            return DVec2::ZERO;
        }
        let delta = DVec2::from_array(self.center) - point;
        let direction = delta.try_normalize().unwrap_or(DVec2::ZERO);
        direction
            * self.strength
            * if self.direction == Direction::Attract {
                1.0
            } else {
                -1.0
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
    Some(
        world
            .spawn((
                CanvasItem {
                    position: DVec2::from_array(area.center),
                    size: DVec2::from_array(area.size).as_vec2(),
                },
                area,
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

fn sync_geometry(mut areas: Query<(&InfluenceArea, &mut CanvasItem)>) {
    for (area, mut item) in &mut areas {
        let position = DVec2::from_array(area.center);
        let size = DVec2::from_array(area.size).as_vec2();
        if item.position != position || item.size != size {
            item.position = position;
            item.size = size;
        }
    }
}

fn forces(
    areas: Query<(
        Entity,
        Ref<InfluenceArea>,
        Ref<ChildOf>,
        Ref<WorkspaceMember>,
    )>,
    records: Query<(
        Entity,
        Ref<CanvasItem>,
        Ref<RecordProperties>,
        Ref<ChildOf>,
        Ref<WorkspaceMember>,
        Option<Ref<Pinned>>,
    )>,
    roots: Query<Ref<Workspaces>>,
    previous: Query<(Entity, &AreaForces)>,
    mut removed_areas: RemovedComponents<InfluenceArea>,
    mut removed_records: RemovedComponents<RecordProperties>,
    mut removed_pins: RemovedComponents<Pinned>,
    mut removed_items: RemovedComponents<CanvasItem>,
    mut removed_members: RemovedComponents<WorkspaceMember>,
    mut commands: Commands,
) {
    let removed = removed_areas.read().count()
        + removed_records.read().count()
        + removed_pins.read().count()
        + removed_items.read().count()
        + removed_members.read().count();
    if removed == 0
        && !areas.iter().any(|(_, area, parent, member)| {
            area.is_changed() || parent.is_changed() || member.is_changed()
        })
        && !records
            .iter()
            .any(|(_, item, record, parent, member, pin)| {
                item.is_changed()
                    || record.is_changed()
                    || parent.is_changed()
                    || member.is_changed()
                    || pin.is_some_and(|pin| pin.is_changed())
            })
        && !roots.iter().any(|root| root.is_changed())
    {
        return;
    }
    let mut valid: Vec<_> = areas
        .iter()
        .filter(|(_, area, _, _)| area.validate())
        .collect();
    valid.sort_by(|a, b| a.1.id.cmp(&b.1.id).then(a.0.cmp(&b.0)));
    for (entity, item, record, parent, member, pin) in &records {
        let mut next = AreaForces::default();
        if pin.is_none()
            && roots
                .get(parent.parent())
                .is_ok_and(|spaces| spaces.active == member.0)
        {
            for (source, area, area_parent, area_member) in &valid {
                if area_parent.parent() != parent.parent() || area_member.0 != member.0 {
                    continue;
                }
                let force = area.force(item.position, &record);
                if force != DVec2::ZERO && force.is_finite() {
                    next.0.push(AreaForce {
                        area: *source,
                        force,
                    });
                }
            }
        }
        if previous
            .get(entity)
            .is_ok_and(|(_, previous)| *previous == next)
        {
            continue;
        }
        commands.entity(entity).insert(next);
    }
    for (entity, _) in &previous {
        if !records.contains(entity) {
            commands.entity(entity).remove::<AreaForces>();
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    fn area() -> InfluenceArea {
        let mut area = InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(200.0));
        area.rules = vec![PropertyRule {
            property: Property::Quantity,
            value: "-3".into(),
        }];
        area
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
        perimeters_reject_open_crossed_repeated_flat_and_oversized_shapes,
        concave_outline_matches_its_interior_including_edges_at_large_coordinates,
        properties_and_direction_produce_finite_constant_forces_only_inside,
        contributions_sum_without_motion_and_clear_on_data_pin_workspace_or_area_changes,
    }
}
