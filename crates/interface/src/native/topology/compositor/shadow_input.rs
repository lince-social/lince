use bevy::{
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};

pub(super) struct ShapeInput {
    size: [u32; 2],
    radius: u32,
    positions: Vec<[u32; 3]>,
    uvs: Vec<[u32; 2]>,
    indices: Option<Indices>,
}

impl ShapeInput {
    pub(super) fn capture(mesh: &Mesh, size: Vec2, radius: f32) -> Self {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!("accepted shadow positions")
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            unreachable!("accepted shadow UVs")
        };
        Self {
            size: size.to_array().map(f32::to_bits),
            radius: radius.to_bits(),
            positions: positions
                .iter()
                .map(|point| point.map(f32::to_bits))
                .collect(),
            uvs: uvs.iter().map(|uv| uv.map(f32::to_bits)).collect(),
            indices: mesh.indices().cloned(),
        }
    }

    pub(super) fn matches(&self, mesh: &Mesh, size: Vec2, radius: f32) -> bool {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList
            || self.size != size.to_array().map(f32::to_bits)
            || self.radius != radius.to_bits()
            || self.indices.as_ref() != mesh.indices()
        {
            return false;
        }
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return false;
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return false;
        };
        self.positions.len() == positions.len()
            && self.uvs.len() == uvs.len()
            && self
                .positions
                .iter()
                .zip(positions)
                .all(|(old, point)| *old == point.map(f32::to_bits))
            && self
                .uvs
                .iter()
                .zip(uvs)
                .all(|(old, uv)| *old == uv.map(f32::to_bits))
    }
}
