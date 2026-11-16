use super::*;
use bevy::{
    asset::RenderAssetUsages,
    math::DVec2,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
};
use std::borrow::Cow;

pub(super) struct Geometry {
    original: Handle<Mesh>,
    owned: Option<(Handle<Mesh>, Handle<Mesh>, Inputs<'static>)>,
}

#[derive(PartialEq)]
struct Inputs<'a> {
    positions: Cow<'a, [[f32; 3]]>,
    uvs: Cow<'a, [[f32; 2]]>,
    indices: Option<Cow<'a, Indices>>,
    size: Vec2,
    radii: [f32; 4],
    clip: Rect,
}

impl Inputs<'_> {
    fn into_owned(self) -> Inputs<'static> {
        Inputs {
            positions: Cow::Owned(self.positions.into_owned()),
            uvs: Cow::Owned(self.uvs.into_owned()),
            indices: self.indices.map(|indices| Cow::Owned(indices.into_owned())),
            size: self.size,
            radii: self.radii,
            clip: self.clip,
        }
    }
}

#[derive(Default)]
struct Vertices {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

impl Vertices {
    fn triangle(&mut self, points: [Vec3; 3], normal: Vec3, uvs: [Vec2; 3]) {
        self.positions.extend(points.map(|point| point.to_array()));
        self.normals.extend([normal.to_array(); 3]);
        self.uvs.extend(uvs.map(|point| point.to_array()));
    }

    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
    }
}

fn outline(size: Vec2, radii: [f32; 4]) -> Vec<DVec2> {
    let mut polygon = Vec::new();
    for (index, radius) in radii.into_iter().enumerate() {
        let radius = (Vec2::splat(radius.clamp(0.0, size.min_element() * 0.5)) / size).as_dvec2();
        let center = match index {
            0 => radius,
            1 => DVec2::new(1.0 - radius.x, radius.y),
            2 => DVec2::ONE - radius,
            _ => DVec2::new(radius.x, 1.0 - radius.y),
        };
        let start = (index as f64 + 2.0) * std::f64::consts::FRAC_PI_2;
        for step in 0..=8 {
            let angle = start + step as f64 * std::f64::consts::FRAC_PI_2 / 8.0;
            let point = center + DVec2::new(angle.cos(), angle.sin()) * radius;
            if polygon
                .last()
                .is_none_or(|previous: &DVec2| previous.distance_squared(point) > 1e-18)
            {
                polygon.push(point);
            }
        }
    }
    polygon
}

fn inputs(world: &World, owner: Entity, face: Entity) -> Option<Inputs<'_>> {
    let node = world.get::<ComputedNode>(owner)?;
    let size = node.size();
    if size.min_element() <= 0.0 || !size.is_finite() {
        return None;
    }
    let radii = node.border_radius();
    let radii = [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ];
    if radii.iter().any(|radius| !radius.is_finite()) || radii.iter().all(|radius| *radius <= 0.0) {
        return None;
    }
    let mesh = world
        .resource::<Assets<Mesh>>()
        .get(&world.get::<Mesh3d>(face)?.0)?;
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    if positions.iter().any(|position| position[2].abs() > 1e-6) {
        return None;
    }
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)? else {
        return None;
    };
    if positions.len() != uvs.len() {
        return None;
    }
    let clip = world.get::<super::super::presentation::Surface>(owner)?.uv;
    if clip.size().min_element() <= 0.0 {
        return None;
    }
    Some(Inputs {
        positions: Cow::Borrowed(positions),
        uvs: Cow::Borrowed(uvs),
        indices: mesh.indices().map(Cow::Borrowed),
        size,
        radii,
        clip,
    })
}

fn plate(input: &Inputs<'_>, outline: &[DVec2]) -> Mesh {
    let mut vertices = Vertices::default();
    let indices: Vec<_> = input
        .indices
        .as_ref()
        .map(|indices| indices.iter().collect())
        .unwrap_or_else(|| (0..input.positions.len()).collect());
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
        if [a, b, c].iter().any(|index| *index >= input.uvs.len()) {
            continue;
        }
        let [ua, ub, uc] = [
            Vec2::from_array(input.uvs[a]).as_dvec2(),
            Vec2::from_array(input.uvs[b]).as_dvec2(),
            Vec2::from_array(input.uvs[c]).as_dvec2(),
        ];
        let determinant = (ub - ua).perp_dot(uc - ua);
        if determinant.abs() < 1e-15 {
            continue;
        }
        let polygon = super::super::holes::intersect(&[ua, ub, uc], outline);
        let [pa, pb, pc] = [a, b, c].map(|index| Vec3::from_array(input.positions[index]));
        let position = |point: DVec2| {
            let b_weight = (point - ua).perp_dot(uc - ua) / determinant;
            let c_weight = (ub - ua).perp_dot(point - ua) / determinant;
            pa + (pb - pa) * b_weight as f32 + (pc - pa) * c_weight as f32
        };
        for index in 1..polygon.len().saturating_sub(1) {
            let uv = [polygon[0], polygon[index], polygon[index + 1]];
            vertices.triangle(uv.map(position), Vec3::Z, uv.map(|point| point.as_vec2()));
        }
    }
    vertices.mesh()
}

fn body(input: &Inputs<'_>, outline: &[DVec2]) -> Mesh {
    let clip = input.clip;
    let bounds = [
        clip.min.as_dvec2(),
        DVec2::new(clip.max.x as f64, clip.min.y as f64),
        clip.max.as_dvec2(),
        DVec2::new(clip.min.x as f64, clip.max.y as f64),
    ];
    let polygon = super::super::holes::intersect(outline, &bounds);
    let point = |uv: DVec2, y: f32| {
        let local = (uv.as_vec2() - clip.min) / clip.size() - Vec2::splat(0.5);
        Vec3::new(local.x, y, local.y)
    };
    let mut vertices = Vertices::default();
    for index in 1..polygon.len().saturating_sub(1) {
        let top = [polygon[0], polygon[index + 1], polygon[index]];
        let bottom = [polygon[0], polygon[index], polygon[index + 1]];
        vertices.triangle(
            top.map(|uv| point(uv, 0.5)),
            Vec3::Y,
            top.map(|point| point.as_vec2()),
        );
        vertices.triangle(
            bottom.map(|uv| point(uv, -0.5)),
            -Vec3::Y,
            bottom.map(|point| point.as_vec2()),
        );
    }
    for (index, (a, b)) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .enumerate()
    {
        let [a, b] = [point(*a, 0.5), point(*b, 0.5)];
        let edge = b - a;
        if edge.length_squared() < 1e-12 {
            continue;
        }
        let normal = Vec3::new(edge.z, 0.0, -edge.x).normalize();
        let [low_a, low_b] = [Vec3::new(a.x, -0.5, a.z), Vec3::new(b.x, -0.5, b.z)];
        let [u, v] = [
            index as f32 / polygon.len() as f32,
            (index + 1) as f32 / polygon.len() as f32,
        ];
        vertices.triangle(
            [a, b, low_b],
            normal,
            [Vec2::new(u, 0.0), Vec2::new(v, 0.0), Vec2::new(v, 1.0)],
        );
        vertices.triangle(
            [a, low_b, low_a],
            normal,
            [Vec2::new(u, 0.0), Vec2::new(v, 1.0), Vec2::new(u, 1.0)],
        );
    }
    vertices.mesh()
}

impl Geometry {
    pub(super) fn new(world: &World, body: Entity) -> Self {
        Self {
            original: world.get::<Mesh3d>(body).unwrap().0.clone(),
            owned: None,
        }
    }

    pub(super) fn release(&mut self, world: &mut World, body: Entity) {
        if let Some((plate, mesh, _)) = self.owned.take() {
            if let Ok(mut entity) = world.get_entity_mut(body) {
                entity.insert(Mesh3d(self.original.clone()));
            }
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            meshes.remove(plate.id());
            meshes.remove(mesh.id());
        }
    }

    pub(super) fn update(
        &mut self,
        world: &mut World,
        owner: Entity,
        face: Entity,
        body_entity: Entity,
        plate_entity: Entity,
    ) {
        let Some(input) = inputs(world, owner, face) else {
            self.release(world, body_entity);
            let mesh = world.get::<Mesh3d>(face).unwrap().clone();
            if world.get::<Mesh3d>(plate_entity).unwrap().0 != mesh.0 {
                world.entity_mut(plate_entity).insert(mesh);
            }
            return;
        };
        if self
            .owned
            .as_ref()
            .is_none_or(|(_, _, previous)| previous != &input)
        {
            let outline = outline(input.size, input.radii);
            let plate_mesh = plate(&input, &outline);
            let body_mesh = body(&input, &outline);
            let input = input.into_owned();
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            if let Some((plate_handle, body_handle, previous)) = &mut self.owned {
                meshes.insert(plate_handle.id(), plate_mesh).unwrap();
                meshes.insert(body_handle.id(), body_mesh).unwrap();
                *previous = input;
            } else {
                self.owned = Some((meshes.add(plate_mesh), meshes.add(body_mesh), input));
            }
        }
        let (plate, body, _) = self.owned.as_ref().unwrap();
        if world.get::<Mesh3d>(plate_entity).unwrap().0 != *plate {
            world.entity_mut(plate_entity).insert(Mesh3d(plate.clone()));
        }
        if world.get::<Mesh3d>(body_entity).unwrap().0 != *body {
            world.entity_mut(body_entity).insert(Mesh3d(body.clone()));
        }
    }
}
