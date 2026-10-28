use super::geometry::Glb;
use crate::Result;
use glam::{Mat4, Vec3};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn accessor_references(
    document: &mut Value,
    mut visit: impl FnMut(&mut Value) -> Result<()>,
) -> Result<()> {
    if let Some(meshes) = document.get_mut("meshes").and_then(Value::as_array_mut) {
        for mesh in meshes {
            for primitive in mesh["primitives"]
                .as_array_mut()
                .ok_or("Missing mesh primitives")?
            {
                for value in primitive["attributes"]
                    .as_object_mut()
                    .ok_or("Missing attributes")?
                    .values_mut()
                {
                    visit(value)?;
                }
                if let Some(index) = primitive.get_mut("indices") {
                    visit(index)?;
                }
                if let Some(targets) = primitive.get_mut("targets").and_then(Value::as_array_mut) {
                    for target in targets {
                        for value in target
                            .as_object_mut()
                            .ok_or("Invalid morph target")?
                            .values_mut()
                        {
                            visit(value)?;
                        }
                    }
                }
            }
        }
    }
    if let Some(skins) = document.get_mut("skins").and_then(Value::as_array_mut) {
        for skin in skins {
            if let Some(accessor) = skin.get_mut("inverseBindMatrices") {
                visit(accessor)?;
            }
        }
    }
    if let Some(animations) = document.get_mut("animations").and_then(Value::as_array_mut) {
        for animation in animations {
            for sampler in animation["samplers"]
                .as_array_mut()
                .ok_or("Missing animation samplers")?
            {
                visit(&mut sampler["input"])?;
                visit(&mut sampler["output"])?;
            }
        }
    }
    Ok(())
}

fn view_references(
    document: &mut Value,
    mut visit: impl FnMut(&mut Value) -> Result<()>,
) -> Result<()> {
    for accessor in document["accessors"]
        .as_array_mut()
        .ok_or("Missing accessors")?
    {
        if let Some(view) = accessor.get_mut("bufferView") {
            visit(view)?;
        }
        if let Some(sparse) = accessor.get_mut("sparse") {
            visit(&mut sparse["indices"]["bufferView"])?;
            visit(&mut sparse["values"]["bufferView"])?;
        }
    }
    if let Some(images) = document.get_mut("images").and_then(Value::as_array_mut) {
        for image in images {
            if let Some(view) = image.get_mut("bufferView") {
                visit(view)?;
            } else {
                return Err("Character fixture requires embedded image payloads".into());
            }
        }
    }
    Ok(())
}

fn index(value: &Value) -> Result<usize> {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or("Invalid glTF reference".into())
}

fn global(node: gltf::Node<'_>, parent: Mat4, target: usize) -> Option<Mat4> {
    let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if node.index() == target {
        return Some(transform);
    }
    node.children()
        .find_map(|child| global(child, transform, target))
}

pub(super) fn starter(source: &[u8]) -> Result<Vec<u8>> {
    let parsed = gltf::binary::Glb::from_slice(source).map_err(|error| error.to_string())?;
    let binary = parsed
        .bin
        .ok_or("Character source has no embedded buffer")?;
    let mut document: Value =
        serde_json::from_slice(&parsed.json).map_err(|error| error.to_string())?;
    let animations = document["animations"]
        .as_array_mut()
        .ok_or("Character has no authored clips")?;
    animations.retain(|clip| matches!(clip["name"].as_str(), Some("idle" | "walk")));
    if animations.len() != 2 {
        return Err("Character fixture requires the actual idle and walk clips".into());
    }
    let mut used = BTreeSet::new();
    accessor_references(&mut document, |value| {
        used.insert(index(value)?);
        Ok(())
    })?;
    let old = document["accessors"]
        .as_array()
        .ok_or("Missing source accessors")?;
    let accessors = used
        .iter()
        .map(|&index| {
            old.get(index)
                .cloned()
                .ok_or("Accessor outside source".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    let remap: BTreeMap<_, _> = used
        .into_iter()
        .enumerate()
        .map(|(new, old)| (old, new))
        .collect();
    accessor_references(&mut document, |value| {
        *value = json!(
            remap
                .get(&index(value)?)
                .ok_or("Missing accessor mapping")?
        );
        Ok(())
    })?;
    document["accessors"] = json!(accessors);
    let mut used = BTreeSet::new();
    view_references(&mut document, |value| {
        used.insert(index(value)?);
        Ok(())
    })?;
    let old = document["bufferViews"]
        .as_array()
        .ok_or("Missing source buffer views")?;
    let mut packed = Vec::new();
    let mut views = Vec::new();
    let mut remap = BTreeMap::new();
    for old_index in used {
        let mut view = old
            .get(old_index)
            .cloned()
            .ok_or("Buffer view outside source")?;
        if view["buffer"].as_u64() != Some(0) {
            return Err("Character fixture supports one embedded source buffer".into());
        }
        let offset = view.get("byteOffset").map(index).transpose()?.unwrap_or(0);
        let length = index(&view["byteLength"])?;
        let end = offset
            .checked_add(length)
            .ok_or("Buffer view range overflow")?;
        let bytes = binary
            .get(offset..end)
            .ok_or("Source buffer view is truncated")?;
        packed.resize(packed.len().next_multiple_of(4), 0);
        view["byteOffset"] = json!(packed.len());
        packed.extend_from_slice(bytes);
        remap.insert(old_index, views.len());
        views.push(view);
    }
    view_references(&mut document, |value| {
        *value = json!(remap.get(&index(value)?).ok_or("Missing view mapping")?);
        Ok(())
    })?;
    document["bufferViews"] = json!(views);
    let parsed = gltf::Gltf::from_slice(source).map_err(|error| error.to_string())?;
    let neck = parsed
        .nodes()
        .find(|node| node.name() == Some("neck"))
        .ok_or("Character has no neck attachment")?
        .index();
    let scene = parsed
        .default_scene()
        .ok_or("Character has no default scene")?;
    let transform = scene
        .nodes()
        .find_map(|node| global(node, Mat4::IDENTITY, neck))
        .ok_or("Neck is not part of the selected scene")?;
    let center = transform.transform_point3(Vec3::ZERO);
    let extent = transform.transform_vector3(Vec3::Y).length() * 0.22;
    let inverse = transform.inverse();
    let positions = [
        center + Vec3::new(-extent, 0., extent * 0.7),
        center + Vec3::new(extent, 0., extent * 0.7),
        center + Vec3::new(0., -extent, extent * 0.85),
    ]
    .map(|point| inverse.transform_point3(point).to_array());
    let normal = (Vec3::from_array(positions[1]) - Vec3::from_array(positions[0]))
        .cross(Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]))
        .normalize()
        .to_array();
    let mut glb = Glb::from_parts(document, packed);
    let mesh = glb.mesh(
        "fixture-bandana",
        &positions,
        &[normal; 3],
        &[0, 1, 2, 2, 1, 0],
    );
    let mesh = glb.material_mesh(mesh, "fixture-bandana-green", [0.12, 0.5, 0.32, 1.]);
    let garment = glb.node("fixture-bandana", mesh, [0.; 3], [1.; 3]);
    glb.attach(neck, garment);
    let roots =
        glb.document()["scenes"][glb.document()["scene"].as_u64().unwrap_or(0) as usize]["nodes"]
            .as_array()
            .ok_or("Missing scene roots")?
            .iter()
            .map(index)
            .collect::<Result<Vec<_>>>()?;
    glb.finish(&roots)
}
