use serde::{Deserialize, Serialize};

const OPEN_SCENE: &str = include_str!("../fixtures/open_scene.gltf");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gltf {
    asset: GltfAsset,
    scene: usize,
    scenes: Vec<GltfScene>,
    nodes: Vec<GltfNode>,
    meshes: Vec<GltfMesh>,
    buffers: Vec<GltfBuffer>,
    buffer_views: Vec<GltfBufferView>,
    accessors: Vec<GltfAccessor>,
}

#[derive(Clone, Debug, Deserialize)]
struct GltfAsset {
    version: String,
    generator: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GltfScene {
    name: String,
    nodes: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
struct GltfNode {
    name: String,
    mesh: usize,
    translation: [f64; 3],
}

#[derive(Clone, Debug, Deserialize)]
struct GltfMesh {
    name: String,
    primitives: Vec<GltfPrimitive>,
}

#[derive(Clone, Debug, Deserialize)]
struct GltfPrimitive {
    attributes: GltfAttributes,
    indices: usize,
    mode: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct GltfAttributes {
    #[serde(rename = "POSITION")]
    position: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GltfBuffer {
    uri: String,
    byte_length: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GltfBufferView {
    buffer: usize,
    byte_offset: usize,
    byte_length: usize,
    target: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GltfAccessor {
    buffer_view: usize,
    component_type: u32,
    count: usize,
    #[serde(rename = "type")]
    kind: String,
    min: Option<[f32; 3]>,
    max: Option<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CollisionProxy {
    pub minimum: [f32; 3],
    pub maximum: [f32; 3],
    pub source_accessor: usize,
    pub replaceable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TypedPickEvent {
    pub schema_version: u32,
    pub event: String,
    pub output_port: String,
    pub artifact_node: String,
    pub world_position: [f64; 3],
    pub proxy_distance: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneArtifactEvidence {
    pub format: String,
    pub asset_version: String,
    pub generator: String,
    pub scene_name: String,
    pub node_name: String,
    pub mesh_name: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub embedded_bytes: usize,
    pub collision_proxy: CollisionProxy,
    pub pick_event: TypedPickEvent,
}

pub fn load_open_scene_fixture() -> Result<SceneArtifactEvidence, String> {
    let gltf: Gltf = serde_json::from_str(OPEN_SCENE).map_err(|error| error.to_string())?;
    if gltf.asset.version != "2.0" {
        return Err(format!("unsupported glTF version {}", gltf.asset.version));
    }
    let scene = gltf
        .scenes
        .get(gltf.scene)
        .ok_or_else(|| "glTF default scene is absent".to_string())?;
    let node_index = *scene
        .nodes
        .first()
        .ok_or_else(|| "glTF scene has no nodes".to_string())?;
    let node = gltf
        .nodes
        .get(node_index)
        .ok_or_else(|| "glTF node index is invalid".to_string())?;
    let mesh = gltf
        .meshes
        .get(node.mesh)
        .ok_or_else(|| "glTF mesh index is invalid".to_string())?;
    let primitive = mesh
        .primitives
        .first()
        .ok_or_else(|| "glTF mesh has no primitive".to_string())?;
    if primitive.mode != 4 {
        return Err("fixture primitive is not a triangle list".into());
    }
    let position = gltf
        .accessors
        .get(primitive.attributes.position)
        .ok_or_else(|| "glTF position accessor is absent".to_string())?;
    let indices = gltf
        .accessors
        .get(primitive.indices)
        .ok_or_else(|| "glTF index accessor is absent".to_string())?;
    validate_accessor(position, &gltf.buffer_views, "VEC3", 5126)?;
    validate_accessor(indices, &gltf.buffer_views, "SCALAR", 5123)?;
    let minimum = position
        .min
        .ok_or_else(|| "position accessor has no minimum".to_string())?;
    let maximum = position
        .max
        .ok_or_else(|| "position accessor has no maximum".to_string())?;
    let buffer = gltf
        .buffers
        .first()
        .ok_or_else(|| "glTF buffer is absent".to_string())?;
    if !buffer
        .uri
        .starts_with("data:application/octet-stream;base64,")
    {
        return Err("fixture buffer is not an embedded open data URI".into());
    }
    if buffer.byte_length != 44 {
        return Err(format!(
            "fixture declared {} bytes instead of 44",
            buffer.byte_length
        ));
    }
    let proxy = CollisionProxy {
        minimum,
        maximum,
        source_accessor: primitive.attributes.position,
        replaceable: true,
    };
    let pick_local = [0.25_f32, 0.0, 0.25];
    let pick_event = TypedPickEvent {
        schema_version: 1,
        event: "scene-picked".into(),
        output_port: "picked-artifact".into(),
        artifact_node: node.name.clone(),
        world_position: [
            node.translation[0] + f64::from(pick_local[0]),
            node.translation[1] + f64::from(pick_local[1]),
            node.translation[2] + f64::from(pick_local[2]),
        ],
        proxy_distance: distance_to_proxy(pick_local, &proxy),
    };
    Ok(SceneArtifactEvidence {
        format: "glTF 2.0 JSON with embedded buffer".into(),
        asset_version: gltf.asset.version,
        generator: gltf.asset.generator,
        scene_name: scene.name.clone(),
        node_name: node.name.clone(),
        mesh_name: mesh.name.clone(),
        vertex_count: position.count,
        index_count: indices.count,
        embedded_bytes: buffer.byte_length,
        collision_proxy: proxy,
        pick_event,
    })
}

fn validate_accessor(
    accessor: &GltfAccessor,
    views: &[GltfBufferView],
    kind: &str,
    component_type: u32,
) -> Result<(), String> {
    if accessor.kind != kind || accessor.component_type != component_type {
        return Err(format!(
            "accessor expected {kind}/{component_type}, got {}/{}",
            accessor.kind, accessor.component_type
        ));
    }
    let view = views
        .get(accessor.buffer_view)
        .ok_or_else(|| "accessor buffer view is absent".to_string())?;
    if view.buffer != 0 || view.byte_length == 0 || !matches!(view.target, 34962 | 34963) {
        return Err("accessor buffer view is malformed".into());
    }
    let _ = view.byte_offset;
    Ok(())
}

fn distance_to_proxy(point: [f32; 3], proxy: &CollisionProxy) -> f32 {
    let mut squared = 0.0;
    for axis in 0..3 {
        let delta = if point[axis] < proxy.minimum[axis] {
            proxy.minimum[axis] - point[axis]
        } else if point[axis] > proxy.maximum[axis] {
            point[axis] - proxy.maximum[axis]
        } else {
            0.0
        };
        squared += delta * delta;
    }
    squared.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_scene_builds_replaceable_proxy_and_typed_pick() {
        let evidence = load_open_scene_fixture().unwrap();
        assert_eq!(evidence.vertex_count, 3);
        assert_eq!(evidence.index_count, 3);
        assert!(evidence.collision_proxy.replaceable);
        assert_eq!(evidence.pick_event.output_port, "picked-artifact");
        assert_eq!(evidence.pick_event.proxy_distance, 0.0);
        assert!(evidence.pick_event.world_position[0] > 6_000_000.0);
    }
}
