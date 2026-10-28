use super::{character, geometry};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Asset {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub encoded_bytes: u64,
    pub kind: String,
    pub license: String,
    pub disclosure: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    pub id: String,
    pub parent: Option<String>,
    pub origin_m: [f64; 3],
    pub rotation_xyzw: [f64; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Object {
    pub id: String,
    pub frame: String,
    pub asset: String,
    pub gltf_scene: Option<usize>,
    pub origin_m: [f64; 3],
    pub rotation_xyzw: [f64; 4],
    pub initially_enabled: bool,
    pub replaces_selection: Option<String>,
    pub display_unit: String,
    pub source_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bookmark {
    pub id: String,
    pub frame: String,
    pub eye_m: [f64; 3],
    pub target_m: [f64; 3],
    pub mode: String,
    pub subject: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scene {
    pub id: String,
    pub required_profiles: Vec<String>,
    pub disclosure: String,
    pub frames: Vec<Frame>,
    pub objects: Vec<Object>,
    pub bookmarks: Vec<Bookmark>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub id: String,
    pub native_action: String,
    pub target: String,
    pub expected: Vec<String>,
    pub measured: Vec<String>,
    pub runtime_owner_stage: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Case {
    pub id: String,
    pub scenes: Vec<String>,
    pub delivery_kind: String,
    pub requirements: Vec<String>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bundle {
    pub format: String,
    pub schema: u32,
    pub status: String,
    pub prototype_boundary: String,
    pub source_hash_algorithm: String,
    pub profile: Value,
    pub assets: Vec<Asset>,
    pub scenes: Vec<Scene>,
    pub cases: Vec<Case>,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn frame(id: &str, parent: Option<&str>, origin_m: [f64; 3]) -> Frame {
    Frame {
        id: id.into(),
        parent: parent.map(str::to_owned),
        origin_m,
        rotation_xyzw: [0., 0., 0., 1.],
    }
}

fn object(id: &str, frame: &str, asset: &str, scene: Option<usize>, position: [f64; 3]) -> Object {
    Object {
        id: id.into(),
        frame: frame.into(),
        asset: asset.into(),
        gltf_scene: scene,
        origin_m: position,
        rotation_xyzw: [0., 0., 0., 1.],
        initially_enabled: true,
        replaces_selection: None,
        display_unit: "m".into(),
        source_status: "authored-synthetic".into(),
    }
}

fn bookmark(
    id: &str,
    frame: &str,
    eye: [f64; 3],
    target: [f64; 3],
    mode: &str,
    subject: &str,
) -> Bookmark {
    Bookmark {
        id: id.into(),
        frame: frame.into(),
        eye_m: eye,
        target_m: target,
        mode: mode.into(),
        subject: subject.into(),
    }
}

fn step(
    id: &str,
    action: &str,
    target: &str,
    stage: u8,
    expected: &[&str],
    metrics: &[&str],
) -> Step {
    Step {
        id: id.into(),
        native_action: action.into(),
        target: target.into(),
        expected: expected.iter().map(|s| (*s).into()).collect(),
        measured: metrics.iter().map(|s| (*s).into()).collect(),
        runtime_owner_stage: stage,
    }
}

fn scenes() -> Vec<Scene> {
    let mut site = frame("site", Some("earth"), [6_378_137., 0., 0.]);
    site.rotation_xyzw = [
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
        0.,
        0.,
    ];
    let mut globe = object("reference-globe", "earth", "globe", Some(0), [0.; 3]);
    globe.rotation_xyzw = [
        std::f64::consts::FRAC_1_SQRT_2,
        0.,
        0.,
        std::f64::consts::FRAC_1_SQRT_2,
    ];
    let mut castle = object("custom-castle-overlay", "site", "castle", Some(0), [0.; 3]);
    castle.initially_enabled = false;
    castle.replaces_selection = Some("personal-places/home".into());
    vec![
        Scene {
            id:"earth-site".into(),
            required_profiles:vec!["frames".into(),"mesh".into(),"geographic-reference".into()],
            disclosure:"fixture-public".into(),
            frames:vec![frame("earth",None,[0.;3]),site],
            objects:vec![
                globe,
                object("personal-places","site","local-place",Some(0),[0.;3]),
                castle,
            ],
            bookmarks:vec![
                bookmark("ball-close","site",[0.85,0.12,0.6],[0.7,0.02,0.4],"Orbit","personal-places"),
                bookmark("home-interior","site",[0.,1.7,1.8],[0.,1.4,-1.],"Walk","personal-places"),
                bookmark("home-approach","site",[0.,1.7,5.],[0.,1.7,0.],"Walk","personal-places"),
                bookmark("street","site",[0.,1.7,15.],[0.,1.7,0.],"Walk","personal-places"),
                bookmark("site-overview","site",[0.,120.,130.],[0.,0.,45.],"Orbit","personal-places"),
                bookmark("globe-overview","earth",[18_000_000.,-12_000_000.,14_000_000.],[0.;3],"Orbit","reference-globe"),
                bookmark("dateline","earth",[-22_000_000.,0.,0.],[0.;3],"Orbit","reference-globe"),
                bookmark("north-pole","earth",[0.,0.,22_000_000.],[0.;3],"Orbit","reference-globe"),
            ],
            notes:vec![
                "The site is synthetic at geodetic latitude/longitude 0/0 and ellipsoidal height 0; it is not a real city or home.".into(),
                "Local +X is east, +Y is up and +Z is south; navigation up is separate from future physical gravity.".into(),
                "The globe payload uses glTF Y-up; its explicit object rotation maps the short +Y axis to ECEF +Z.".into(),
                "The castle is an initially disabled authored replacement at the home occurrence; never show the solid base and replacement as one colliding model.".into(),
                "Natural Earth is a separate coarse coastline layer; zero added relief is fallback, not measured terrain.".into(),
                "Optional solar context attaches outside this root through solar-context.json; adding it cannot rewrite local source identities or dimensions.".into(),
            ],
        },
        Scene {
            id:"standalone-room".into(),required_profiles:vec!["frames".into(),"mesh".into()],
            disclosure:"fixture-public".into(),
            frames:vec![frame("room",None,[0.;3])],
            objects:vec![object("independent-room","room","local-place",Some(1),[0.;3])],
            bookmarks:vec![
                bookmark("room-enter","room",[0.,1.7,5.],[0.,1.7,0.],"Walk","independent-room"),
                bookmark("room-detail","room",[0.85,0.12,0.6],[0.7,0.02,0.4],"Orbit","independent-room"),
            ],
            notes:vec!["Complete room root with no Earth, solar, coastline, provider or solver dependency.".into()],
        },
        Scene {
            id:"blank-grid".into(),required_profiles:vec!["frames".into(),"flat-native".into()],
            disclosure:"fixture-public".into(),
            frames:vec![frame("blank-root",None,[0.;3])],
            objects:vec![object("flat-fallback","blank-root","flat-fallback",None,[0.;3])],
            bookmarks:vec![bookmark("blank-origin","blank-root",[0.,20.,20.],[0.;3],"Orbit","flat-fallback")],
            notes:vec![
                "Independent flat workspace root; constant fallback support has zero added height and requires no geographic data.".into(),
                "Applying the same authored relief adds bounded support/render demand; blank extent never allocates an extent-sized plane.".into(),
            ],
        },
        Scene {
            id:"private-study".into(),required_profiles:vec!["frames".into(),"mesh".into()],
            disclosure:"identified-editor".into(),
            frames:vec![frame("study-root",None,[0.;3])],
            objects:vec![object("restricted-interior","study-root","restricted-interior",Some(0),[0.;3])],
            bookmarks:vec![bookmark("private-interior","study-root",[0.,1.6,2.],[0.,1.2,0.],"Walk","restricted-interior")],
            notes:vec!["Synthetic mixed-rights fixture; public projections must omit its payload, identity/hash/count metadata and selectors unless separately published.".into()],
        },
        Scene {
            id:"invented-extent".into(),required_profiles:vec!["frames".into(),"mesh".into()],
            disclosure:"fixture-public".into(),
            frames:vec![
                frame("open-structure",None,[0.;3]),
                frame("remote-pod",Some("open-structure"),[-1.0e12,5.0e11,2.0e12]),
                frame("compartment",Some("remote-pod"),[12.,4.,-7.]),
            ],
            objects:vec![
                object("pod-room","compartment","local-place",Some(1),[0.;3]),
                object("seven-mm-feature","compartment","invented-feature",Some(0),[1.,0.0035,0.]),
            ],
            bookmarks:vec![
                bookmark("invented-detail","compartment",[1.02,0.02,0.03],[1.,0.0035,0.],"Orbit","seven-mm-feature"),
                bookmark("invented-walk","compartment",[0.,1.7,5.],[0.,1.7,0.],"Walk","pod-room"),
                bookmark("invented-outer","open-structure",[0.,3.0e12,4.0e12],[0.;3],"Orbit","pod-room"),
            ],
            notes:vec![
                "The 7 mm target and 4e12 m outer context are unrelated data choices, with a scale ratio comparable to B25.".into(),
                "Relative local evaluation must use the common frame ancestry; giant global f64 subtraction is not a valid local-precision implementation.".into(),
                "Repeat after renaming all scene/frame/object IDs, expressing local numeric input in cm and adding/removing an outer frame.".into(),
                "Display-unit conversion never scales accepted geometry and loading this scene starts no geographic/astronomical source.".into(),
            ],
        },
    ]
}

fn cases() -> Vec<Case> {
    let navigation_metrics = [
        "frame_p95_ms",
        "frame_p99_ms",
        "visible_input_p95_ms",
        "active_cpu_gpu_and_staging_bytes",
        "local_dimension_error_m",
    ];
    let scale = Case {
        id: "B25".into(),
        scenes: vec!["earth-site".into()],
        delivery_kind: "required-example-recipe-over-shared-primitives".into(),
        requirements: vec![
            "C09".into(),
            "K2".into(),
            "K7".into(),
            "I1".into(),
            "I6".into(),
        ],
        steps: vec![
            step(
                "inspect-ball",
                "Open ball-close in Orbit, select home/ball and inspect its diameter numerically.",
                "ball-close",
                5,
                &[
                    "diameter = 0.04 m",
                    "same occurrence through numeric and spatial views",
                ],
                &navigation_metrics,
            ),
            step(
                "leave-building",
                "Select Walk explicitly, use home-interior then walk along -Z to the interior and back through home-approach.",
                "home-approach",
                5,
                &[
                    "doorway remains passable",
                    "look toward the horizon",
                    "physical dimensions unchanged",
                ],
                &navigation_metrics,
            ),
            step(
                "use-content",
                "Focus home-notes and shared-tasks mounted content, edit permitted content, return focus to navigation.",
                "personal-places",
                5,
                &[
                    "same content in spatial and readable presentation",
                    "source/login policy remains independent",
                ],
                &navigation_metrics,
            ),
            step(
                "walk-street",
                "Walk the supplied street and locate work, university and community landmarks.",
                "street",
                5,
                &[
                    "all four supplied place identities remain findable",
                    "missing city detail remains absent",
                ],
                &navigation_metrics,
            ),
            step(
                "leave-site",
                "Switch Fly then Orbit explicitly and dolly outward through site-overview to globe-overview.",
                "globe-overview",
                5,
                &[
                    "one continuous scene",
                    "covering representations survive refinement",
                    "source and dimensions do not change",
                ],
                &navigation_metrics,
            ),
            step(
                "add-outer-context",
                "Admit solar-context.json and dolly to its declared outer view; remove and re-add it.",
                "solar-context",
                5,
                &[
                    "Earth/site source identities and local values remain unchanged",
                    "no orbital worker or mandatory solar parent",
                ],
                &navigation_metrics,
            ),
            step(
                "return",
                "Reverse the same continuous path and select the original home/ball occurrence.",
                "ball-close",
                5,
                &[
                    "same saved selection and 0.04 m diameter",
                    "local numerical preservation within profile bound",
                ],
                &navigation_metrics,
            ),
            step(
                "override-home",
                "Enable custom-castle-overlay as a replacement, observe its silhouette from street/overview and walk its gate/keep.",
                "custom-castle-overlay",
                5,
                &[
                    "base/replacement are distinct layers",
                    "coarse proxy and near geometry identify one authored place",
                    "doorway/interior usable",
                ],
                &navigation_metrics,
            ),
        ],
    };
    vec![
        scale,
        Case {
            id: "B26".into(),
            scenes: vec!["earth-site".into(), "blank-grid".into()],
            delivery_kind: "shared-surface-source-example".into(),
            requirements: vec!["C12".into(), "K3".into(), "K6".into()],
            steps: vec![step(
                "shared-relief",
                "Place relief.json once in site and once in an independent blank scene; move its anchor and cancel/undo an amplitude edit.",
                "relief",
                5,
                &[
                    "same source recipe",
                    "zero-relief fallback and absent coverage stay explicit",
                    "only affected bounds/render/support invalidate",
                ],
                &[
                    "projected_error_px",
                    "affected_products",
                    "peak_replacement_bytes",
                    "source_identity",
                ],
            )],
        },
        Case {
            id: "B27".into(),
            scenes: vec!["earth-site".into(), "standalone-room".into()],
            delivery_kind: "navigation-and-mounted-input-example".into(),
            requirements: vec!["C10".into(), "I1".into(), "I5".into()],
            steps: vec![step(
                "modes-and-support",
                "Switch Orbit/Fly/Walk explicitly, adjust speed, enter the doorway, transfer mounted input and remove required support during a controlled replay.",
                "home-approach",
                5,
                &[
                    "pose/focus/selection preserved where supported",
                    "missing support stops/reconciles",
                    "body and camera separately owned",
                ],
                &navigation_metrics,
            )],
        },
        Case {
            id: "B28".into(),
            scenes: vec!["earth-site".into(), "invented-extent".into()],
            delivery_kind: "demand-cache-and-offline-example".into(),
            requirements: vec!["C07".into(), "I6".into(), "E1".into(), "E6".into()],
            steps: vec![step(
                "reverse-and-release",
                "Open two views, reverse travel while detail loads, close one view then the last, and revisit permitted cached inputs in full-offline mode.",
                "invented-outer",
                5,
                &[
                    "shared demand survives one close",
                    "last demand releases decoded/GPU products",
                    "offline remote requests = 0",
                    "saved edits and disk pins survive residency eviction",
                ],
                &[
                    "peak_cpu_gpu_inflight_bytes",
                    "release_ms",
                    "cache_bytes",
                    "cancel_ms",
                    "remote_requests",
                    "bytes_versus_visited_history",
                ],
            )],
        },
        Case {
            id: "B38".into(),
            scenes: vec!["standalone-room".into(), "invented-extent".into()],
            delivery_kind: "required-scene-independence-example".into(),
            requirements: vec![
                "C09".into(),
                "K2".into(),
                "K7".into(),
                "I1".into(),
                "I6".into(),
            ],
            steps: vec![
                step(
                    "independent-room",
                    "Disable geographic/astronomical profiles and open standalone-room directly; walk inside and use the same source/selection/tools.",
                    "room-enter",
                    5,
                    &[
                        "no Earth or source request",
                        "same ordinary native/save/undo path",
                    ],
                    &navigation_metrics,
                ),
                step(
                    "arbitrary-extent",
                    "Travel from invented-detail through invented-walk to invented-outer and back using the generic implementation.",
                    "invented-outer",
                    5,
                    &[
                        "same 0.007 m feature identity and dimension",
                        "cost follows active data, not address range",
                    ],
                    &navigation_metrics,
                ),
                step(
                    "metamorphic-inputs",
                    "Rename IDs, re-express inputs in cm, add an unrelated outer parent, reverse the route and repeat two distant close-ups.",
                    "invented-detail",
                    5,
                    &[
                        "physical quantities invariant under view/unit changes",
                        "no scene-name or fixed-scale dispatch",
                    ],
                    &navigation_metrics,
                ),
            ],
        },
        Case {
            id: "B41".into(),
            scenes: vec!["standalone-room".into()],
            delivery_kind: "phased-articulated-character-and-mechanism-example".into(),
            requirements: vec![
                "K1".into(),
                "K5".into(),
                "K7".into(),
                "I4".into(),
                "I6".into(),
            ],
            steps: vec![
                step(
                    "starter-navigation",
                    "Admit starter-character.glb and only its idle/walk clips; navigate first/third person through the supplied room.",
                    "starter-character",
                    5,
                    &[
                        "body/camera ownership independent",
                        "no internal anatomy or scientific worker",
                        "permitted lightweight presence only",
                    ],
                    &navigation_metrics,
                ),
                step(
                    "appearance",
                    "Apply character-variant.json through normal selected-copy scope; edit garment/fur appearance, undo and reopen.",
                    "starter-character",
                    7,
                    &[
                        "unselected copies unchanged",
                        "source and rigid garment binding retained",
                    ],
                    &[
                        "changed_upload_bytes",
                        "active_vertices_joints_clips",
                        "native_edit_commit_ms",
                    ],
                ),
                step(
                    "shared-articulation",
                    "Pose and edit the character and the differently articulated two-link mechanism using shared commands; compare FK and bounded IK, save and reload.",
                    "mechanism",
                    11,
                    &[
                        "valid limits and bindings",
                        "unreachable/ambiguous mappings explicit",
                        "source/pose/runtime separate",
                    ],
                    &[
                        "constraint_residual",
                        "iterations",
                        "deformation_error_m",
                        "active_cpu_gpu_and_staging_bytes",
                    ],
                ),
                step(
                    "optional-anatomy",
                    "Admit the separately selected reference pack, inspect its supported upper-limb structures and pose-pair annotation with permitted Fiote context.",
                    "optional-reference-anatomy",
                    11,
                    &[
                        "coverage/rights/reference-vs-observation explicit",
                        "native continuation works",
                        "return to cheap body releases internal payloads",
                    ],
                    &[
                        "pack_bytes",
                        "named_regions",
                        "context_bytes",
                        "last_demand_release_ms",
                    ],
                ),
            ],
        },
    ]
}

fn solar() -> Value {
    let rows = [
        ("Mercury", 2_439_500., 57.9e9),
        ("Venus", 6_052_000., 108.2e9),
        ("Earth", 6_378_137., 149.6e9),
        ("Mars", 3_396_000., 228.0e9),
        ("Jupiter", 71_492_000., 778.5e9),
        ("Saturn", 60_268_000., 1432.0e9),
        ("Uranus", 25_559_000., 2867.0e9),
        ("Neptune", 24_764_000., 4515.0e9),
    ];
    let mut bodies = vec![json!({"id":"Sun","radius_m":695_700_000.,"position_m":[0.,0.,0.]})];
    for (index, (id, radius, distance)) in rows.into_iter().enumerate() {
        let phase = if id == "Earth" {
            0.
        } else {
            index as f64 * 0.71
        };
        bodies.push(json!({"id":id,"radius_m":radius,"position_m":[distance*phase.cos(),distance*phase.sin(),0.],"source_status":"rounded-context-with-authored-phase"}));
    }
    json!({
        "format":"lince-stage01-optional-context","schema":1,
        "source":"illustrative-solar-context",
        "epoch":"authored-static-context-2026-01-01",
        "scientific_accuracy":"No ephemeris or orbital dynamics; phases are authored fixture data.",
        "frame":{"id":"solar","axes":"right-handed authored coplanar context","units":"metre"},
        "bodies":bodies,
        "optional_attachment":{"child":"earth-site/earth","parent":"solar","translation_m":[149.6e9,0.,0.],"preserve_child_sources":true},
        "outer_view":{"frame":"solar","eye_m":[0.,0.,10.0e12],"target_m":[0.,0.,0.],"mode":"Orbit"},
        "automatic_workers":0
    })
}

fn hosted_cases(hosting: &Value) -> Result<Vec<Case>> {
    hosting["cases"]
        .as_array()
        .ok_or("Hosting fixture lacks phased cases")?
        .iter()
        .map(|case| {
            Ok(Case {
                id: case["id"].as_str().ok_or("Hosting case has no ID")?.into(),
                scenes: vec![
                    "earth-site".into(),
                    "private-study".into(),
                    "invented-extent".into(),
                ],
                delivery_kind: "phased-hosting-replay-specification".into(),
                requirements: serde_json::from_value(case["requirements"].clone())
                    .map_err(|error| error.to_string())?,
                steps: serde_json::from_value(case["phase_replays"].clone())
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect()
}

fn add_asset(
    output: &Path,
    assets: &mut Vec<Asset>,
    id: &str,
    path: &str,
    kind: &str,
    license: &str,
    disclosure: &str,
    bytes: &[u8],
) -> Result<()> {
    fs::write(output.join(path), bytes).map_err(|error| error.to_string())?;
    assets.push(Asset {
        id: id.into(),
        path: path.into(),
        sha256: digest(bytes),
        encoded_bytes: bytes.len() as u64,
        kind: kind.into(),
        license: license.into(),
        disclosure: disclosure.into(),
    });
    Ok(())
}

pub(super) fn create(root: &Path, output: &Path) -> Result<Bundle> {
    if output.exists() {
        return Err(
            "Fixture preparation requires a new directory; existing inputs/evidence are retained"
                .into(),
        );
    }
    let source = root.join("institute/fixtures/workspace/stage01");
    let profile: Value = serde_json::from_slice(
        &fs::read(source.join("profile.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let hosting_bytes = fs::read(source.join("hosting.json")).map_err(|error| error.to_string())?;
    let hosting: Value =
        serde_json::from_slice(&hosting_bytes).map_err(|error| error.to_string())?;
    let mut replay_cases = cases();
    replay_cases.extend(hosted_cases(&hosting)?);
    let coastline = fs::read(source.join("context/ne_110m_coastline.geojson"))
        .map_err(|error| error.to_string())?;
    if digest(&coastline) != "851f581ff5ffb844deed8ae1a9ce22e3c4bb3d74fa342cadb5d8e39b41ae7c3c" {
        return Err("Pinned Natural Earth coastline differs from the reviewed source".into());
    }
    let original_character = fs::read(root.join("institute/assets/characters/fiote/fiote.glb"))
        .map_err(|error| error.to_string())?;
    let character = character::starter(&original_character)?;
    fs::create_dir(output).map_err(|error| error.to_string())?;
    let mut assets = Vec::new();
    for (id, path, data) in [
        ("globe", "globe.glb", geometry::globe()?),
        ("local-place", "local-place.glb", geometry::local_place()?),
        ("castle", "castle.glb", geometry::castle()?),
        ("starter-character", "starter-character.glb", character),
        ("mechanism", "mechanism.glb", geometry::mechanism()?),
    ] {
        add_asset(
            output,
            &mut assets,
            id,
            path,
            "glb",
            "MIT",
            "fixture-public",
            &data,
        )?;
    }
    add_asset(
        output,
        &mut assets,
        "restricted-interior",
        "restricted-interior.glb",
        "glb",
        "MIT",
        "identified-editor",
        &geometry::private_room()?,
    )?;
    let mut feature = geometry::Glb::new();
    let sphere = feature.sphere(8, 16);
    let node = feature.node("arbitrary-seven-mm-feature", sphere, [0.; 3], [0.0035; 3]);
    add_asset(
        output,
        &mut assets,
        "invented-feature",
        "invented-feature.glb",
        "glb",
        "MIT",
        "fixture-public",
        &feature.finish(&[node])?,
    )?;
    add_asset(
        output,
        &mut assets,
        "coastline",
        "coastline.geojson",
        "geojson",
        "public-domain",
        "fixture-public",
        &coastline,
    )?;
    for (id,path,data) in [
        ("sources","sources.json",fs::read(source.join("sources.json")).map_err(|error|error.to_string())?),
        ("contracts","contracts.json",fs::read(source.join("contracts.json")).map_err(|error|error.to_string())?),
        ("hosting","hosting.json",hosting_bytes),
        ("solar-context","solar-context.json",serde_json::to_vec_pretty(&solar()).map_err(|error|error.to_string())?),
        ("flat-fallback","flat-fallback.json",serde_json::to_vec_pretty(&json!({
            "source_status":"constant-fallback-not-surveyed","support":"plane","units":"metre",
            "height_m":0.,"up":[0.,1.,0.],"coverage":"fallback-only",
            "allocation":"bounded ready neighborhood on demand; never populate the plane by address extent",
            "future_runtime_owner_stage":5
        })).map_err(|error|error.to_string())?),
        ("relief","relief.json",serde_json::to_vec_pretty(&json!({
            "source_status":"authored-synthetic","source_units":"metre","support":"height-field-with-declared-bounds",
            "formula":"amplitude * exp(-(x*x+z*z)/(radius*radius))",
            "parameters":{"amplitude_m":50.,"radius_m":128.},"bounds_m":[[-384.,-1.,-384.],[384.,50.,384.]],
            "placements":[{"scene":"earth-site","frame":"site","offset_m":[300.,0.,300.]},{"scene":"blank-grid","frame":"blank-root","offset_m":[0.,0.,0.]}],
            "composition":"additive source terms; equal opposite terms cancel within declared numeric tolerance",
            "future_runtime_owner_stage":5,"initially_enabled":false
        })).map_err(|error|error.to_string())?),
        ("character-variant","character-variant.json",serde_json::to_vec_pretty(&json!({
            "source":"starter-character","scope":"captured-selected-occurrence",
            "overrides":{"material":"fixture-bandana-green","base_color":[0.72,0.22,0.16,1.],"uniform_visual_scale":0.9},
            "source_retained":true,"future_runtime_owner_stage":7
        })).map_err(|error|error.to_string())?),
        ("optional-reference-anatomy","optional-reference-anatomy.json",serde_json::to_vec_pretty(&json!({
            "availability":"deferred-source-selection","owner_stage":11,
            "reference_pack_profiles":["male-reference","female-reference"],
            "required_bounded_first_region":"upper limb",
            "named_coverage":["bones","joints","ligaments","tendons","muscles","fat","major vessels","skin"],
            "source_dataset":null,"source_rights":null,"actual_source_counts":null,
            "default_download_bytes":0,"default_active_bytes":0,
            "admission_constraints":{"requested_selected_region_decoded_peak_bytes":67108864,"requested_pack_encoded_bytes":67108864,"aggregate_starter_limits_still_apply":true},
            "before_stage_11_implementation":["Select reviewed source/rights and supported variation.","Measure actual joints/vertices/weights/morphs/regions/encoded/decoded/staging counts.","Prepare reference geometry/rig/semantic mappings and freeze pose/error checks.","Admit only requested coverage within aggregate limits or review the named profile change."],
            "claims":"No delivered human anatomy, patient reconstruction, tissue law or diagnosis."
        })).map_err(|error|error.to_string())?),
        ("private-annotation","private-annotation.json",serde_json::to_vec_pretty(&json!({
            "source_status":"synthetic-restricted-fixture","body":"HOST-ONLY-STUDY-NOTES",
            "region":"optional-reference-anatomy","pose_pair":["pose-a","pose-b"],
            "is_medical_evidence":false,"access":"host-only; do not publish this payload, its hash or membership"
        })).map_err(|error|error.to_string())?),
    ] {
        let disclosure = if id == "private-annotation" { "host-private" } else { "fixture-public" };
        add_asset(output,&mut assets,id,path,"json","MIT",disclosure,&data)?;
    }
    add_asset(
        output,
        &mut assets,
        "fixture-license",
        "LICENSE",
        "text",
        "MIT",
        "fixture-public",
        &fs::read(root.join("LICENSE")).map_err(|error| error.to_string())?,
    )?;
    let mut credits = fs::read_to_string(source.join("context/CREDITS.txt"))
        .map_err(|error| error.to_string())?;
    credits.push_str("\nSynthetic geometry and the derived rigid bandana are original Lince fixture assets under MIT.\n");
    credits.push_str(
        &fs::read_to_string(root.join("institute/assets/characters/fiote/CREDITS.txt"))
            .map_err(|error| error.to_string())?,
    );
    add_asset(
        output,
        &mut assets,
        "credits",
        "CREDITS.txt",
        "text",
        "MIT-and-public-domain",
        "fixture-public",
        credits.as_bytes(),
    )?;
    let bundle = Bundle {
        format:"lince-workspace-stage01-fixtures".into(),schema:1,
        status:"fixture-inputs-only-runtime-milestones-pending".into(),
        prototype_boundary:"Private fixture/evidence schema, not the production workspace format, controller API or scientific validation.".into(),
        source_hash_algorithm:"sha256-file-bytes; distinct from accepted canonical document identity".into(),
        profile,assets,scenes:scenes(),cases:replay_cases,
    };
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&bundle).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(bundle)
}
