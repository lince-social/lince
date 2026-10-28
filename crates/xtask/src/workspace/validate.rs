use super::fixtures::{Bundle, digest};
use crate::Result;
use glam::{Mat4, Vec3};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path},
    time::Instant,
};

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

pub(super) fn read(root: &Path, relative: &str, limit: u64) -> Result<Vec<u8>> {
    let path = Path::new(relative);
    require(
        !relative.is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "Fixture paths must be nonempty local paths without traversal",
    )?;
    let mut candidate = root.to_path_buf();
    for part in path.components() {
        candidate.push(part);
        require(
            !fs::symlink_metadata(&candidate)
                .map_err(|error| error.to_string())?
                .file_type()
                .is_symlink(),
            "Fixture payload symlinks are not accepted",
        )?;
    }
    let file = File::open(&candidate).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    require(
        metadata.is_file() && metadata.len() <= limit,
        format!("Fixture file is not regular or exceeds its byte cap: {relative}"),
    )?;
    let mut bytes = Vec::new();
    file.take(limit.checked_add(1).ok_or("Invalid fixture byte cap")?)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    require(
        bytes.len() as u64 <= limit,
        "Fixture file grew past its byte cap",
    )?;
    Ok(bytes)
}

fn finite(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
}

fn rotation(values: &[f64; 4]) -> bool {
    finite(values) && (values.iter().map(|value| value * value).sum::<f64>() - 1.).abs() <= 1e-6
}

fn cap(profile: &Value, group: &str, key: &str) -> Result<u64> {
    profile[group][key]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("Missing positive fixture cap: {group}.{key}"))
}

fn contract_report(value: &Value) -> Result<Value> {
    require(
        value["format"] == "lince-workspace-stage01-contract-decisions" && value["schema"] == 1,
        "Unsupported contract specification",
    )?;
    let expected: BTreeSet<_> = [("C", 17), ("K", 8), ("I", 6), ("E", 8), ("H", 10)]
        .into_iter()
        .flat_map(|(prefix, count)| {
            (1..=count).map(move |index| {
                if prefix == "C" {
                    format!("{prefix}{index:02}")
                } else {
                    format!("{prefix}{index}")
                }
            })
        })
        .collect();
    let mut actual = BTreeSet::new();
    for requirement in value["requirements"]
        .as_array()
        .ok_or("Missing contract requirements")?
    {
        let id = requirement["id"].as_str().ok_or("Contract has no ID")?;
        require(
            actual.insert(id.to_owned()),
            "Duplicate contract requirement",
        )?;
        let owner = requirement["normative_owner_stage"]
            .as_u64()
            .ok_or("Contract has no owner")?;
        require(
            (1..=21).contains(&owner)
                && requirement["normative_document"]
                    == format!("institute/anicca/workspace/{owner:02}-stage.md")
                && requirement["core_application_or_deferred_boundary"]
                    .as_str()
                    .is_some_and(|text| !text.is_empty()),
            "Contract lacks a valid owning stage or implementation boundary",
        )?;
    }
    require(
        actual == expected,
        "Required contract coverage differs from C/K/I/E/H",
    )?;
    for key in [
        "initial_encoding",
        "directory_minimum",
        "native_and_agent_edit_flow",
        "first_adapter_record",
    ] {
        require(
            value[key].is_object(),
            format!("Missing selected specification: {key}"),
        )?;
    }
    require(
        value["directory_minimum"]["first_consumer_stage"] == 2,
        "Directory minimum lacks its first consumer",
    )?;
    require(
        value["remaining_decisions"]
            .as_array()
            .is_some_and(|decisions| {
                decisions.iter().all(|decision| {
                    decision["owner_stage"]
                        .as_u64()
                        .is_some_and(|owner| (2..=21).contains(&owner))
                        && decision["decisive_evidence"]
                            .as_str()
                            .is_some_and(|text| !text.is_empty())
                })
            }),
        "Remaining contract decision lacks owner/evidence",
    )?;
    Ok(
        json!({"registered_requirements":actual.len(),"scope":"Specification coverage only; production implementations remain owned by their stages."}),
    )
}

fn hosting_report(value: &Value, bundle: &Bundle) -> Result<Value> {
    require(
        value["format"] == "lince-workspace-stage01-hosting-replay" && value["schema"] == 1,
        "Unsupported hosting specification",
    )?;
    let participants: BTreeSet<_> = value["participants"]
        .as_array()
        .ok_or("Missing hosted participants")?
        .iter()
        .filter_map(|participant| participant["id"].as_str())
        .collect();
    require(
        participants == BTreeSet::from(["host", "editor", "visitor"]),
        "Hosting needs host/editor/restricted visitor specifications",
    )?;
    let mut ids = BTreeSet::new();
    for case in value["cases"].as_array().ok_or("Missing hosting cases")? {
        let id = case["id"].as_str().ok_or("Hosting case lacks ID")?;
        require(ids.insert(id), "Duplicate hosting case")?;
        let replay = bundle
            .cases
            .iter()
            .find(|replay| replay.id == id)
            .ok_or("Missing phased hosting replay")?;
        require(
            serde_json::to_value(&replay.steps).map_err(|error| error.to_string())?
                == case["phase_replays"],
            "Hosting source and manifest replay disagree",
        )?;
        let phases: BTreeSet<_> = replay
            .steps
            .iter()
            .map(|step| u64::from(step.runtime_owner_stage))
            .collect();
        let declared: BTreeSet<_> = case["core_phases"]
            .as_array()
            .ok_or("Missing hosting phase owners")?
            .iter()
            .filter_map(Value::as_u64)
            .collect();
        require(
            phases == declared && !phases.is_empty(),
            "Hosting replay phase ownership disagrees",
        )?;
        for key in ["hard", "quality", "metrics"] {
            require(
                case[key].as_array().is_some_and(|items| !items.is_empty()),
                format!("Hosting case lacks {key} acceptance"),
            )?;
        }
    }
    require(
        ids == BTreeSet::from(["B33", "B34", "B35", "B36"]),
        "Hosting replay coverage is incomplete",
    )?;
    require(
        value["B37"]["first_runtime_owner_stage"] == 17
            && value["B37"]["status"] == "deferred-no-stage01-experiment",
        "B37 must stay deferred",
    )?;
    Ok(
        json!({"cases":ids,"participants":participants,"scope":"Phased replay contract consistency, not current hosting runtime verification."}),
    )
}

fn graph_node(
    node: gltf::Node<'_>,
    ancestors: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<usize>,
) -> Result<()> {
    require(
        ancestors.insert(node.index()),
        "Cycle in glTF node hierarchy",
    )?;
    require(
        visited.insert(node.index()),
        "Repeated glTF node within a scene; expanded work is not a tree",
    )?;
    let matrix = node.transform().matrix();
    require(
        matrix.iter().flatten().all(|value| value.is_finite()),
        "Nonfinite glTF transform",
    )?;
    for child in node.children() {
        graph_node(child, ancestors, visited)?;
    }
    ancestors.remove(&node.index());
    Ok(())
}

fn gltf_report(bytes: &[u8], profile: &Value) -> Result<Value> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| error.to_string())?;
    let binary = gltf
        .blob
        .as_deref()
        .ok_or("Fixture glTF must have one embedded buffer")?;
    require(
        gltf.buffers().count() == 1,
        "Fixture glTF has multiple buffers",
    )?;
    for buffer in gltf.buffers() {
        require(
            matches!(buffer.source(), gltf::buffer::Source::Bin) && buffer.length() <= binary.len(),
            "External or truncated glTF buffer",
        )?;
    }
    require(
        gltf.nodes().count() as u64 <= cap(profile, "fixture_validation_limits", "gltf_nodes")?,
        "Too many fixture glTF nodes",
    )?;
    for scene in gltf.scenes() {
        let mut visited = BTreeSet::new();
        for node in scene.nodes() {
            graph_node(node, &mut BTreeSet::new(), &mut visited)?;
        }
    }
    let mut skin_sizes = BTreeMap::new();
    for node in gltf.nodes() {
        if let (Some(mesh), Some(skin)) = (node.mesh(), node.skin()) {
            let count = skin.joints().count();
            skin_sizes
                .entry(mesh.index())
                .and_modify(|limit: &mut usize| *limit = (*limit).min(count))
                .or_insert(count);
        }
    }
    let mut position_accessors = BTreeSet::new();
    let mut vertices = 0u64;
    let mut triangles = 0u64;
    let mut influences = 0u64;
    let mut morph_targets = 0u64;
    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            require(
                primitive.mode() == gltf::mesh::Mode::Triangles,
                "Fixture mesh must use explicit triangles",
            )?;
            let accessor = primitive
                .get(&gltf::Semantic::Positions)
                .ok_or("Fixture mesh has no positions")?;
            let reader = primitive.reader(|buffer| (buffer.index() == 0).then_some(binary));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("Unreadable fixture positions")?
                .collect();
            require(
                !positions.is_empty() && positions.iter().flatten().all(|value| value.is_finite()),
                "Empty or nonfinite fixture positions",
            )?;
            let bounds = primitive.bounding_box();
            require(
                (0..3).all(|axis| {
                    let minimum = bounds.min[axis];
                    let maximum = bounds.max[axis];
                    let tolerance = 8.0 * f32::EPSILON * minimum.abs().max(maximum.abs()).max(1.0);
                    minimum.is_finite()
                        && maximum.is_finite()
                        && minimum <= maximum
                        && positions.iter().all(|point| {
                            point[axis] >= minimum - tolerance && point[axis] <= maximum + tolerance
                        })
                }),
                "Fixture bounds do not enclose the actual position buffer",
            )?;
            if position_accessors.insert(accessor.index()) {
                vertices = vertices
                    .checked_add(positions.len() as u64)
                    .ok_or("Vertex count overflow")?;
            }
            let indices: Vec<_> = reader
                .read_indices()
                .ok_or("Fixture mesh needs explicit indices")?
                .into_u32()
                .collect();
            require(
                indices.len().is_multiple_of(3)
                    && indices
                        .iter()
                        .all(|index| (*index as usize) < positions.len()),
                "Invalid fixture triangle indices",
            )?;
            triangles = triangles
                .checked_add(indices.len() as u64 / 3)
                .ok_or("Triangle count overflow")?;
            if let Some(normals) = reader.read_normals() {
                let normals: Vec<_> = normals.collect();
                require(
                    normals.len() == positions.len()
                        && normals.iter().flatten().all(|value| value.is_finite()),
                    "Invalid fixture normals",
                )?;
            }
            if let Some(&joint_count) = skin_sizes.get(&mesh.index()) {
                let weights: Vec<_> = reader
                    .read_weights(0)
                    .ok_or("Skinned fixture has no weights")?
                    .into_f32()
                    .collect();
                let joints: Vec<_> = reader
                    .read_joints(0)
                    .ok_or("Skinned fixture has no joint indices")?
                    .into_u16()
                    .collect();
                require(
                    weights.len() == positions.len() && joints.len() == positions.len(),
                    "Skin attribute count mismatch",
                )?;
                for (weights, joints) in weights.iter().zip(joints) {
                    require(
                        weights
                            .iter()
                            .all(|weight| weight.is_finite() && *weight >= 0.)
                            && (weights.iter().sum::<f32>() - 1.).abs() < 1e-4,
                        "Nonfinite, negative or unnormalized fixture weights",
                    )?;
                    require(
                        weights.iter().zip(joints).all(|(weight, joint)| {
                            *weight == 0. || usize::from(joint) < joint_count
                        }),
                        "Skin joint index outside the bound rig",
                    )?;
                }
                influences = influences.max(4);
            }
            morph_targets += primitive.morph_targets().count() as u64;
        }
    }
    require(
        vertices
            <= cap(
                profile,
                "fixture_validation_limits",
                "gltf_unique_position_vertices",
            )?,
        "Fixture vertex cap exceeded",
    )?;
    let joints = gltf
        .skins()
        .map(|skin| skin.joints().count())
        .sum::<usize>();
    require(
        joints as u64 <= cap(profile, "fixture_validation_limits", "gltf_joint_count")?,
        "Fixture joint cap exceeded",
    )?;
    let mut image_rgba_bytes = 0u64;
    for image in gltf.images() {
        let gltf::image::Source::View { view, .. } = image.source() else {
            return Err("Fixture images must be embedded".into());
        };
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("Image range overflow")?;
        let bytes = binary
            .get(view.offset()..end)
            .ok_or("Truncated fixture image")?;
        let dimensions = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|error| error.to_string())?
            .into_dimensions()
            .map_err(|error| error.to_string())?;
        let maximum = cap(
            profile,
            "fixture_validation_limits",
            "gltf_texture_dimension",
        )?;
        require(
            dimensions.0 > 0
                && dimensions.1 > 0
                && u64::from(dimensions.0) <= maximum
                && u64::from(dimensions.1) <= maximum,
            "Fixture image dimensions exceed admission",
        )?;
        image_rgba_bytes = image_rgba_bytes
            .checked_add(u64::from(dimensions.0) * u64::from(dimensions.1) * 4)
            .ok_or("Image byte overflow")?;
    }
    let clips: Vec<_> = gltf
        .animations()
        .map(|animation| animation.name().unwrap_or("").to_owned())
        .collect();
    for animation in gltf.animations() {
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| (buffer.index() == 0).then_some(binary));
            let times: Vec<_> = reader
                .read_inputs()
                .ok_or("Animation inputs are missing")?
                .collect();
            require(
                !times.is_empty()
                    && times.iter().all(|time| time.is_finite() && *time >= 0.)
                    && times.windows(2).all(|pair| pair[0] < pair[1]),
                "Invalid animation time samples",
            )?;
        }
    }
    Ok(json!({
        "nodes":gltf.nodes().count(),"scenes":gltf.scenes().count(),"meshes":gltf.meshes().count(),
        "unique_position_vertices":vertices,"mesh_definition_triangles":triangles,
        "skin_joints":joints,"maximum_vertex_influences":influences,"morph_target_sets":morph_targets,
        "clips":clips,"embedded_source_buffer_bytes":binary.len(),"decoded_rgba_image_bytes":image_rgba_bytes,
        "skin_joint_names":gltf.skins().flat_map(|skin|skin.joints()).map(|joint|joint.name().unwrap_or("unnamed").to_owned()).collect::<Vec<_>>(),
        "scope":"Validated fixture payloads/counts; display error, physical behavior and actual GPU residency need the owning runtime proof."
    }))
}

fn passage(node: gltf::Node<'_>, parent: Mat4, minimum: Vec3, maximum: Vec3) -> Result<()> {
    let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            let bounds = primitive.bounding_box();
            let lo = Vec3::from_array(bounds.min);
            let hi = Vec3::from_array(bounds.max);
            let corners: [Vec3; 8] = std::array::from_fn(|i| {
                transform.transform_point3(Vec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                ))
            });
            let lo = corners
                .iter()
                .copied()
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let hi = corners
                .iter()
                .copied()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            require(
                !(lo.cmplt(maximum).all() && hi.cmpgt(minimum).all()),
                format!(
                    "Fixture doorway/interior path blocked by {}",
                    node.name().unwrap_or("unnamed node")
                ),
            )?;
        }
    }
    for child in node.children() {
        passage(child, transform, minimum, maximum)?;
    }
    Ok(())
}

fn doorway(bytes: &[u8]) -> Result<()> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| error.to_string())?;
    for node in gltf
        .default_scene()
        .ok_or("Fixture has no default scene")?
        .nodes()
    {
        passage(
            node,
            Mat4::IDENTITY,
            Vec3::new(-0.25, 0.01, 0.),
            Vec3::new(0.25, 1.8, 4.),
        )?;
    }
    Ok(())
}

fn coastline(bytes: &[u8], profile: &Value) -> Result<Value> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    require(
        value["type"] == "FeatureCollection",
        "Coastline is not GeoJSON",
    )?;
    let features = value["features"]
        .as_array()
        .ok_or("Missing coastline features")?;
    require(
        features.len() as u64 <= cap(profile, "fixture_validation_limits", "coastline_features")?,
        "Too many coastline features",
    )?;
    let mut coordinates = 0u64;
    for feature in features {
        require(
            feature["geometry"]["type"] == "LineString",
            "Unsupported coastline geometry",
        )?;
        for point in feature["geometry"]["coordinates"]
            .as_array()
            .ok_or("Missing coastline coordinates")?
        {
            let point = point.as_array().ok_or("Invalid coordinate")?;
            require(point.len() == 2, "Invalid geographic dimension")?;
            let longitude = point[0].as_f64().ok_or("Invalid longitude")?;
            let latitude = point[1].as_f64().ok_or("Invalid latitude")?;
            require(
                longitude.is_finite()
                    && latitude.is_finite()
                    && longitude.abs() <= 180.
                    && latitude.abs() <= 90.,
                "Geographic coordinate outside longitude/latitude range",
            )?;
            coordinates += 1;
        }
    }
    require(
        coordinates
            <= cap(
                profile,
                "fixture_validation_limits",
                "coastline_coordinates",
            )?,
        "Too many coastline coordinates",
    )?;
    Ok(
        json!({"features":features.len(),"coordinates":coordinates,"crs":"longitude/latitude degrees","height_coverage":"absent"}),
    )
}

pub(super) fn check(root: &Path) -> Result<Value> {
    let started = Instant::now();
    let bytes = read(root, "manifest.json", 262144)?;
    let bundle: Bundle = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    require(
        bundle.format == "lince-workspace-stage01-fixtures" && bundle.schema == 1,
        "Unsupported fixture schema",
    )?;
    require(
        bundle.status == "fixture-inputs-only-runtime-milestones-pending",
        "Fixture inspection cannot certify runtime completion",
    )?;
    require(
        bundle.assets.len() as u64 <= cap(&bundle.profile, "fixture_validation_limits", "assets")?,
        "Too many fixture assets",
    )?;
    require(
        bundle.scenes.len() as u64 <= cap(&bundle.profile, "fixture_validation_limits", "scenes")?,
        "Too many fixture scenes",
    )?;
    require(
        bytes.len() as u64
            <= cap(
                &bundle.profile,
                "fixture_validation_limits",
                "manifest_bytes",
            )?,
        "Fixture manifest cap exceeded",
    )?;
    let mut assets = BTreeMap::new();
    let mut paths = BTreeSet::new();
    let mut reports = BTreeMap::new();
    let mut total = bytes.len() as u64;
    let mut source_buffers = 0u64;
    for asset in &bundle.assets {
        require(
            !asset.id.is_empty() && assets.insert(asset.id.as_str(), asset).is_none(),
            "Empty or duplicate fixture asset ID",
        )?;
        require(
            paths.insert(asset.path.as_str()),
            "Duplicate fixture asset path",
        )?;
        require(
            matches!(
                asset.disclosure.as_str(),
                "fixture-public" | "identified-editor" | "host-private"
            ),
            "Missing or unknown disclosure label",
        )?;
        require(!asset.license.is_empty(), "Missing fixture source license")?;
        let data = read(
            root,
            &asset.path,
            cap(
                &bundle.profile,
                "fixture_validation_limits",
                "individual_asset_bytes",
            )?,
        )?;
        require(
            data.len() as u64 == asset.encoded_bytes && digest(&data) == asset.sha256,
            format!("Fixture length/hash mismatch: {}", asset.id),
        )?;
        total = total
            .checked_add(data.len() as u64)
            .ok_or("Fixture byte overflow")?;
        require(
            total <= cap(&bundle.profile, "limits", "packed_fixture_bytes")?,
            "Packed fixture cap exceeded",
        )?;
        let report = match asset.kind.as_str() {
            "glb" => gltf_report(&data, &bundle.profile)?,
            "geojson" => coastline(&data, &bundle.profile)?,
            "json" => {
                let value: Value =
                    serde_json::from_slice(&data).map_err(|error| error.to_string())?;
                match asset.id.as_str() {
                    "contracts" => contract_report(&value)?,
                    "hosting" => hosting_report(&value, &bundle)?,
                    _ => json!({"valid_json":true}),
                }
            }
            "text" => {
                std::str::from_utf8(&data).map_err(|error| error.to_string())?;
                json!({"valid_utf8":true})
            }
            _ => return Err("Unsupported fixture asset kind".into()),
        };
        source_buffers = source_buffers
            .checked_add(report["embedded_source_buffer_bytes"].as_u64().unwrap_or(0))
            .and_then(|bytes| {
                bytes.checked_add(report["decoded_rgba_image_bytes"].as_u64().unwrap_or(0))
            })
            .ok_or("Decoded estimate overflow")?;
        if matches!(asset.id.as_str(), "local-place" | "castle") {
            doorway(&data)?;
        }
        reports.insert(asset.id.clone(), report);
    }
    require(
        source_buffers
            <= cap(
                &bundle.profile,
                "limits",
                "incremental_model_cpu_peak_bytes",
            )?,
        "Fixture raw-buffer/decoded-image sum exceeds the entire model budget before runtime overhead",
    )?;
    let mut scenes = BTreeSet::new();
    let mut targets: BTreeSet<&str> = assets.keys().copied().collect();
    for scene in &bundle.scenes {
        require(
            !scene.id.is_empty() && scenes.insert(scene.id.as_str()),
            "Empty or duplicate scene ID",
        )?;
        require(
            matches!(
                scene.disclosure.as_str(),
                "fixture-public" | "identified-editor" | "host-private"
            ),
            "Unknown scene disclosure label",
        )?;
        require(
            !scene.required_profiles.is_empty(),
            "Scene has no declared profile",
        )?;
        require(
            scene.frames.len() as u64
                <= cap(
                    &bundle.profile,
                    "fixture_validation_limits",
                    "frames_per_scene",
                )?,
            "Scene frame cap exceeded",
        )?;
        require(
            scene.objects.len() as u64
                <= cap(
                    &bundle.profile,
                    "fixture_validation_limits",
                    "objects_per_scene",
                )?,
            "Scene object cap exceeded",
        )?;
        let mut frames = BTreeMap::new();
        for frame in &scene.frames {
            require(
                !frame.id.is_empty() && frames.insert(frame.id.as_str(), frame).is_none(),
                "Empty or duplicate frame ID",
            )?;
            require(
                finite(&frame.origin_m) && rotation(&frame.rotation_xyzw),
                "Invalid frame origin/rotation",
            )?;
        }
        require(
            scene.frames.iter().any(|frame| frame.parent.is_none()),
            "Scene needs an independent root frame",
        )?;
        for frame in &scene.frames {
            let mut chain = BTreeSet::new();
            let mut current = Some(frame.id.as_str());
            while let Some(id) = current {
                require(chain.insert(id), "Cycle in fixture frame hierarchy")?;
                current = frames
                    .get(id)
                    .ok_or("Frame parent does not exist")?
                    .parent
                    .as_deref();
            }
        }
        let mut objects = BTreeSet::new();
        for object in &scene.objects {
            require(
                !object.id.is_empty() && objects.insert(object.id.as_str()),
                "Empty or duplicate occurrence ID",
            )?;
            require(
                frames.contains_key(object.frame.as_str())
                    && finite(&object.origin_m)
                    && rotation(&object.rotation_xyzw),
                "Invalid object frame/placement",
            )?;
            let asset = assets
                .get(object.asset.as_str())
                .ok_or("Object has no required asset")?;
            if scene.disclosure == "fixture-public" {
                require(
                    asset.disclosure == "fixture-public",
                    "Public scene references restricted payload",
                )?;
            }
            if let Some(index) = object.gltf_scene {
                require(
                    reports[&object.asset]["scenes"]
                        .as_u64()
                        .is_some_and(|count| (index as u64) < count),
                    "Object glTF scene index is unavailable",
                )?;
            }
            targets.insert(object.id.as_str());
        }
        for bookmark in &scene.bookmarks {
            require(
                !bookmark.id.is_empty() && targets.insert(bookmark.id.as_str()),
                "Empty or duplicate bookmark ID",
            )?;
            require(
                frames.contains_key(bookmark.frame.as_str())
                    && objects.contains(bookmark.subject.as_str()),
                "Bookmark frame/subject does not resolve",
            )?;
            require(
                finite(&bookmark.eye_m)
                    && finite(&bookmark.target_m)
                    && bookmark.eye_m != bookmark.target_m,
                "Invalid bookmark camera",
            )?;
            require(
                matches!(bookmark.mode.as_str(), "Orbit" | "Fly" | "Walk"),
                "Unknown explicit navigation mode",
            )?;
        }
    }
    let mut cases = BTreeSet::new();
    for case in &bundle.cases {
        require(
            cases.insert(case.id.as_str())
                && !case.steps.is_empty()
                && !case.delivery_kind.is_empty()
                && !case.requirements.is_empty(),
            "Duplicate or empty fixture case",
        )?;
        require(
            case.scenes.iter().all(|id| scenes.contains(id.as_str())),
            "Fixture case references an unavailable scene",
        )?;
        require(
            case.steps.len() as u64
                <= cap(&bundle.profile, "fixture_validation_limits", "replay_steps")?,
            "Replay step cap exceeded",
        )?;
        let mut ids = BTreeSet::new();
        for step in &case.steps {
            require(
                ids.insert(step.id.as_str())
                    && !step.native_action.is_empty()
                    && !step.expected.is_empty()
                    && !step.measured.is_empty(),
                "Replay lacks a unique step, action, expected result or metric",
            )?;
            require(
                targets.contains(step.target.as_str()),
                format!("Replay target does not resolve: {}", step.target),
            )?;
            require(
                (2..=15).contains(&step.runtime_owner_stage),
                "Core replay must identify its later runtime owner",
            )?;
        }
    }
    require(
        [
            "B25", "B26", "B27", "B28", "B33", "B34", "B35", "B36", "B38", "B41",
        ]
        .iter()
        .all(|id| cases.contains(id)),
        "Required core fixture case is missing",
    )?;
    let character = reports
        .get("starter-character")
        .ok_or("Missing starter character")?;
    let clips = character["clips"]
        .as_array()
        .ok_or("Missing starter clips")?;
    require(
        clips.len() == 2 && clips.contains(&json!("idle")) && clips.contains(&json!("walk")),
        "Starter unexpectedly includes inactive source clips",
    )?;
    require(
        character["skin_joints"] == 27 && character["maximum_vertex_influences"] == 4,
        "Starter rig/weight counts changed; refine B41 before accepting",
    )?;
    let starter_caps = &bundle.profile["character_phase_selection"]["starter_caps"];
    for metric in [
        "unique_position_vertices",
        "skin_joints",
        "maximum_vertex_influences",
        "morph_target_sets",
    ] {
        require(
            character[metric]
                .as_u64()
                .zip(starter_caps[metric].as_u64())
                .is_some_and(|(actual, limit)| actual <= limit),
            format!("Starter exceeds selected {metric} cap"),
        )?;
    }
    require(
        assets.get("starter-character").is_some_and(|asset| {
            starter_caps["encoded_bytes"]
                .as_u64()
                .is_some_and(|limit| asset.encoded_bytes <= limit)
        }) && character["embedded_source_buffer_bytes"]
            .as_u64()
            .zip(character["decoded_rgba_image_bytes"].as_u64())
            .and_then(|(buffers, images)| buffers.checked_add(images))
            .zip(starter_caps["decoded_source_and_texture_bytes"].as_u64())
            .is_some_and(|(bytes, limit)| bytes <= limit),
        "Starter encoded/decoded lower bound exceeds its phase cap",
    )?;
    require(
        reports
            .get("local-place")
            .is_some_and(|report| report["scenes"].as_u64().is_some_and(|count| count >= 2)),
        "Standalone room scene is missing",
    )?;
    Ok(json!({
        "format":"lince-workspace-fixture-inspection","schema":1,
        "status":"fixture-integrity-and-replay-specifications-verified",
        "audience":"local fixture author; this catalogue includes restricted synthetic assets and is not a public entry manifest",
        "runtime_workflow_evidence":"pending at the specified owning stages",
        "packed_fixture_bytes":total,
        "raw_buffers_and_decoded_images_bytes":source_buffers,
        "memory_scope":"Source/decoded-image lower bound only, not actual active peak CPU or GPU residency.",
        "assets":reports,"scenes":scenes,"cases":cases,
        "validation_elapsed_ms":started.elapsed().as_secs_f64()*1000.
    }))
}
