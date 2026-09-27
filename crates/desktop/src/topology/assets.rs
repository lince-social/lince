use super::Spatial;
use crate::{canvas::CanvasItem, workspace::WorkspaceMember};
use bevy::{gltf::Gltf, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_PACKAGE: u64 = 256 * 1024 * 1024;

#[derive(Resource)]
pub struct AssetDirectory(pub PathBuf);

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct ImportedAsset {
    pub id: String,
    pub name: String,
    pub file: String,
    pub scale: f32,
}

impl ImportedAsset {
    pub fn valid(&self) -> bool {
        self.id.len() == 32
            && self.id.bytes().all(|b| b.is_ascii_hexdigit())
            && matches!(
                self.file.as_str(),
                "source.gltf" | "source.glb" | "source.gcloud"
            )
            && self.name.len() <= 512
            && self.scale.is_finite()
            && self.scale > 0.0
            && self.scale <= 100_000.0
    }
}

#[derive(Component)]
pub struct Ready;

pub fn effective_scale(world: &World, entity: Entity, asset: &ImportedAsset) -> f32 {
    asset.scale
        * world
            .get::<crate::area_effects::AreaScale>(entity)
            .map_or(1.0, |scale| scale.0)
}

#[derive(Component)]
pub struct Loading(pub Handle<Gltf>);

#[derive(Component)]
pub struct ImportedScene(pub Entity);

#[derive(Component, Clone, Copy)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

pub fn mesh_parts(
    world: &World,
    entity: Entity,
) -> Option<Vec<(Entity, Handle<Mesh>, GlobalTransform)>> {
    let scene = world.get::<ImportedScene>(entity)?.0;
    let instance = world.get::<bevy::world_serialization::WorldInstance>(scene)?;
    if !world
        .get_resource::<WorldInstanceSpawner>()?
        .instance_is_ready(**instance)
    {
        return None;
    }
    let mut pending = vec![(scene, GlobalTransform::IDENTITY)];
    let mut parts = Vec::new();
    while let Some((child, parent)) = pending.pop() {
        let transform = parent * world.get::<Transform>(child).copied().unwrap_or_default();
        if let Some(mesh) = world.get::<Mesh3d>(child) {
            parts.push((child, mesh.0.clone(), transform));
        }
        if let Some(children) = world.get::<Children>(child) {
            pending.extend(children.iter().map(|child| (child, transform)));
        }
    }
    Some(parts)
}

#[derive(Resource, Default)]
pub struct Imports(Vec<Pending>);

struct Pending {
    root: Entity,
    workspace: u64,
    position: bevy::math::DVec2,
    elevation: f64,
    cancelled: Arc<AtomicBool>,
    result: std::sync::Mutex<mpsc::Receiver<Result<ImportedAsset, String>>>,
}

fn problem(message: &str) -> io::Error {
    io::Error::other(message)
}

fn document(bytes: &[u8], glb: bool) -> io::Result<serde_json::Value> {
    let json = if glb {
        if bytes.len() < 20
            || &bytes[..4] != b"glTF"
            || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 2
            || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize != bytes.len()
            || &bytes[16..20] != b"JSON"
        {
            return Err(problem("Invalid GLB header"));
        }
        let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        bytes
            .get(
                20..20usize
                    .checked_add(length)
                    .ok_or_else(|| problem("Invalid GLB length"))?,
            )
            .ok_or_else(|| problem("Invalid GLB JSON length"))?
    } else {
        bytes
    };
    if json.len() > 16 * 1024 * 1024 {
        return Err(problem("glTF document is too large"));
    }
    let value: serde_json::Value = serde_json::from_slice(json).map_err(io::Error::other)?;
    if value.pointer("/asset/version").and_then(|v| v.as_str()) != Some("2.0") {
        return Err(problem("Only glTF 2.0 is supported"));
    }
    if value
        .get("meshes")
        .and_then(|v| v.as_array())
        .is_none_or(|v| v.is_empty() || v.len() > 10_000)
    {
        return Err(problem("glTF must contain between 1 and 10000 meshes"));
    }
    Ok(value)
}

fn resource_path(uri: &str) -> io::Result<PathBuf> {
    let mut bytes = Vec::new();
    let mut input = uri.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let a = input.next().and_then(|c| (c as char).to_digit(16));
            let b = input.next().and_then(|c| (c as char).to_digit(16));
            let (Some(a), Some(b)) = (a, b) else {
                return Err(problem("Invalid resource path"));
            };
            bytes.push((a * 16 + b) as u8);
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8(bytes).map_err(io::Error::other)?;
    if path.is_empty()
        || path.contains([':', '\\', '\0', '?', '#'])
        || !Path::new(&path)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(problem(
            "Import resources must be relative files inside the selected directory",
        ));
    }
    let path: PathBuf = Path::new(&path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect();
    if path.as_os_str().is_empty() {
        return Err(problem("Resource path must name a file"));
    }
    Ok(path)
}

fn resources(value: &serde_json::Value, found: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key == "uri" {
                    let uri = value
                        .as_str()
                        .ok_or_else(|| problem("Invalid resource URI"))?;
                    if !uri.starts_with("data:") {
                        found.insert(resource_path(uri)?);
                    }
                } else {
                    resources(value, found)?;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                resources(value, found)?;
            }
        }
        _ => {}
    }
    if found.len() > 1024 {
        return Err(problem("Too many import resources"));
    }
    Ok(())
}

pub fn copy_import(source: &Path, directory: &Path) -> io::Result<ImportedAsset> {
    copy_package(source, directory, &AtomicBool::new(false))
}

fn copy_cloud(
    source: &Path,
    directory: &Path,
    cancelled: &AtomicBool,
) -> io::Result<ImportedAsset> {
    let source = source.canonicalize()?;
    let parent = source
        .parent()
        .ok_or_else(|| problem("Missing source directory"))?;
    std::fs::create_dir_all(directory)?;
    let staging = tempfile::Builder::new()
        .prefix("import-")
        .tempdir_in(directory)?;
    let mut inputs = vec![(
        source.clone(),
        "source.gcloud".to_owned(),
        super::splats::codec::MAX_FILE_BYTES,
    )];
    for name in [
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "license.txt",
        "COPYING",
        "CREDITS",
        "CREDITS.txt",
        "credits.txt",
    ] {
        let path = parent.join(name);
        if path.is_file() {
            let path = path.canonicalize()?;
            if !path.starts_with(parent) {
                return Err(problem("Credits leave the import directory"));
            }
            inputs.push((path, name.to_owned(), 1024 * 1024));
        }
    }
    for (input, name, limit) in inputs {
        let mut input = std::fs::File::open(input)?;
        if !input.metadata()?.is_file() || input.metadata()?.len() > limit {
            return Err(problem(
                "Gaussian asset exceeds 1 GiB or its credits exceed 1 MiB",
            ));
        }
        let mut output = std::fs::File::create(staging.path().join(name))?;
        let mut total = 0;
        let mut buffer = [0; 65536];
        loop {
            check_cancelled(cancelled)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > limit {
                return Err(problem("Gaussian import exceeds its size limit"));
            }
            output.write_all(&buffer[..count])?;
        }
    }
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
    check_cancelled(cancelled)?;
    std::fs::rename(staging.path(), directory.join(&id))?;
    Ok(ImportedAsset {
        id,
        name: source
            .file_name()
            .unwrap()
            .to_string_lossy()
            .chars()
            .take(120)
            .collect(),
        file: "source.gcloud".into(),
        scale: 100.0,
    })
}

fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(problem("Import cancelled"))
    } else {
        Ok(())
    }
}

fn copy_package(
    source: &Path,
    directory: &Path,
    cancelled: &AtomicBool,
) -> io::Result<ImportedAsset> {
    check_cancelled(cancelled)?;
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "gcloud" {
        return copy_cloud(source, directory, cancelled);
    }
    if !matches!(extension.as_str(), "gltf" | "glb") {
        return Err(problem(
            "Choose a .gltf, .glb, or .gcloud file. Convert Gaussian PLY before importing.",
        ));
    }
    if std::fs::metadata(source)?.len() > MAX_FILE {
        return Err(problem("Import file exceeds 128 MiB"));
    }
    let source = source.canonicalize()?;
    let parent = source
        .parent()
        .ok_or_else(|| problem("Missing source directory"))?;
    let bytes = std::fs::read(&source)?;
    check_cancelled(cancelled)?;
    let value = document(&bytes, extension == "glb")?;
    let mut files = BTreeSet::new();
    resources(&value, &mut files)?;
    for name in [
        "LICENSE",
        "LICENSE.txt",
        "LICENSE.md",
        "COPYING",
        "CREDITS",
        "CREDITS.txt",
    ] {
        if parent.join(name).is_file() {
            files.insert(PathBuf::from(name));
        }
    }
    let mut total = bytes.len() as u64;
    let mut inputs = Vec::new();
    for file in files {
        let path = parent.join(&file).canonicalize()?;
        if !path.starts_with(parent) || !path.is_file() {
            return Err(problem("Resource leaves the import directory"));
        }
        let length = std::fs::metadata(&path)?.len();
        total = total
            .checked_add(length)
            .ok_or_else(|| problem("Import is too large"))?;
        if length > MAX_FILE || total > MAX_PACKAGE {
            return Err(problem("Import resources exceed 256 MiB"));
        }
        inputs.push((file, path));
    }
    std::fs::create_dir_all(directory)?;
    let staging = tempfile::Builder::new()
        .prefix("import-")
        .tempdir_in(directory)?;
    let file = format!("source.{extension}");
    if inputs
        .iter()
        .any(|(relative, _)| relative == Path::new(&file))
    {
        return Err(problem(
            "A resource conflicts with the imported document name",
        ));
    }
    std::fs::write(staging.path().join(&file), &bytes)?;
    let mut copied = bytes.len() as u64;
    for (relative, path) in inputs {
        let target = staging.path().join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut input = std::fs::File::open(path)?;
        let mut output = std::fs::File::create(target)?;
        let mut buffer = [0; 65536];
        loop {
            check_cancelled(cancelled)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            copied += count as u64;
            if copied > MAX_PACKAGE {
                return Err(problem("Import resources exceed 256 MiB"));
            }
            output.write_all(&buffer[..count])?;
        }
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    check_cancelled(cancelled)?;
    std::fs::rename(staging.path(), directory.join(&id))?;
    Ok(ImportedAsset {
        id,
        file,
        name: source
            .file_name()
            .unwrap()
            .to_string_lossy()
            .chars()
            .take(120)
            .collect(),
        scale: 100.0,
    })
}

pub fn import(world: &mut World, root: Entity, source: PathBuf) -> Result<(), String> {
    if world
        .get_resource::<Imports>()
        .is_some_and(|imports| imports.0.len() >= 4)
    {
        return Err("Wait for the current imports to finish".into());
    }
    let directory = world
        .get_resource::<AssetDirectory>()
        .ok_or("Asset storage is unavailable")?
        .0
        .clone();
    let spaces = world
        .get::<crate::workspace::Workspaces>(root)
        .ok_or("Workspace is unavailable")?;
    let workspace = spaces.active;
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .ok_or("Canvas is unavailable")?
        .center;
    let elevation = world
        .get::<super::view::View>(root)
        .map_or(0.0, |view| view.plane);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (sender, receiver) = mpsc::channel();
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    std::thread::Builder::new()
        .name("topology-import".into())
        .spawn(move || {
            let result =
                copy_package(&source, &directory, &worker_cancelled).map_err(|e| e.to_string());
            if let Err(mpsc::SendError(Ok(asset))) = sender.send(result) {
                let _ = std::fs::remove_dir_all(directory.join(asset.id));
            }
            if let Some(wake) = wake {
                wake.ring();
            }
        })
        .map_err(|e| e.to_string())?;
    world.init_resource::<Imports>();
    world.resource_mut::<Imports>().0.push(Pending {
        root,
        workspace,
        position,
        elevation,
        cancelled,
        result: std::sync::Mutex::new(receiver),
    });
    Ok(())
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: bevy::math::DVec2,
    asset: ImportedAsset,
) -> Entity {
    let elevation = world
        .get::<super::view::View>(root)
        .map_or(0.0, |view| view.plane);
    let entity = world
        .spawn((
            CanvasItem {
                position,
                size: Vec2::splat(100.0),
            },
            Spatial {
                elevation,
                world_pinned: true,
                ..default()
            },
            asset.clone(),
            WorkspaceMember(workspace),
            ChildOf(root),
            Transform::default(),
            Visibility::default(),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
        ))
        .id();
    if let Some(server) = world.get_resource::<AssetServer>() {
        if super::splats::is_splat(&asset) {
            super::splats::load(
                world,
                entity,
                format!("topology://{}/{}", asset.id, asset.file),
            );
            return entity;
        }
        let handle = server
            .load_builder()
            .with_settings(|settings: &mut bevy::gltf::GltfLoaderSettings| {
                settings.load_cameras = false;
                settings.load_lights = false;
                settings.load_animations = false;
            })
            .load(format!("topology://{}/{}", asset.id, asset.file));
        world.entity_mut(entity).insert(Loading(handle));
    }
    entity
}

pub fn update(world: &mut World) {
    super::splats::update(world);
    if world.contains_resource::<Imports>() {
        let mut imports = world.remove_resource::<Imports>().unwrap();
        imports.0.retain(|pending| {
            let result = pending.result.lock().unwrap().try_recv();
            match result {
                Ok(Ok(asset)) => {
                    if !pending.cancelled.load(Ordering::Acquire)
                        && world
                            .get::<crate::workspace::Workspaces>(pending.root)
                            .is_some_and(|s| s.entries.iter().any(|s| s.id == pending.workspace))
                    {
                        let entity = spawn(
                            world,
                            pending.root,
                            pending.workspace,
                            pending.position,
                            asset,
                        );
                        world.get_mut::<Spatial>(entity).unwrap().elevation = pending.elevation;
                        if world
                            .get::<ImportedAsset>(entity)
                            .is_some_and(super::splats::is_splat)
                        {
                            world.entity_mut(entity).insert(super::splats::FrameOnReady);
                        }
                    } else if let Some(directory) = world.get_resource::<AssetDirectory>() {
                        let _ = std::fs::remove_dir_all(directory.0.join(asset.id));
                    }
                    false
                }
                Ok(Err(error)) => {
                    if !pending.cancelled.load(Ordering::Acquire) {
                        crate::notifications::report(world, "Topology", &error);
                    }
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(_) => false,
            }
        });
        world.insert_resource(imports);
    }
    if !world.contains_resource::<Assets<Gltf>>() {
        return;
    }
    let loading: Vec<_> = world
        .query::<(Entity, &Loading)>()
        .iter(world)
        .map(|(e, h)| (e, h.0.clone()))
        .collect();
    for (entity, handle) in loading {
        if world
            .resource::<AssetServer>()
            .is_loaded_with_dependencies(handle.id())
            && let Some(gltf) = world.resource::<Assets<Gltf>>().get(&handle)
        {
            let scene = gltf
                .default_scene
                .clone()
                .or_else(|| gltf.scenes.first().cloned());
            if let Some(scene) = scene {
                let scene = world
                    .spawn((
                        WorldAssetRoot(scene),
                        Transform::default(),
                        Visibility::default(),
                        ChildOf(entity),
                    ))
                    .id();
                world
                    .entity_mut(entity)
                    .insert(ImportedScene(scene))
                    .remove::<Loading>();
            } else {
                crate::notifications::report(world, "Topology", "The imported file has no scene");
                discard(world, entity);
            }
        } else if matches!(
            world.resource::<AssetServer>().load_state(handle.id()),
            bevy::asset::LoadState::Failed(_)
        ) || matches!(
            world
                .resource::<AssetServer>()
                .recursive_dependency_load_state(handle.id()),
            bevy::asset::RecursiveDependencyLoadState::Failed(_)
        ) {
            crate::notifications::report(world, "Topology", "Could not load glTF resources");
            discard(world, entity);
        }
    }
    let imports: Vec<_> = world
        .query::<(
            Entity,
            &ImportedAsset,
            &CanvasItem,
            &ChildOf,
            &WorkspaceMember,
        )>()
        .iter(world)
        .map(|(e, a, item, p, m)| (e, a.scale, *item, p.parent(), m.0))
        .collect();
    for (entity, scale, item, root, workspace) in imports {
        if world.get::<Bounds>(entity).is_none()
            && let Some(parts) = mesh_parts(world, entity)
        {
            if parts.is_empty() {
                crate::notifications::report(
                    world,
                    "Topology",
                    "The selected scene contains no meshes",
                );
                discard(world, entity);
                continue;
            }
            use bevy::camera::primitives::MeshAabb;
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for (mesh_entity, handle, transform) in parts {
                world.entity_mut(mesh_entity).insert(Pickable::IGNORE);
                if let Some(bounds) = world
                    .resource::<Assets<Mesh>>()
                    .get(&handle)
                    .and_then(MeshAabb::compute_aabb)
                {
                    for x in [-1.0, 1.0] {
                        for y in [-1.0, 1.0] {
                            for z in [-1.0, 1.0] {
                                let point = transform.transform_point(
                                    Vec3::from(bounds.center)
                                        + Vec3::from(bounds.half_extents) * Vec3::new(x, y, z),
                                );
                                min = min.min(point);
                                max = max.max(point);
                            }
                        }
                    }
                }
            }
            if min.is_finite() && max.is_finite() {
                world.entity_mut(entity).insert(Bounds { min, max });
            } else {
                crate::notifications::report(world, "Topology", "Imported mesh has invalid bounds");
                discard(world, entity);
                continue;
            }
        }
        if let Some(bounds) = world.get::<Bounds>(entity) {
            let size = (bounds.max - bounds.min) * scale;
            let size = Vec2::new(size.x.max(1.0), size.z.max(1.0));
            if world.get::<CanvasItem>(entity).unwrap().size != size {
                world.get_mut::<CanvasItem>(entity).unwrap().size = size;
            }
        }
        let effective = effective_scale(world, entity, world.get::<ImportedAsset>(entity).unwrap());
        let visual_only = super::splats::is_splat(world.get::<ImportedAsset>(entity).unwrap());
        if !visual_only {
            match super::physics::imported_shape(world, entity, effective) {
                Ok(Some(_)) => {
                    if world.get::<Ready>(entity).is_none() {
                        world.entity_mut(entity).insert(Ready);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    crate::notifications::report(world, "Topology", error);
                    discard(world, entity);
                    continue;
                }
            }
        }
        let spatial = super::spatial(world, entity);
        let visible = world
            .get::<crate::workspace::Workspaces>(root)
            .is_some_and(|s| s.active == workspace)
            && world.get::<Ready>(entity).is_some();
        let origin = super::presentation::origin(world, root);
        let transform = Transform {
            translation: (spatial.position(item.position) - origin).as_vec3(),
            rotation: spatial.rotation().as_quat(),
            scale: Vec3::splat(effective),
        };
        world
            .get_mut::<Transform>(entity)
            .unwrap()
            .set_if_neq(transform);
        world
            .get_mut::<Visibility>(entity)
            .unwrap()
            .set_if_neq(if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
        world.entity_mut(entity).remove::<Node>();
    }
}

pub fn discard(world: &mut World, entity: Entity) {
    let asset = world.get::<ImportedAsset>(entity).cloned();
    if let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) {
        if let Some(mut selection) = world.get_mut::<crate::canvas_selection::SandSelection>(root) {
            selection.0.retain(|selected| *selected != entity);
        }
        if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
            if inspection.selected == Some(entity) {
                inspection.selected = None;
            }
        }
    }
    world.despawn(entity);
    if let Some(asset) = asset {
        let used = world
            .query::<&ImportedAsset>()
            .iter(world)
            .any(|other| other.id == asset.id);
        if !used && asset.valid() {
            if let Some(directory) = world.get_resource::<AssetDirectory>() {
                let _ = std::fs::remove_dir_all(directory.0.join(asset.id));
            }
        }
    }
}

pub fn frame(world: &mut World, entity: Entity) {
    let Some(bounds) = world.get::<Bounds>(entity).copied() else {
        return;
    };
    let Some(asset) = world.get::<ImportedAsset>(entity) else {
        return;
    };
    let scale = effective_scale(world, entity, asset);
    let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
        return;
    };
    if world.get::<WorkspaceMember>(entity).is_some_and(|member| {
        world
            .get::<crate::workspace::Workspaces>(root)
            .is_some_and(|spaces| spaces.active != member.0)
    }) {
        return;
    }
    let Some(item) = world.get::<CanvasItem>(entity) else {
        return;
    };
    let placement = super::spatial(world, entity);
    let center = placement.position(item.position)
        + placement.rotation() * ((bounds.min + bounds.max) * (scale * 0.5)).as_dvec3();
    let radius = f64::from((bounds.max - bounds.min).length() * scale * 0.5).max(1.0);
    let viewport = world
        .get_resource::<super::presentation::SceneCamera>()
        .and_then(|c| world.get::<Camera>(c.0))
        .and_then(Camera::logical_viewport_size)
        .unwrap_or(Vec2::new(800.0, 640.0));
    if let Some(mut view) = world.get_mut::<super::view::View>(root) {
        let rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0);
        let half_fov =
            ((std::f32::consts::PI / 8.0).tan() * (viewport.x / viewport.y).min(1.0)).atan();
        view.position = (center
            + (rotation * Vec3::Z).as_dvec3() * (radius / f64::from(half_fov.sin()) * 1.1))
            .to_array();
    }
    if let Some(mut canvas) = world.get_mut::<crate::canvas::CanvasView>(root) {
        canvas.center = bevy::math::DVec2::new(center.x, center.z);
        canvas.set_zoom(f64::from(viewport.min_element()) / (radius * 2.2));
    }
}

pub fn cancel(world: &mut World, root: Entity) {
    let Some(workspace) = world
        .get::<crate::workspace::Workspaces>(root)
        .map(|s| s.active)
    else {
        return;
    };
    if let Some(imports) = world.get_resource::<Imports>() {
        for pending in &imports.0 {
            if pending.root == root && pending.workspace == workspace {
                pending.cancelled.store(true, Ordering::Release);
            }
        }
    }
    let entities: Vec<_> = world.query_filtered::<(Entity, &ChildOf, &WorkspaceMember), (With<ImportedAsset>, Without<Ready>)>()
        .iter(world).filter(|(_, parent, member)| parent.parent() == root && member.0 == workspace)
        .map(|(entity, _, _)| entity).collect();
    for entity in entities {
        discard(world, entity);
    }
}

pub fn duplicate(world: &mut World, entity: Entity) -> Option<Entity> {
    let asset = world.get::<ImportedAsset>(entity)?.clone();
    let root = world.get::<ChildOf>(entity)?.parent();
    let workspace = world.get::<WorkspaceMember>(entity)?.0;
    let item = *world.get::<CanvasItem>(entity)?;
    let placement = super::spatial(world, entity);
    let copy = spawn(
        world,
        root,
        workspace,
        item.position + bevy::math::DVec2::splat(40.0),
        asset,
    );
    world.entity_mut(copy).insert(placement);
    world.get_mut::<CanvasItem>(copy).unwrap().size = item.size;
    if let Some(bounds) = world.get::<Bounds>(entity).copied() {
        world.entity_mut(copy).insert(bounds);
    }
    Some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_discards_completed_packages_without_spawning() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let path = source.path().join("source.gltf");
        std::fs::write(&path, include_bytes!("../../fixtures/open_scene.gltf")).unwrap();
        assert!(copy_package(&path, destination.path(), &AtomicBool::new(true)).is_err());
        assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
        let asset = copy_import(&path, destination.path()).unwrap();
        let package = destination.path().join(&asset.id);
        let mut world = World::new();
        world.insert_resource(AssetDirectory(destination.path().into()));
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok(asset)).unwrap();
        world.insert_resource(Imports(vec![Pending {
            root,
            workspace: 1,
            position: bevy::math::DVec2::ZERO,
            elevation: 0.0,
            cancelled: Arc::new(AtomicBool::new(false)),
            result: std::sync::Mutex::new(receiver),
        }]));
        cancel(&mut world, root);
        update(&mut world);
        assert!(world.resource::<Imports>().0.is_empty());
        assert_eq!(world.query::<&ImportedAsset>().iter(&world).count(), 0);
        assert!(!package.exists());
    }

    #[test]
    fn copies_share_package_but_keep_independent_placement_and_pinning() {
        let directory = tempfile::tempdir().unwrap();
        let mut world = World::new();
        world.insert_resource(AssetDirectory(directory.path().into()));
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let asset = ImportedAsset {
            id: "a".repeat(32),
            name: "Asset".into(),
            file: "source.glb".into(),
            scale: 10.0,
        };
        let package = directory.path().join(&asset.id);
        std::fs::create_dir(&package).unwrap();
        let original = spawn(&mut world, root, 1, bevy::math::DVec2::ZERO, asset);
        world.entity_mut(original).insert(Spatial {
            elevation: 123.0,
            world_pinned: true,
            ..default()
        });
        let copy = duplicate(&mut world, original).unwrap();
        assert_eq!(
            world.get::<ImportedAsset>(copy).unwrap().id,
            world.get::<ImportedAsset>(original).unwrap().id
        );
        assert_eq!(
            super::super::position(&world, copy).unwrap(),
            bevy::math::DVec3::new(40.0, 123.0, 40.0)
        );
        world.get_mut::<Spatial>(copy).unwrap().world_pinned = false;
        assert!(world.get::<Spatial>(original).unwrap().world_pinned);
        discard(&mut world, original);
        assert!(package.exists());
        assert!(world.get_entity(copy).is_ok());
        discard(&mut world, copy);
        assert!(!package.exists());
    }

    #[test]
    fn import_rejects_path_escape_and_remote_resources() {
        for path in [
            "../x.bin",
            "%2e%2e/x.bin",
            "/x.bin",
            "https://x/model.bin",
            "C:\\x.bin",
            "a%00b",
            "a%2fb/../../x",
        ] {
            assert!(resource_path(path).is_err(), "{path}");
        }
        assert_eq!(
            resource_path("textures/a%20b.png").unwrap(),
            PathBuf::from("textures/a b.png")
        );
    }

    #[test]
    fn importing_copies_a_self_contained_asset_and_leaves_source_unchanged() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let path = source.path().join("simple.gltf");
        let bytes = include_bytes!("../../fixtures/open_scene.gltf");
        std::fs::write(&path, bytes).unwrap();
        let asset = copy_import(&path, destination.path()).unwrap();
        assert!(asset.valid());
        assert_eq!(
            std::fs::read(destination.path().join(asset.id).join(asset.file)).unwrap(),
            bytes
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedAsset {
    pub asset: ImportedAsset,
    pub workspace: u64,
    pub position: [f64; 2],
    pub size: [f32; 2],
    pub placement: crate::sand_placement::Placement,
}

pub fn snapshot(world: &mut World, root: Entity) -> Vec<SavedAsset> {
    world
        .query::<(
            Entity,
            &ImportedAsset,
            &CanvasItem,
            &WorkspaceMember,
            &ChildOf,
        )>()
        .iter(world)
        .filter(|(_, _, _, _, p)| p.parent() == root)
        .map(|(e, asset, item, member, _)| SavedAsset {
            asset: asset.clone(),
            workspace: member.0,
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, e),
        })
        .collect()
}
