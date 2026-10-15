use super::*;
use serde::{Serialize, de::DeserializeOwned};

fn state<T: DeserializeOwned>(
    settings: &BTreeMap<String, serde_json::Value>,
    fallback: T,
) -> Result<T, String> {
    settings
        .get("state")
        .and_then(serde_json::Value::as_str)
        .map(|source| serde_json::from_str(source).map_err(|error| error.to_string()))
        .unwrap_or(Ok(fallback))
}

fn component(kind: &str, state: &impl Serialize) -> ContentKind {
    let value = serde_json::to_value(state).unwrap();
    ContentKind::Native {
        kind: kind.into(),
        settings: BTreeMap::from([(
            "state".into(),
            serde_json::json!(serde_json::to_string(&value).unwrap()),
        )]),
        bindings: record_references(&value),
    }
}

pub(super) fn capture(world: &World, entity: Entity) -> Option<ContentKind> {
    if let Some(Content(ContentKind::Native { kind, .. })) = world.get::<Content>(entity)
        && kind == "layout"
    {
        if let Some(layout) = world.get::<crate::native::layout::LayoutBox>(entity) {
            return Some(ContentKind::Native {
                kind: "layout".into(),
                settings: BTreeMap::from([(
                    "rules".into(),
                    serde_json::json!(serde_json::to_string(&layout.rules).unwrap()),
                )]),
                bindings: Vec::new(),
            });
        }
    }
    if let Some(state) = world.get::<crate::native::simulation_castle::SimulationCastle>(entity) {
        return Some(component("simulation", state));
    }
    if let Some(state) = world.get::<crate::native::topology::assets::ImportedAsset>(entity) {
        return Some(component("import", state));
    }
    if world
        .get::<crate::native::shader_castle::ShaderCastle>(entity)
        .is_some()
    {
        let slug = world
            .get::<crate::native::castle_feed::Frame>(entity)
            .and_then(|frame| world.get::<crate::native::area::InfluenceArea>(frame.area))
            .and_then(|area| area.protein.as_ref())
            .and_then(|config| config.draft.query["where"][0]["all"][0]["slug_eq"].as_str())
            .unwrap_or_default();
        return Some(ContentKind::Native {
            kind: "shader".into(),
            settings: BTreeMap::from([("slug".into(), serde_json::json!(slug))]),
            bindings: Vec::new(),
        });
    }
    if let Some(state) = world.get::<crate::native::document_viewer::DocumentViewer>(entity) {
        return Some(component("document", state));
    }
    if let Some(state) = world.get::<crate::native::ide::Ide>(entity) {
        return Some(component("ide", state));
    }
    if let Some(state) = world.get::<crate::native::file_explorer::FileExplorer>(entity) {
        return Some(component("explorer", state));
    }
    if let Some(state) = world.get::<crate::native::recorder_castle::RecorderCastle>(entity) {
        return Some(component("recorder", state));
    }
    if let Some(state) = world.get::<crate::native::media_sand::MediaSand>(entity) {
        return Some(component("media", state));
    }
    if let Some(state) = world.get::<crate::native::drawing::NativeDrawing>(entity) {
        return Some(component("drawing", &state.0));
    }
    if let Some(state) = world.get::<crate::native::assertion_castle::AssertionCastle>(entity) {
        return Some(component("assertion", state));
    }
    None
}

pub(super) fn descriptors() -> Vec<api::Descriptor> {
    let defaults = [
        (
            "document",
            "Document viewer",
            serde_json::to_string(&crate::native::document_viewer::DocumentViewer::default())
                .unwrap(),
        ),
        (
            "ide",
            "Editor",
            serde_json::to_string(&crate::native::ide::Ide::default()).unwrap(),
        ),
        (
            "explorer",
            "File explorer",
            serde_json::to_string(&crate::native::file_explorer::FileExplorer::default()).unwrap(),
        ),
        (
            "recorder",
            "Recorder",
            serde_json::to_string(&crate::native::recorder_castle::RecorderCastle::default())
                .unwrap(),
        ),
        (
            "media",
            "Media",
            serde_json::to_string(&crate::native::media_sand::MediaSand::Link {
                url: "https://lince.social".into(),
            })
            .unwrap(),
        ),
        (
            "drawing",
            "Drawing",
            serde_json::to_string(&nucleus::drawing::Drawing::default()).unwrap(),
        ),
        (
            "assertion",
            "Assertion",
            serde_json::to_string(&crate::native::assertion_castle::AssertionCastle::default())
                .unwrap(),
        ),
        (
            "simulation",
            "Simulation",
            serde_json::to_string(&crate::native::simulation_castle::SimulationCastle::default())
                .unwrap(),
        ),
        ("import", "Imported resource", "{}".into()),
    ];
    let mut result: Vec<_> = defaults.into_iter().map(|(kind, name, default)| api::Descriptor { kind: kind.into(), name: name.into(), description: format!("Native {name}, using its existing validated state JSON. Part authoring preserves native controls; configuring state recreates this factory and refuses active drafts. Referenced Record UIDs must also appear in bindings. Imported resources require an existing local asset package; serialization does not copy its files."), settings: BTreeMap::from([("state".into(), api::Setting::Text { default, max_bytes: 65536 })]), max_bindings: 32, placeable: true, configurable: true, composable: true, unavailable_reason: None }).collect();
    result.push(api::Descriptor { kind: "shader".into(), name: "Shader Castle".into(), description: "Existing native WGSL preview bound by Record slug. Shader execution uses the normal native controls.".into(), settings: BTreeMap::from([("slug".into(), api::Setting::Text { default: String::new(), max_bytes: 256 })]), max_bindings: 0, placeable: true, configurable: true, composable: true, unavailable_reason: None });
    result.push(api::Descriptor { kind: "layout".into(), name: "Layout container".into(), description: "A native Square container using existing validated layout Rules JSON. Part authoring configures rules and attaches/detaches sibling canvas parts through layout_parent; flow controls use their Node parent.".into(), settings: BTreeMap::from([("rules".into(), api::Setting::Text { default: serde_json::to_string(&crate::native::layout::Rules::fixed(Vec2::new(400.0, 240.0))).unwrap(), max_bytes: 4096 })]), max_bindings: 0, placeable: true, configurable: true, composable: true, unavailable_reason: None });
    result
}

pub(super) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    kind: &str,
    settings: &BTreeMap<String, serde_json::Value>,
    bindings: &[String],
) -> Result<Option<Entity>, String> {
    let value: serde_json::Value = settings
        .get("state")
        .and_then(serde_json::Value::as_str)
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or(serde_json::Value::Null);
    if record_references(&value)
        .iter()
        .any(|uid| !bindings.contains(uid))
    {
        return Err("List the native state's referenced Record UIDs in bindings.".into());
    }
    let entity = match kind {
        "layout" => {
            let rules: crate::native::layout::Rules = settings
                .get("rules")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str)
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| crate::native::layout::Rules::fixed(Vec2::new(400.0, 240.0)));
            if !rules.valid() {
                return Err("Invalid layout container rules.".into());
            }
            let entity = crate::native::sand_store::spawn_sand(
                world,
                root,
                workspace,
                SandKind::Square,
                "",
                position,
            );
            if let Err(error) = crate::native::layout::configure(world, entity, rules) {
                world.despawn(entity);
                return Err(error.into());
            }
            entity
        }
        "document" => {
            let value = state(
                settings,
                crate::native::document_viewer::DocumentViewer::default(),
            )?;
            if !value.valid() {
                return Err("Invalid document state.".into());
            }
            crate::native::document_viewer::spawn(world, root, workspace, position, value)
        }
        "ide" => {
            let value = state(settings, crate::native::ide::Ide::default())?;
            if !value.valid() {
                return Err("Invalid editor state.".into());
            }
            crate::native::ide::spawn(world, root, workspace, position, value)
        }
        "explorer" => {
            let value = state(
                settings,
                crate::native::file_explorer::FileExplorer::default(),
            )?;
            if !value.valid() {
                return Err("Invalid explorer state.".into());
            }
            crate::native::file_explorer::spawn(world, root, workspace, position, value)
        }
        "recorder" => {
            let value = state(
                settings,
                crate::native::recorder_castle::RecorderCastle::default(),
            )?;
            if !value.valid() {
                return Err("Invalid recorder state.".into());
            }
            crate::native::recorder_castle::spawn(world, root, workspace, position, value)
        }
        "media" => {
            let value = state(
                settings,
                crate::native::media_sand::MediaSand::Link {
                    url: "https://lince.social".into(),
                },
            )?;
            if !value.valid() {
                return Err("Invalid media state.".into());
            }
            crate::native::media_sand::spawn(world, root, workspace, position, value)
        }
        "drawing" => {
            let value = state(settings, nucleus::drawing::Drawing::default())?;
            value.validate()?;
            crate::native::drawing::spawn(world, root, workspace, position, value)
        }
        "assertion" => {
            let value = state(
                settings,
                crate::native::assertion_castle::AssertionCastle::default(),
            )?;
            if value
                .selection
                .as_ref()
                .is_some_and(|selection| !selection.valid())
            {
                return Err("Invalid assertion selection.".into());
            }
            crate::native::assertion_castle::spawn(world, root, workspace, position, value)
        }
        "simulation" => {
            let value = state(
                settings,
                crate::native::simulation_castle::SimulationCastle::default(),
            )?;
            if !value.valid() {
                return Err("Invalid simulation state.".into());
            }
            crate::native::simulation_castle::spawn(world, root, workspace, position, value)
        }
        "shader" => crate::native::shader_castle::spawn(
            world,
            root,
            workspace,
            position,
            settings
                .get("slug")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
        ),
        "import" => {
            let source = settings
                .get("state")
                .and_then(serde_json::Value::as_str)
                .ok_or("Provide an existing imported resource state.")?;
            let asset: crate::native::topology::assets::ImportedAsset =
                serde_json::from_str(source).map_err(|error| error.to_string())?;
            if !asset.valid() {
                return Err("Invalid imported resource.".into());
            }
            let directory = world
                .get_resource::<crate::native::topology::assets::AssetDirectory>()
                .ok_or("The local asset directory is unavailable.")?;
            if !directory.0.join(&asset.id).join(&asset.file).is_file() {
                return Err(
                    "This resource's local package is missing; import its files before reuse."
                        .into(),
                );
            }
            crate::native::topology::assets::spawn(world, root, workspace, position, asset)
        }
        _ => return Ok(None),
    };
    Ok(Some(entity))
}
