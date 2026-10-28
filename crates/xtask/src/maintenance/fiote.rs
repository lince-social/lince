use super::Result;
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use gltf::animation::util::ReadOutputs;
use image::{Rgba, RgbaImage};
use serde_json::{Value, json};
use std::{ffi::OsString, fs, path::Path};

const CLIPS: &[(&str, f32)] = &[
    ("idle", 4.0),
    ("look_around", 4.0),
    ("walk", 1.5),
    ("turn_left", 1.5),
    ("turn_right", 1.5),
    ("sit_idle", 4.0),
    ("sit_down", 1.5),
    ("stand_up", 1.5),
    ("lie_down", 2.0),
    ("rest", 4.0),
    ("get_up", 2.0),
    ("fall_asleep", 2.0),
    ("sleep", 6.0),
    ("wake_up", 2.0),
    ("groom_paw", 4.0),
    ("groom_flank", 4.0),
    ("stretch", 3.0),
];
const PAWS: &[&str] = &["front_left", "front_right", "hind_left", "hind_right"];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

pub(super) fn dispatch(root: &Path, args: &[OsString]) -> Result<()> {
    let action = args
        .first()
        .and_then(|x| x.to_str())
        .ok_or("Choose export, validate, render, or extract-source")?;
    require(args.len() <= 2, "Unexpected asset arguments")?;
    let directory = args
        .get(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("institute/assets/characters/fiote"));
    match action {
        "extract-source" => extract(&directory),
        "export" => export(&directory),
        "validate" => {
            let report = validate(&directory.join("fiote.glb"))?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(err)?);
            Ok(())
        }
        "render" => render(&directory),
        _ => Err(format!("Unknown Fiote asset action: {action}")),
    }
}

fn extract(directory: &Path) -> Result<()> {
    let bytes = fs::read(directory.join("fiote.glb")).map_err(err)?;
    validate_bytes(&bytes)?;
    let parsed = gltf::binary::Glb::from_slice(&bytes).map_err(err)?;
    let mut document: Value = serde_json::from_slice(&parsed.json).map_err(err)?;
    let blob = parsed.bin.ok_or("Missing GLB buffer")?;
    document["buffers"][0]["uri"] = json!("fiote.bin");
    let source = directory.join("source");
    fs::create_dir(&source).map_err(err)?;
    fs::write(source.join("fiote.bin"), &blob).map_err(err)?;
    fs::write(
        source.join("fiote.gltf"),
        serde_json::to_vec_pretty(&document).map_err(err)?,
    )
    .map_err(err)
}

fn export(directory: &Path) -> Result<()> {
    let mut document: Value =
        serde_json::from_slice(&fs::read(directory.join("source/fiote.gltf")).map_err(err)?)
            .map_err(err)?;
    require(
        document["buffers"].as_array().is_some_and(|v| v.len() == 1),
        "Expected one source buffer",
    )?;
    require(
        document["buffers"][0]["uri"] == "fiote.bin",
        "Only the local fiote.bin buffer is accepted",
    )?;
    let bin = fs::read(directory.join("source/fiote.bin")).map_err(err)?;
    let length = document["buffers"][0]["byteLength"]
        .as_u64()
        .ok_or("Missing source buffer length")? as usize;
    require(
        bin.len() >= length && bin.len() - length <= 3,
        "Source buffer length differs from glTF",
    )?;
    document["buffers"][0]
        .as_object_mut()
        .ok_or("Invalid source buffer")?
        .remove("uri");
    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: std::borrow::Cow::Owned(serde_json::to_vec(&document).map_err(err)?),
        bin: Some(std::borrow::Cow::Owned(bin)),
    }
    .to_vec()
    .map_err(err)?;
    let report = validate_bytes(&glb)?;
    let temporary = directory.join(".fiote.glb.tmp");
    fs::write(&temporary, glb).map_err(err)?;
    fs::rename(temporary, directory.join("fiote.glb")).map_err(err)?;
    println!("{}", serde_json::to_string_pretty(&report).map_err(err)?);
    render(directory)
}

fn parsed(bytes: &[u8]) -> Result<gltf::Gltf> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(err)?;
    let blob = gltf.blob.as_ref().ok_or("Missing embedded GLB buffer")?;
    for buffer in gltf.buffers() {
        require(
            matches!(buffer.source(), gltf::buffer::Source::Bin) && buffer.length() <= blob.len(),
            "External or truncated GLB buffer",
        )?;
    }
    for view in gltf.views() {
        require(
            view.offset()
                .checked_add(view.length())
                .is_some_and(|n| n <= blob.len()),
            "Buffer view exceeds the embedded data",
        )?;
    }
    Ok(gltf)
}

fn worlds(
    gltf: &gltf::Gltf,
    animation: Option<gltf::Animation<'_>>,
    time: f32,
) -> Result<Vec<Mat4>> {
    let blob = gltf.blob.as_deref().ok_or("Missing buffer")?;
    let mut transforms: Vec<_> = gltf
        .nodes()
        .map(|n| {
            let (t, r, s) = n.transform().decomposed();
            (Vec3::from(t), Quat::from_array(r), Vec3::from(s))
        })
        .collect();
    if let Some(animation) = animation {
        for channel in animation.channels() {
            let reader = channel.reader(|_| Some(blob));
            let times: Vec<_> = reader
                .read_inputs()
                .ok_or("Missing animation times")?
                .collect();
            require(
                !times.is_empty()
                    && times.iter().all(|t| t.is_finite())
                    && times.windows(2).all(|w| w[0] < w[1]),
                "Invalid animation timeline",
            )?;
            let last = times.partition_point(|t| *t <= time).saturating_sub(1);
            let next = (last + 1).min(times.len() - 1);
            let blend = if next == last
                || channel.sampler().interpolation() == gltf::animation::Interpolation::Step
            {
                0.0
            } else {
                ((time - times[last]) / (times[next] - times[last])).clamp(0.0, 1.0)
            };
            require(
                channel.sampler().interpolation() != gltf::animation::Interpolation::CubicSpline,
                "Cubic animation requires an explicit native interpolator",
            )?;
            let node = &mut transforms[channel.target().node().index()];
            match reader.read_outputs().ok_or("Missing animation output")? {
                ReadOutputs::Translations(values) => {
                    let values: Vec<_> = values.map(Vec3::from).collect();
                    require(
                        values.len() == times.len(),
                        "Translation sample count mismatch",
                    )?;
                    node.0 = values[last].lerp(values[next], blend);
                }
                ReadOutputs::Rotations(values) => {
                    let values: Vec<_> = values.into_f32().map(Quat::from_array).collect();
                    require(
                        values.len() == times.len(),
                        "Rotation sample count mismatch",
                    )?;
                    node.1 = values[last].slerp(values[next], blend).normalize();
                }
                ReadOutputs::Scales(values) => {
                    let values: Vec<_> = values.map(Vec3::from).collect();
                    require(values.len() == times.len(), "Scale sample count mismatch")?;
                    node.2 = values[last].lerp(values[next], blend);
                }
                ReadOutputs::MorphTargetWeights(_) => {
                    return Err("Unsupported morph animation".into());
                }
            }
        }
    }
    let mut parents = vec![None; transforms.len()];
    for n in gltf.nodes() {
        for child in n.children() {
            require(
                parents[child.index()].replace(n.index()).is_none(),
                "Node has multiple parents",
            )?;
        }
    }
    let local: Vec<_> = transforms
        .into_iter()
        .map(|(t, r, s)| Mat4::from_scale_rotation_translation(s, r, t))
        .collect();
    let mut result = Vec::with_capacity(local.len());
    for index in 0..local.len() {
        let mut world = local[index];
        let mut parent = parents[index];
        let mut depth = 0;
        while let Some(index) = parent {
            depth += 1;
            require(depth <= local.len(), "Cyclic node hierarchy")?;
            world = local[index] * world;
            parent = parents[index];
        }
        require(world.is_finite(), "Non-finite node transform")?;
        result.push(world);
    }
    Ok(result)
}

fn target(kind: &str, phase: f32, limb: usize) -> Vec3 {
    let transition = match kind {
        "sit_down" => Some(("idle", "sit_idle")),
        "stand_up" => Some(("sit_idle", "idle")),
        "lie_down" => Some(("idle", "rest")),
        "get_up" => Some(("rest", "idle")),
        "fall_asleep" => Some(("rest", "sleep")),
        "wake_up" => Some(("sleep", "rest")),
        _ => None,
    };
    if let Some((first, last)) = transition {
        return target(first, 0.0, limb)
            .lerp(target(last, 0.0, limb), phase * phase * (3.0 - 2.0 * phase));
    }
    let x = if limb % 2 == 0 { 0.28 } else { -0.28 };
    let front = limb < 2;
    let mut y = if front { -0.42 } else { 0.34 };
    let mut z = 0.14;
    if matches!(kind, "sit_idle" | "groom_paw") {
        y = if front { -0.40 } else { 0.12 };
    }
    if matches!(kind, "rest" | "sleep" | "groom_flank") {
        y = if front { -0.72 } else { 0.38 };
    }
    if kind == "groom_paw" && limb == 0 {
        return Vec3::new(0.18, 1.10, 0.70);
    }
    if matches!(kind, "walk" | "turn_left" | "turn_right") {
        let step = (phase + [0.0, 0.5, 0.25, 0.75][limb]) % 1.0;
        let swing = ((step - 0.65) / 0.35).max(0.0);
        z += 0.08 * (swing * std::f32::consts::PI).sin();
        if kind == "walk" {
            y += if step <= 0.65 {
                -0.09 + 0.18 * step / 0.65
            } else {
                0.09 - 0.18 * swing * swing * (3.0 - 2.0 * swing)
            };
        } else {
            let half = 0.55 * 1.5 * 0.65 / 2.0;
            let angle = (if kind == "turn_left" { 1.0 } else { -1.0 })
                * if step <= 0.65 {
                    half - 0.55 * 1.5 * step
                } else {
                    -half + 2.0 * half * swing * swing * (3.0 - 2.0 * swing)
                };
            let (s, c) = angle.sin_cos();
            return Vec3::new(x * c - y * s, z, -(x * s + y * c));
        }
    }
    if kind == "stretch" && front {
        y -= 0.18 * (phase * std::f32::consts::PI).sin();
    }
    Vec3::new(x, z, -y)
}

fn validate(path: &Path) -> Result<Value> {
    validate_bytes(&fs::read(path).map_err(err)?)
}

fn validate_bytes(bytes: &[u8]) -> Result<Value> {
    let gltf = parsed(bytes)?;
    let blob = gltf.blob.as_deref().ok_or("Missing buffer")?;
    let mut vertices = 0;
    let mut triangles = 0;
    let mut blended = 0;
    require(gltf.skins().count() == 1, "Expected one Fiote skin")?;
    let skin = gltf.skins().next().ok_or("Missing skin")?;
    let joint_count = skin.joints().count();
    let inverse: Vec<_> = skin
        .reader(|_| Some(blob))
        .read_inverse_bind_matrices()
        .ok_or("Missing bind matrices")?
        .collect();
    require(
        inverse.len() == joint_count && inverse.iter().flatten().flatten().all(|v| v.is_finite()),
        "Invalid inverse bind matrices",
    )?;
    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            require(
                primitive.mode() == gltf::mesh::Mode::Triangles,
                "Expected triangle meshes",
            )?;
            let reader = primitive.reader(|_| Some(blob));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("Missing positions")?
                .collect();
            let normals: Vec<_> = reader.read_normals().ok_or("Missing normals")?.collect();
            let weights: Vec<_> = reader
                .read_weights(0)
                .ok_or("Missing skin weights")?
                .into_f32()
                .collect();
            let joints: Vec<_> = reader
                .read_joints(0)
                .ok_or("Missing joints")?
                .into_u16()
                .collect();
            require(
                !positions.is_empty()
                    && positions.len() == normals.len()
                    && weights.len() == positions.len()
                    && joints.len() == positions.len(),
                "Vertex attribute count mismatch",
            )?;
            require(
                positions
                    .iter()
                    .chain(&normals)
                    .flatten()
                    .all(|v| v.is_finite()),
                "Non-finite mesh coordinate",
            )?;
            for (weights, joints) in weights.iter().zip(&joints) {
                require(
                    weights.iter().all(|v| v.is_finite() && *v >= 0.0)
                        && (weights.iter().sum::<f32>() - 1.0).abs() < 0.001,
                    "Skin weights must sum to one",
                )?;
                require(
                    joints.iter().all(|j| usize::from(*j) < joint_count),
                    "Skin joint out of bounds",
                )?;
                blended += usize::from(weights.iter().filter(|w| **w > 0.00001).count() > 1);
            }
            let indices: Vec<_> = reader
                .read_indices()
                .ok_or("Missing triangle indices")?
                .into_u32()
                .collect();
            require(
                indices.len() % 3 == 0 && indices.iter().all(|i| (*i as usize) < positions.len()),
                "Invalid triangle indices",
            )?;
            vertices += positions.len();
            triangles += indices.len() / 3;
        }
    }
    require(blended > 1000, "Expected a blended continuous coat")?;
    require(
        gltf.animations().count() == CLIPS.len(),
        "Animation clip count differs from the character contract",
    )?;
    let paws: Vec<_> = PAWS
        .iter()
        .map(|name| {
            gltf.nodes()
                .find(|n| n.name() == Some(&format!("{name}_paw")))
                .map(|n| n.index())
                .ok_or_else(|| format!("Missing {name} paw"))
        })
        .collect::<Result<_>>()?;
    let mut maximum_error = 0.0f32;
    let mut maximum_loop = 0.0f32;
    let mut samples = 0;
    for &(name, duration) in CLIPS {
        let animation = gltf
            .animations()
            .find(|a| a.name() == Some(name))
            .ok_or_else(|| format!("Missing animation {name}"))?;
        let end = animation
            .channels()
            .filter_map(|c| c.reader(|_| Some(blob)).read_inputs())
            .flatten()
            .fold(0.0f32, f32::max);
        require(
            (end - duration).abs() < 0.001,
            format!("Unexpected duration for {name}: {end}"),
        )?;
        let first = worlds(&gltf, Some(animation.clone()), 0.0)?;
        let frames = (duration * 24.0).round() as usize;
        for frame in 0..=frames {
            let phase = frame as f32 / frames as f32;
            let world = worlds(&gltf, Some(animation.clone()), phase * duration)?;
            for (limb, &node) in paws.iter().enumerate() {
                let position = world[node].transform_point3(Vec3::ZERO);
                require(
                    position.y > 0.09,
                    format!("{name} frame {frame}: paw below ground"),
                )?;
                let error = position.distance(target(name, phase, limb));
                maximum_error = maximum_error.max(error);
                require(
                    error < 0.03,
                    format!("{name} frame {frame} {} target error {error}", PAWS[limb]),
                )?;
                if frame == frames
                    && matches!(
                        name,
                        "idle"
                            | "walk"
                            | "turn_left"
                            | "turn_right"
                            | "sit_idle"
                            | "rest"
                            | "sleep"
                    )
                {
                    let seam = position.distance(first[node].transform_point3(Vec3::ZERO));
                    maximum_loop = maximum_loop.max(seam);
                    require(seam < 0.001, format!("{name}: paw loop seam {seam}"))?;
                }
            }
            samples += 1;
        }
    }
    let image = gltf.images().next().ok_or("Missing fur texture")?;
    let gltf::image::Source::View { view, .. } = image.source() else {
        return Err("Texture must be embedded".into());
    };
    let texture = image::load_from_memory(&blob[view.offset()..view.offset() + view.length()])
        .map_err(err)?;
    require(
        texture.width() == 128 && texture.height() == 128,
        "Expected a 128px fur texture",
    )?;
    Ok(
        json!({"vertices":vertices,"triangles":triangles,"blended_vertices":blended,"maximum_paw_target_error_m":maximum_error,"maximum_paw_loop_seam_m":maximum_loop,"animation_clips":CLIPS.len(),"animation_samples":samples,"fur_texture_size":[128,128],"source":"native glTF with baked skeletal animations"}),
    )
}

fn render(directory: &Path) -> Result<()> {
    let bytes = fs::read(directory.join("fiote.glb")).map_err(err)?;
    let gltf = parsed(&bytes)?;
    let blob = gltf.blob.as_deref().ok_or("Missing buffer")?;
    let animation = gltf
        .animations()
        .find(|a| a.name() == Some("sit_idle"))
        .ok_or("Missing sit_idle")?;
    let world = worlds(&gltf, Some(animation), 0.0)?;
    let eye = Vec3::new(0.0, 1.6, 5.0);
    let vp = glam::camera::rh::proj::opengl::orthographic(-1.325, 1.325, -1.325, 1.325, 0.1, 20.0)
        * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 0.98, 0.05), Vec3::Y);
    let size = 1024usize;
    let mut image = RgbaImage::new(size as u32, size as u32);
    let mut depth = vec![f32::INFINITY; size * size];
    for node in gltf.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        let skin = node.skin().ok_or("Mesh has no skin")?;
        let inverse: Vec<_> = skin
            .reader(|_| Some(blob))
            .read_inverse_bind_matrices()
            .ok_or("Missing bind matrices")?
            .map(|m| Mat4::from_cols_array_2d(&m))
            .collect();
        let matrices: Vec<_> = skin
            .joints()
            .zip(&inverse)
            .map(|(j, m)| world[j.index()] * *m)
            .collect();
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|_| Some(blob));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("Missing positions")?
                .map(Vec3::from)
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .ok_or("Missing normals")?
                .map(Vec3::from)
                .collect();
            let weights: Vec<_> = reader
                .read_weights(0)
                .ok_or("Missing weights")?
                .into_f32()
                .collect();
            let joints: Vec<_> = reader
                .read_joints(0)
                .ok_or("Missing joints")?
                .into_u16()
                .collect();
            let uv: Vec<_> = reader
                .read_tex_coords(0)
                .ok_or("Missing texture coordinates")?
                .into_f32()
                .map(Vec2::from)
                .collect();
            let mut points = Vec::new();
            let mut directions = Vec::new();
            let mut model_positions = Vec::new();
            for i in 0..positions.len() {
                let mut position = Vec3::ZERO;
                let mut normal = Vec3::ZERO;
                for k in 0..4 {
                    let m = matrices[joints[i][k] as usize];
                    position += m.transform_point3(positions[i]) * weights[i][k];
                    normal += m.transform_vector3(normals[i]) * weights[i][k];
                }
                let p = vp.project_point3(position);
                points.push(Vec3::new(
                    (p.x * 0.5 + 0.5) * size as f32,
                    (0.5 - p.y * 0.5) * size as f32,
                    p.z,
                ));
                directions.push(normal.normalize_or_zero());
                model_positions.push(position);
            }
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            let factor = Vec4::from(pbr.base_color_factor());
            let texture = if let Some(info) = pbr.base_color_texture() {
                let gltf::image::Source::View { view, .. } = info.texture().source().source()
                else {
                    return Err("External texture".into());
                };
                Some(
                    image::load_from_memory(&blob[view.offset()..view.offset() + view.length()])
                        .map_err(err)?
                        .to_rgba8(),
                )
            } else {
                None
            };
            let indices: Vec<_> = reader
                .read_indices()
                .ok_or("Missing indices")?
                .into_u32()
                .collect();
            for face in indices.chunks_exact(3) {
                let ids = [face[0] as usize, face[1] as usize, face[2] as usize];
                let [a, b, c] = ids.map(|i| points[i]);
                let area = edge(a.truncate(), b.truncate(), c.truncate());
                if area.abs() < 0.00001 || (!material.double_sided() && area >= 0.0) {
                    continue;
                }
                let min = a.min(b).min(c).truncate().floor().max(Vec2::ZERO);
                let max = a
                    .max(b)
                    .max(c)
                    .truncate()
                    .ceil()
                    .min(Vec2::splat((size - 1) as f32));
                for y in min.y as usize..=max.y as usize {
                    for x in min.x as usize..=max.x as usize {
                        let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                        let bary = Vec3::new(
                            edge(b.truncate(), c.truncate(), p),
                            edge(c.truncate(), a.truncate(), p),
                            edge(a.truncate(), b.truncate(), p),
                        ) / area;
                        if bary.min_element() < -0.00001 {
                            continue;
                        }
                        let z = bary.dot(Vec3::new(a.z, b.z, c.z));
                        if z >= depth[y * size + x] {
                            continue;
                        }
                        depth[y * size + x] = z;
                        let interpolate = |values: &[Vec3]| {
                            values[ids[0]] * bary.x
                                + values[ids[1]] * bary.y
                                + values[ids[2]] * bary.z
                        };
                        let n = interpolate(&directions).normalize_or_zero();
                        let position = interpolate(&model_positions);
                        let illumination = 0.35
                            + 0.45
                                * n.dot((Vec3::new(-3.0, 5.0, 4.0) - position).normalize())
                                    .max(0.0)
                            + 0.25
                                * n.dot((Vec3::new(3.0, 3.0, 2.0) - position).normalize())
                                    .max(0.0);
                        let mut color = factor.truncate();
                        if let Some(texture) = &texture {
                            let tex =
                                uv[ids[0]] * bary.x + uv[ids[1]] * bary.y + uv[ids[2]] * bary.z;
                            let px = texture.get_pixel(
                                ((tex.x.rem_euclid(1.0) * texture.width() as f32) as u32)
                                    .min(texture.width() - 1),
                                ((tex.y.rem_euclid(1.0) * texture.height() as f32) as u32)
                                    .min(texture.height() - 1),
                            );
                            color *= Vec3::new(
                                px[0] as f32 / 255.0,
                                px[1] as f32 / 255.0,
                                px[2] as f32 / 255.0,
                            )
                            .powf(2.2);
                        }
                        color = (color * illumination)
                            .clamp(Vec3::ZERO, Vec3::ONE)
                            .powf(1.0 / 2.2)
                            * 255.0;
                        image.put_pixel(
                            x as u32,
                            y as u32,
                            Rgba([
                                color.x as u8,
                                color.y as u8,
                                color.z as u8,
                                (factor.w * 255.0) as u8,
                            ]),
                        );
                    }
                }
            }
        }
    }
    require(
        image.pixels().filter(|p| p[3] > 0).count() > 10000,
        "Portrait renderer produced an empty image",
    )?;
    let image = image::imageops::resize(&image, 512, 512, image::imageops::FilterType::Lanczos3);
    image.save(directory.join("fiote-seated.png")).map_err(err)
}

fn edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_paw_targets_close_loops_and_transitions() {
        for name in [
            "walk",
            "turn_left",
            "turn_right",
            "idle",
            "sit_idle",
            "rest",
            "sleep",
        ] {
            for limb in 0..4 {
                assert!(target(name, 0.0, limb).distance(target(name, 1.0, limb)) < 0.00001);
            }
        }
        assert_eq!(target("sit_down", 1.0, 0), target("sit_idle", 0.0, 0));
        assert_eq!(target("stand_up", 1.0, 2), target("idle", 0.0, 2));
    }
    #[test]
    fn truncated_assets_are_rejected() {
        assert!(validate_bytes(b"glTF").is_err());
    }
    fn character_bytes() -> Vec<u8> {
        let path = std::env::var_os("LINCE_FIOTE_TEST_ASSET")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../institute/assets/characters/fiote/fiote.glb")
            });
        fs::read(path).unwrap()
    }
    #[test]
    fn exported_character_satisfies_the_native_rig_contract() {
        let report = validate_bytes(&character_bytes()).unwrap();
        assert_eq!(report["animation_clips"], 17);
        assert_eq!(report["animation_samples"], 1181);
        assert!(report["maximum_paw_target_error_m"].as_f64().unwrap() < 0.03);
    }
    #[test]
    fn invalid_skin_weights_are_rejected() {
        let mut bytes = character_bytes();
        let gltf = parsed(&bytes).unwrap();
        let accessor = gltf
            .meshes()
            .next()
            .unwrap()
            .primitives()
            .next()
            .unwrap()
            .get(&gltf::Semantic::Weights(0))
            .unwrap();
        let offset = accessor.view().unwrap().offset() + accessor.offset();
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let binary_offset = 20 + json_length + 8 + offset;
        drop(gltf);
        bytes[binary_offset..binary_offset + 4].copy_from_slice(&2.0f32.to_le_bytes());
        assert!(validate_bytes(&bytes).unwrap_err().contains("weights"));
    }
}
