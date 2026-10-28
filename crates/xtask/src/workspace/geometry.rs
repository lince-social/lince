use crate::Result;
use serde_json::{Value, json};
use std::borrow::Cow;

pub(super) struct Glb {
    document: Value,
    binary: Vec<u8>,
}

impl Glb {
    pub(super) fn new() -> Self {
        Self {
            document: json!({
                "asset":{"version":"2.0","generator":"Lince Stage 01 bounded fixture generator","copyright":"2026 Lince; MIT"},
                "scene":0,
                "scenes":[{"nodes":[]}],
                "nodes":[],
                "meshes":[],
                "materials":[],
                "accessors":[],
                "bufferViews":[],
                "buffers":[{"byteLength":0}]
            }),
            binary: Vec::new(),
        }
    }

    pub(super) fn from_parts(document: Value, binary: Vec<u8>) -> Self {
        Self { document, binary }
    }

    pub(super) fn document(&self) -> &Value {
        &self.document
    }

    pub(super) fn attach(&mut self, parent: usize, child: usize) {
        let node = &mut self.document["nodes"][parent];
        if node.get("children").is_none() {
            node["children"] = json!([]);
        }
        node["children"].as_array_mut().unwrap().push(json!(child));
    }

    pub(super) fn array(&mut self, key: &str, value: Value) -> usize {
        let entries = self.document[key].as_array_mut().unwrap();
        let index = entries.len();
        entries.push(value);
        index
    }

    pub(super) fn accessor(
        &mut self,
        data: &[u8],
        count: usize,
        component: u32,
        kind: &str,
        bounds: Option<([f32; 3], [f32; 3])>,
    ) -> usize {
        self.binary.resize(self.binary.len().next_multiple_of(4), 0);
        let offset = self.binary.len();
        self.binary.extend_from_slice(data);
        let view = self.array(
            "bufferViews",
            json!({
                "buffer":0,"byteOffset":offset,"byteLength":data.len()
            }),
        );
        let mut descriptor = json!({
            "bufferView":view,"byteOffset":0,"componentType":component,"count":count,"type":kind
        });
        if let Some((minimum, maximum)) = bounds {
            descriptor["min"] = json!(minimum);
            descriptor["max"] = json!(maximum);
        }
        self.array("accessors", descriptor)
    }

    pub(super) fn mesh(
        &mut self,
        name: &str,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
    ) -> usize {
        let encode = |values: &[[f32; 3]]| {
            values
                .iter()
                .flatten()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>()
        };
        let minimum = std::array::from_fn(|axis| {
            positions
                .iter()
                .map(|p| p[axis])
                .fold(f32::INFINITY, f32::min)
        });
        let maximum = std::array::from_fn(|axis| {
            positions
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        let position = self.accessor(
            &encode(positions),
            positions.len(),
            5126,
            "VEC3",
            Some((minimum, maximum)),
        );
        let normal = self.accessor(&encode(normals), normals.len(), 5126, "VEC3", None);
        let index = self.accessor(
            &indices
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
            indices.len(),
            5125,
            "SCALAR",
            None,
        );
        self.array("meshes", json!({
            "name":name,"primitives":[{"attributes":{"POSITION":position,"NORMAL":normal},"indices":index,"mode":4}]
        }))
    }

    pub(super) fn cube(&mut self) -> usize {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        for (normal, right, up) in [
            ([0., 0., 1.], [1., 0., 0.], [0., 1., 0.]),
            ([1., 0., 0.], [0., 0., -1.], [0., 1., 0.]),
            ([0., 0., -1.], [-1., 0., 0.], [0., 1., 0.]),
            ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
            ([0., 1., 0.], [1., 0., 0.], [0., 0., -1.]),
            ([0., -1., 0.], [1., 0., 0.], [0., 0., 1.]),
        ] {
            let base = positions.len() as u32;
            for (x, y) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
                positions.push(std::array::from_fn(|axis| {
                    (normal[axis] + right[axis] * x + up[axis] * y) * 0.5
                }));
                normals.push(normal);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        self.mesh("shared-unit-cube", &positions, &normals, &indices)
    }

    pub(super) fn sphere(&mut self, latitude: usize, longitude: usize) -> usize {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for row in 0..=latitude {
            let theta = std::f32::consts::PI * row as f32 / latitude as f32;
            for column in 0..=longitude {
                let phi = std::f32::consts::TAU * column as f32 / longitude as f32;
                positions.push([
                    theta.sin() * phi.cos(),
                    theta.cos(),
                    theta.sin() * phi.sin(),
                ]);
            }
        }
        for row in 0..latitude {
            for column in 0..longitude {
                let a = (row * (longitude + 1) + column) as u32;
                let b = a + 1;
                let c = a + (longitude + 1) as u32;
                let d = c + 1;
                if row != 0 {
                    indices.extend([a, b, c]);
                }
                if row + 1 != latitude {
                    indices.extend([b, d, c]);
                }
            }
        }
        self.mesh("shared-unit-sphere", &positions, &positions, &indices)
    }

    pub(super) fn material_mesh(&mut self, mesh: usize, name: &str, color: [f32; 4]) -> usize {
        let material = self.array("materials", json!({
            "name":name,"pbrMetallicRoughness":{"baseColorFactor":color,"metallicFactor":0.0,"roughnessFactor":0.85}
        }));
        let mut result = self.document["meshes"][mesh].clone();
        result["name"] = json!(name);
        result["primitives"][0]["material"] = json!(material);
        self.array("meshes", result)
    }

    pub(super) fn node(
        &mut self,
        id: &str,
        mesh: usize,
        position: [f64; 3],
        size: [f64; 3],
    ) -> usize {
        self.array("nodes", json!({
            "name":id,"mesh":mesh,"translation":position,"scale":size,
            "extras":{"fixture_identity":id,"units":"metre","source_status":"authored-synthetic"}
        }))
    }

    pub(super) fn group(&mut self, id: &str, children: Vec<usize>, position: [f64; 3]) -> usize {
        self.array(
            "nodes",
            json!({
                "name":id,"children":children,"translation":position,
                "extras":{"fixture_identity":id,"units":"metre"}
            }),
        )
    }

    pub(super) fn scene(&mut self, name: &str, roots: &[usize]) {
        self.array("scenes", json!({"name":name,"nodes":roots}));
    }

    pub(super) fn finish(mut self, roots: &[usize]) -> Result<Vec<u8>> {
        self.document["scenes"][0]["nodes"] = json!(roots);
        self.document["buffers"][0]["byteLength"] = json!(self.binary.len());
        let bytes = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&self.document).map_err(|e| e.to_string())?),
            bin: Some(Cow::Owned(self.binary)),
        }
        .to_vec()
        .map_err(|e| e.to_string())?;
        gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

pub(super) fn local_place() -> Result<Vec<u8>> {
    let mut glb = Glb::new();
    let cube = glb.cube();
    let sphere = glb.sphere(12, 24);
    let wall = glb.material_mesh(cube, "warm-stone", [0.68, 0.63, 0.52, 1.]);
    let floor = glb.material_mesh(cube, "floor", [0.32, 0.25, 0.19, 1.]);
    let roof = glb.material_mesh(cube, "roof", [0.18, 0.25, 0.30, 1.]);
    let panel = glb.material_mesh(cube, "mounted-content-proxy", [0.17, 0.44, 0.37, 1.]);
    let ball = glb.material_mesh(sphere, "40-mm-ball", [0.94, 0.92, 0.82, 1.]);
    let road = glb.material_mesh(cube, "street", [0.22, 0.23, 0.25, 1.]);
    let grass = glb.material_mesh(cube, "context-ground", [0.27, 0.37, 0.22, 1.]);
    let mut home = Vec::new();
    for (id, mesh, center, size) in [
        ("floor", floor, [0., -0.075, 0.], [6.2, 0.15, 5.2]),
        ("left-wall", wall, [-3., 1.5, 0.], [0.2, 3., 5.2]),
        ("right-wall", wall, [3., 1.5, 0.], [0.2, 3., 5.2]),
        ("back-wall", wall, [0., 1.5, -2.5], [6.2, 3., 0.2]),
        ("front-left", wall, [-1.75, 1.5, 2.5], [2.5, 3., 0.2]),
        ("front-right", wall, [1.75, 1.5, 2.5], [2.5, 3., 0.2]),
        ("door-header", wall, [0., 2.6, 2.5], [1., 0.8, 0.2]),
        ("roof", roof, [0., 3.05, 0.], [6.4, 0.1, 5.4]),
        ("home-notes", panel, [-2.88, 1.4, 0.], [0.02, 0.7, 1.]),
        ("shared-tasks", panel, [0., 1.4, -2.38], [1., 0.7, 0.02]),
        ("ball", ball, [0.7, 0.02, 0.4], [0.02, 0.02, 0.02]),
    ] {
        home.push(glb.node(&format!("home/{id}"), mesh, center, size));
    }
    let home = glb.group("home", home, [0., 0., 0.]);
    let street = glb.node("street", road, [0., -0.175, 45.], [8., 0.1, 150.]);
    let ground = glb.node("local-ground", grass, [0., -0.25, 45.], [100., 0.1, 170.]);
    let mut roots = vec![ground, street, home];
    for (id, position, size) in [
        ("work", [20., 3., 30.], [12., 6., 10.]),
        ("university", [-25., 4., 65.], [16., 8., 12.]),
        ("community", [20., 2.5, 100.], [10., 5., 10.]),
    ] {
        roots.push(glb.node(id, wall, position, size));
    }
    glb.scene("standalone-room", &[home]);
    glb.finish(&roots)
}

pub(super) fn globe() -> Result<Vec<u8>> {
    let mut glb = Glb::new();
    let sphere = glb.sphere(16, 32);
    let surface = glb.material_mesh(sphere, "coarse-water", [0.06, 0.20, 0.34, 1.]);
    let a = 6_378_137.0;
    let b = a * (1.0 - 1.0 / 298.257223563);
    let root = glb.node("coarse-reference-ellipsoid", surface, [0.; 3], [a, b, a]);
    glb.finish(&[root])
}

pub(super) fn castle() -> Result<Vec<u8>> {
    let mut glb = Glb::new();
    let cube = glb.cube();
    let stone = glb.material_mesh(cube, "castle-stone", [0.48, 0.54, 0.50, 1.]);
    let mut roots = Vec::new();
    for (id, center, size) in [
        ("keep-floor", [0., -0.075, 0.], [6., 0.15, 5.]),
        ("keep-left", [-2., 4., -0.8], [0.2, 8., 3.2]),
        ("keep-right", [2., 4., -0.8], [0.2, 8., 3.2]),
        ("keep-back", [0., 4., -2.3], [4.2, 8., 0.2]),
        ("keep-front-left", [-1.25, 4., 0.7], [1.5, 8., 0.2]),
        ("keep-front-right", [1.25, 4., 0.7], [1.5, 8., 0.2]),
        ("keep-header", [0., 5.2, 0.7], [1., 5.6, 0.2]),
        ("keep-roof", [0., 8.05, -0.8], [4.4, 0.1, 3.4]),
    ] {
        roots.push(glb.node(id, stone, center, size));
    }
    for (index, (x, z)) in [(-2.5, -2.), (2.5, -2.), (-2.5, 2.), (2.5, 2.)]
        .into_iter()
        .enumerate()
    {
        roots.push(glb.node(&format!("tower-{index}"), stone, [x, 5., z], [1., 10., 1.]));
    }
    roots.push(glb.node("gate-left", stone, [-1.75, 2., 2.], [2.5, 4., 0.3]));
    roots.push(glb.node("gate-right", stone, [1.75, 2., 2.], [2.5, 4., 0.3]));
    roots.push(glb.node("gate-header", stone, [0., 3.2, 2.], [1., 1.6, 0.3]));
    glb.finish(&roots)
}

pub(super) fn mechanism() -> Result<Vec<u8>> {
    let mut glb = Glb::new();
    let cube = glb.cube();
    let metal = glb.material_mesh(cube, "mechanism-links", [0.35, 0.48, 0.57, 1.]);
    let base = glb.node("base", metal, [0., 0.1, 0.], [0.5, 0.2, 0.5]);
    let first = glb.node("first-link", metal, [0.5, 0., 0.], [1., 0.1, 0.15]);
    let second = glb.node("second-link", metal, [0.4, 0., 0.], [0.8, 0.1, 0.15]);
    let elbow = glb.group("elbow-port", vec![second], [1., 0., 0.]);
    let shoulder = glb.group("base-port", vec![first, elbow], [0., 0.25, 0.]);
    glb.finish(&[base, shoulder])
}

pub(super) fn private_room() -> Result<Vec<u8>> {
    let mut glb = Glb::new();
    let cube = glb.cube();
    let surface = glb.material_mesh(cube, "restricted-study", [0.45, 0.29, 0.38, 1.]);
    let mut roots = Vec::new();
    for (id, center, size) in [
        ("private-floor", [0., -0.05, 0.], [3., 0.1, 3.]),
        ("private-back", [0., 1.25, -1.5], [3., 2.5, 0.1]),
        ("private-left", [-1.5, 1.25, 0.], [0.1, 2.5, 3.]),
        ("private-right", [1.5, 1.25, 0.], [0.1, 2.5, 3.]),
        ("private-shelf", [0.8, 1.1, -1.35], [0.6, 0.15, 0.2]),
    ] {
        roots.push(glb.node(id, surface, center, size));
    }
    glb.finish(&roots)
}
