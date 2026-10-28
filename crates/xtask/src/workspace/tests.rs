use super::{character, fixtures, validate};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lince-workspace-fixtures-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn prepare(&self) -> PathBuf {
        let path = self.0.join("dataset");
        fixtures::create(project(), &path).unwrap();
        path
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
}

fn manifest(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap()
}

fn save(root: &Path, manifest: &Value) {
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

fn update_asset(root: &Path, name: &str, bytes: &[u8]) {
    let mut value = manifest(root);
    let asset = value["assets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|asset| asset["id"] == name)
        .unwrap();
    let path = asset["path"].as_str().unwrap().to_owned();
    asset["sha256"] = json!(fixtures::digest(bytes));
    asset["encoded_bytes"] = json!(bytes.len());
    fs::write(root.join(path), bytes).unwrap();
    save(root, &value);
}

fn channel_samples(channel: &gltf::animation::Channel<'_>, bytes: &[u8]) -> (Vec<f32>, Vec<f32>) {
    use gltf::animation::util::ReadOutputs;
    let reader = channel.reader(|_| Some(bytes));
    let inputs = reader.read_inputs().unwrap().collect();
    let outputs = match reader.read_outputs().unwrap() {
        ReadOutputs::Translations(values) => values.flatten().collect(),
        ReadOutputs::Rotations(values) => values.into_f32().flatten().collect(),
        ReadOutputs::Scales(values) => values.flatten().collect(),
        ReadOutputs::MorphTargetWeights(values) => values.into_f32().collect(),
    };
    (inputs, outputs)
}

#[test]
fn prepared_scenes_and_replays_are_bounded_complete_inputs_without_runtime_claims() {
    let temp = Directory::new();
    let root = temp.prepare();
    let report = validate::check(&root).unwrap();
    assert_eq!(
        report["status"],
        "fixture-integrity-and-replay-specifications-verified"
    );
    assert_eq!(
        report["runtime_workflow_evidence"],
        "pending at the specified owning stages"
    );
    assert!(report["packed_fixture_bytes"].as_u64().unwrap() < 64 * 1024 * 1024);
    assert_eq!(report["assets"]["coastline"]["features"], 134);
    assert_eq!(report["assets"]["starter-character"]["skin_joints"], 27);
    assert_eq!(
        report["assets"]["starter-character"]["clips"],
        json!(["idle", "walk"])
    );
    let scene = manifest(&root)["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|scene| scene["id"] == "invented-extent")
        .unwrap()
        .clone();
    assert!(
        !scene["required_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| profile == "geographic-reference")
    );
    assert!(scene["frames"][1]["origin_m"][0].as_f64().unwrap().abs() > 1e11);
    assert_eq!(report["assets"]["local-place"]["scenes"], 2);
    assert_eq!(report["assets"]["contracts"]["registered_requirements"], 49);
    assert_eq!(
        report["assets"]["hosting"]["cases"],
        json!(["B33", "B34", "B35", "B36"])
    );
    assert!(fixtures::create(project(), &root).is_err());
}

#[test]
fn missing_contract_ownership_and_divergent_hosting_replays_are_rejected() {
    let temp = Directory::new();
    let root = temp.prepare();
    let original = fs::read(root.join("contracts.json")).unwrap();
    let mut contracts: Value = serde_json::from_slice(&original).unwrap();
    contracts["requirements"][0]["normative_owner_stage"] = Value::Null;
    update_asset(&root, "contracts", &serde_json::to_vec(&contracts).unwrap());
    assert!(validate::check(&root).unwrap_err().contains("owner"));
    update_asset(&root, "contracts", &original);
    let mut hosting: Value =
        serde_json::from_slice(&fs::read(root.join("hosting.json")).unwrap()).unwrap();
    hosting["cases"][0]["phase_replays"][0]["runtime_owner_stage"] = json!(19);
    update_asset(&root, "hosting", &serde_json::to_vec(&hosting).unwrap());
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("replay disagree")
    );
}

#[test]
fn preparation_is_reproducible_and_does_not_replace_existing_data() {
    let first = Directory::new();
    let second = Directory::new();
    let left = first.prepare();
    let right = second.prepare();
    assert_eq!(
        fs::read(left.join("manifest.json")).unwrap(),
        fs::read(right.join("manifest.json")).unwrap()
    );
    for entry in fs::read_dir(left).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(
            fs::read(entry.path()).unwrap(),
            fs::read(right.join(entry.file_name())).unwrap()
        );
    }
}

#[test]
fn changed_payloads_sizes_and_missing_required_cases_are_rejected() {
    let temp = Directory::new();
    let root = temp.prepare();
    let original = manifest(&root);
    let mut changed = original.clone();
    changed["assets"][0]["encoded_bytes"] = json!(1);
    save(&root, &changed);
    assert!(validate::check(&root).unwrap_err().contains("length/hash"));
    let mut changed = original.clone();
    changed["cases"]
        .as_array_mut()
        .unwrap()
        .retain(|case| case["id"] != "B38");
    save(&root, &changed);
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("Required core")
    );
    let mut changed = original.clone();
    changed["cases"][0]["steps"][0]["measured"] = json!([]);
    save(&root, &changed);
    assert!(validate::check(&root).unwrap_err().contains("metric"));
    save(&root, &original);
    fs::write(root.join("globe.glb"), b"truncated").unwrap();
    assert!(validate::check(&root).unwrap_err().contains("length/hash"));
}

#[test]
fn local_payload_reads_reject_traversal_absolute_paths_and_symlinks() {
    let temp = Directory::new();
    let root = temp.prepare();
    for path in ["../manifest.json", "/etc/passwd", "./globe.glb", ""] {
        assert!(validate::read(&root, path, 1024).is_err(), "{path}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("globe.glb"), root.join("linked.glb")).unwrap();
        assert!(
            validate::read(&root, "linked.glb", 8 * 1024 * 1024)
                .unwrap_err()
                .contains("symlink")
        );
    }
    assert!(
        validate::read(&root, "globe.glb", 1)
            .unwrap_err()
            .contains("byte cap")
    );
}

#[test]
fn cycles_dangling_references_and_public_private_payload_mixing_are_rejected() {
    let temp = Directory::new();
    let root = temp.prepare();
    let original = manifest(&root);
    let mut changed = original.clone();
    changed["scenes"][0]["frames"][1]["parent"] = json!("site");
    save(&root, &changed);
    assert!(validate::check(&root).unwrap_err().contains("Cycle"));
    let mut changed = original.clone();
    changed["scenes"][0]["objects"][0]["asset"] = json!("missing");
    save(&root, &changed);
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("required asset")
    );
    let mut changed = original;
    changed["scenes"][0]["objects"][0]["asset"] = json!("private-annotation");
    changed["scenes"][0]["objects"][0]["gltf_scene"] = Value::Null;
    save(&root, &changed);
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("restricted payload")
    );
}

#[test]
fn an_updated_hash_does_not_hide_a_blocked_doorway_or_bad_geographic_data() {
    let temp = Directory::new();
    let root = temp.prepare();
    let bytes = fs::read(root.join("local-place.glb")).unwrap();
    let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut document: Value = serde_json::from_slice(&parsed.json).unwrap();
    let header = document["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["name"] == "home/door-header")
        .unwrap();
    header["scale"] = json!([1., 8., 0.2]);
    let changed = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
        bin: parsed.bin,
    }
    .to_vec()
    .unwrap();
    update_asset(&root, "local-place", &changed);
    assert!(validate::check(&root).unwrap_err().contains("path blocked"));
    update_asset(&root, "local-place", &bytes);
    let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut document: Value = serde_json::from_slice(&parsed.json).unwrap();
    let position_accessor = document["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    document["accessors"][position_accessor]["min"] = json!([1000., 1000., 1000.]);
    document["accessors"][position_accessor]["max"] = json!([1001., 1001., 1001.]);
    let changed = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
        bin: parsed.bin,
    }
    .to_vec()
    .unwrap();
    update_asset(&root, "local-place", &changed);
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("bounds do not enclose")
    );
    update_asset(&root, "local-place", &bytes);
    let data = fs::read(root.join("coastline.geojson")).unwrap();
    let mut coastline: Value = serde_json::from_slice(&data).unwrap();
    coastline["features"][0]["geometry"]["coordinates"][0][1] = json!(91.);
    update_asset(&root, "coastline", &serde_json::to_vec(&coastline).unwrap());
    assert!(
        validate::check(&root)
            .unwrap_err()
            .contains("longitude/latitude")
    );
}

#[test]
fn starter_repacking_preserves_skin_and_keeps_only_admitted_clips() {
    let bytes = fs::read(project().join("institute/assets/characters/fiote/fiote.glb")).unwrap();
    let source = gltf::Gltf::from_slice(&bytes).unwrap();
    let prepared = character::starter(&bytes).unwrap();
    let starter = gltf::Gltf::from_slice(&prepared).unwrap();
    assert!(prepared.len() < bytes.len());
    let original_mesh = source.meshes().next().unwrap();
    let prepared_mesh = starter.meshes().next().unwrap();
    let original_primitive = original_mesh.primitives().next().unwrap();
    let prepared_primitive = prepared_mesh.primitives().next().unwrap();
    let original = original_primitive.reader(|_| source.blob.as_deref());
    let derived = prepared_primitive.reader(|_| starter.blob.as_deref());
    assert_eq!(
        original.read_positions().unwrap().collect::<Vec<_>>(),
        derived.read_positions().unwrap().collect::<Vec<_>>()
    );
    assert_eq!(
        original
            .read_weights(0)
            .unwrap()
            .into_f32()
            .collect::<Vec<_>>(),
        derived
            .read_weights(0)
            .unwrap()
            .into_f32()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        original
            .read_joints(0)
            .unwrap()
            .into_u16()
            .collect::<Vec<_>>(),
        derived
            .read_joints(0)
            .unwrap()
            .into_u16()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        starter
            .animations()
            .map(|clip| clip.name().unwrap())
            .collect::<Vec<_>>(),
        vec!["idle", "walk"]
    );
    for clip in starter.animations() {
        let original = source
            .animations()
            .find(|original| original.name() == clip.name())
            .unwrap();
        assert_eq!(original.channels().count(), clip.channels().count());
        for (left, right) in original.channels().zip(clip.channels()) {
            assert_eq!(left.target().node().index(), right.target().node().index());
            assert_eq!(left.target().property(), right.target().property());
            assert_eq!(
                channel_samples(&left, source.blob.as_deref().unwrap()),
                channel_samples(&right, starter.blob.as_deref().unwrap())
            );
        }
    }
    assert!(
        starter
            .nodes()
            .any(|node| node.name() == Some("fixture-bandana"))
    );
}
