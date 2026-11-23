//! B39 in the shipped workbench. Every mutation is made through a native control.
use super::{control, fill, inspect, receipts::click, screenshot, selected};
use bevy::{
    a11y::AccessibilityNode,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use nucleus::canvas::scene::{DefinitionKind, Pin, Scene, Slot};
use serde_json::{Value, json};
use std::{fs, path::Path};
use store::workspace_scene as storage;

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Import,
    Reopen,
}
const BRACKET_A: &str = "040bbc352c8a";
const BRACKET_B: &str = "8303a75c2bca";
const STEEL_B: &str = "b86b61906deb";
fn short(pin: &Pin) -> &str {
    &pin.revision.as_str()[7..19]
}
fn visible(world: &mut World, prefix: &str) -> String {
    world
        .query::<&Text>()
        .iter(world)
        .find(|t| t.0.starts_with(prefix))
        .unwrap_or_else(|| panic!("Missing visible text {prefix}"))
        .0
        .clone()
}
fn button_exists(world: &mut World, caption: &str) -> bool {
    world
        .query_filtered::<&AccessibilityNode, With<lince_sandbox::actions::ActionButton>>()
        .iter(world)
        .any(|n| n.label() == Some(caption))
}
fn placement_count(world: &mut World) -> usize {
    world
        .query_filtered::<&AccessibilityNode, With<lince_sandbox::actions::ActionButton>>()
        .iter(world)
        .filter_map(|node| node.label())
        .filter(|label| label.starts_with("Placement slot_") || label.starts_with("Selected slot_"))
        .count()
}
fn write(path: &Path, v: &Value) {
    fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn verify_imports(scene: &Scene, before: &Value) {
    let original: Vec<Slot> = serde_json::from_value(before["slots"].clone()).unwrap();
    assert_eq!(&scene.slots[..original.len()], original.as_slice());
    assert_eq!(json!(&scene.canvas), before["binding"]);
    let (a, b) = (&scene.slots[3], &scene.slots[4]);
    let (pa, pb) = (a.pin(&scene.document), b.pin(&scene.document));
    assert_eq!(pa.part, pb.part);
    assert_eq!(pa.document, pb.document);
    assert_eq!((short(&pa), short(&pb)), (BRACKET_A, BRACKET_B));
    assert_ne!(a.id, b.id);
    assert!(a.record.is_none() && b.record.is_none());
    assert_eq!(a.metres.map(|x| x.to_string()), ["0", "0", "0"]);
    let material = |p: &Pin| {
        scene
            .definition(&scene.definition(p).unwrap().requires[0])
            .unwrap()
    };
    let (ma, mb) = (material(&pa), material(&pb));
    assert_eq!(ma.id, mb.id);
    assert_eq!(
        ma.intrinsic,
        DefinitionKind::Material {
            grade: "A36".into()
        }
    );
    assert_eq!(
        mb.intrinsic,
        DefinitionKind::Material {
            grade: "304".into()
        }
    );
    assert_eq!(ma.requires, mb.requires);
    assert_eq!(
        scene
            .parts
            .iter()
            .filter(|p| p.name == "Millimetre")
            .count(),
        1
    );
    assert!(scene.parts.iter().all(|p| !matches!(
        p.name.as_str(),
        "Spacer" | "Coated bracket" | "Workshop coating"
    )));
}

pub(super) fn replay(
    world: &mut World,
    frame: u32,
    directory: &Path,
    workspace: u64,
    phase: Phase,
) {
    let destination = directory.join(format!("workspaces/{workspace}/scene.workspace"));
    let source = destination.with_file_name("showoff-parts.workspace");
    let offline = directory.join("offline-example-library.workspace");
    let prior = directory.join("before-import.json");
    let evidence = directory.join("definition-import.json");
    let disk = || storage::open(&destination).unwrap();
    if matches!(phase, Phase::Reopen) {
        match frame {
            36 => {
                assert!(!source.exists());
                click(world, "Definition library");
            }
            40 => {
                fill(world, "Source scene directory", source.to_str().unwrap());
                click(world, "Browse source definitions");
            }
            44 => {
                visible(world, "Source unavailable.");
                click(world, &format!("Inspect Bracket · {BRACKET_A}"));
            }
            48 => {
                visible(world, "Document definition · Bracket");
                assert!(
                    visible(world, "Requires Steel · Material · grade A36")
                        .contains("ccdafc930dc2")
                );
                click(world, &format!("Inspect Bracket · {BRACKET_B}"));
            }
            52 => {
                assert!(visible(world, "Requires Steel · Material · grade 304").contains(STEEL_B));
                let scene = disk();
                let proof = read(&evidence);
                assert_eq!(json!(scene.revision), proof["head"]);
                assert_eq!(json!(scene.scene.slots), proof["slots"]);
                assert_eq!(json!(scene.scene.parts), proof["definitions"]);
                assert_eq!(json!(scene.last_receipt), proof["receipt"]);
                verify_imports(&scene.scene, &read(&prior));
                let e = control(world, "Place this definition");
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(e, bevy::input_focus::FocusCause::Navigated);
            }
            60 => {
                let (viewport, _, _, button) =
                    super::control_layout(world, "Place this definition");
                write(
                    &directory.join("definition-layout.json"),
                    &json!({"viewport":viewport.size().to_array(),"control_min":button.min.to_array(),"control_max":button.max.to_array()}),
                );
                super::pointer::pointer(world, button.center());
            }
            64 => {
                world
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(directory.join("definitions-reopened.png")));
                super::pointer::button(world, true);
            }
            68 => super::pointer::button(world, false),
            72 => {
                visible(world, "Placed a new occurrence at the local origin.");
                let added = selected(world);
                let saved = disk().scene;
                assert!(saved.slots.iter().all(|s| s.id.as_str() != added));
                assert_eq!(placement_count(world), saved.slots.len() + 1);
                assert_eq!(inspect(world, "X (m)"), "0");
                click(world, "Undo scene edit");
            }
            76 => {
                visible(world, "Restored the scene edit.");
                let saved = disk().scene;
                assert_eq!(placement_count(world), saved.slots.len());
                assert_eq!(json!(saved.slots), read(&evidence)["slots"]);
                let layout_path = directory.join("definition-layout.json");
                let mut proof = read(&layout_path);
                proof["pointer_placed_and_undo"] = json!(true);
                write(&layout_path, &proof);
                fs::rename(&offline, &source).unwrap();
                world
                    .resource::<super::Replay>()
                    .complete
                    .store(true, std::sync::atomic::Ordering::Release);
                world.write_message(AppExit::Success);
            }
            _ => {}
        }
        return;
    }
    match frame {
        36 => {
            let s = disk().scene;
            assert_eq!(s.slots.len(), 3);
            write(
                &prior,
                &json!({"slots":s.slots,"binding":s.canvas,"document":s.document,"parts":s.parts}),
            );
            click(world, "Definition library");
        }
        40 => click(world, "Browse the example parts library"),
        44 => {
            assert_eq!(
                inspect(world, "Source scene directory"),
                source.to_str().unwrap()
            );
            click(world, "Browse source definitions");
        }
        48 => click(world, &format!("Review Bracket · {BRACKET_A}")),
        50 => click(world, "Show source and revision details"),
        52 => {
            assert!(visible(world, "Bring in · Steel").contains("grade A36"));
            assert!(visible(world, "Bring in · Steel").contains("Source doc_"));
            assert!(button_exists(world, "Hide source and revision details"));
            visible(world, "Bring in · Millimetre");
            assert!(visible(world, "Bring in · Bracket").contains("Conflict · keep both"));
            click(world, "Import reviewed definitions");
        }
        56 => {
            visible(world, "Imported 3 definitions. Reused 0.");
            click(world, "Place this definition");
        }
        60 => click(world, "Save scene"),
        64 => {
            let s = disk().scene;
            assert_eq!(s.slots.len(), 4);
            assert_eq!(short(&s.slots[3].pin(&s.document)), BRACKET_A);
            assert_eq!(selected(world), s.slots[3].id.as_str());
            click(world, &format!("Review Bracket · {BRACKET_B}"));
        }
        66 => {
            assert!(visible(world, "Bring in · Steel").contains("grade 304"));
            assert!(button_exists(world, "Show source and revision details"));
            assert!(visible(world, "Bring in · Steel").contains("Conflict · keep both"));
            visible(world, "Reuse · Millimetre");
            let control = control(world, "Import reviewed definitions");
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .set(control, bevy::input_focus::FocusCause::Navigated);
        }
        69 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(directory.join("definition-review.png")));
        }
        71 => click(world, "Import reviewed definitions"),
        72 => {
            visible(world, "Imported 2 definitions. Reused 1.");
            click(world, "Place this definition");
        }
        76 => click(world, "Save scene"),
        80 => {
            let s = disk().scene;
            assert_eq!(s.slots.len(), 5);
            verify_imports(&s, &read(&prior));
            click(world, "Browse source definitions");
        }
        84 => click(world, &format!("Review Bracket · {BRACKET_B}")),
        88 => {
            visible(world, "Reuse · Millimetre");
            visible(world, "Reuse · Steel");
            visible(world, "Reuse · Bracket");
            click(world, "Import reviewed definitions");
        }
        92 => {
            visible(world, "Imported 0 definitions. Reused 3.");
            let catalog = storage::library::Catalog::open(&source).unwrap();
            let coated = catalog
                .available()
                .find(|e| e.name.as_deref() == Some("Coated bracket"))
                .unwrap();
            click(
                world,
                &format!("Review Coated bracket · {}", short(&coated.pin)),
            );
        }
        96 => {
            assert!(
                visible(world, "Unavailable · Required definition")
                    .contains("keeps this definition local")
            );
            assert!(!button_exists(world, "Import reviewed definitions"));
            fill(world, "New part name", "Workbench mounting plate");
            click(world, "Create part and place it");
        }
        100 => click(world, &format!("Inspect Steel · {STEEL_B}")),
        104 => click(world, "Require for the selected placement"),
        108 => {
            visible(world, "The selected placement now pins a local revision");
            click(world, "Undo scene edit");
        }
        112 => click(world, "Redo scene edit"),
        116 => click(world, "Save scene"),
        120 => {
            let s = disk().scene;
            verify_imports(&s, &read(&prior));
            let local = s.slots.last().unwrap();
            assert!(local.source.is_none());
            let part = s.definition(&local.pin(&s.document)).unwrap();
            assert_eq!(part.name, "Workbench mounting plate");
            assert_eq!(short(&part.requires[0]), STEEL_B);
            click(world, &format!("Placement {}", s.slots[4].id.as_str()));
        }
        124 => click(world, "Accepted edits and measurements"),
        128 => {
            fill(world, "X (m)", "0.375");
            click(world, "Publish placement coordinates");
        }
        132 => {
            let s = disk();
            assert_eq!(s.scene.slots.len(), 6);
            verify_imports(&s.scene, &read(&prior));
            let b = &s.scene.slots[4];
            assert_eq!(b.metres[0].to_string(), "0.375");
            assert_eq!(s.last_receipt.as_ref().unwrap().handle, s.scene.handle(b));
            click(world, &format!("Inspect Steel · {STEEL_B}"));
        }
        136 => click(world, "Keep this definition local"),
        140 => click(world, "Save scene"),
        144 => {
            fill(
                world,
                "Source scene directory",
                destination.to_str().unwrap(),
            );
            click(world, "Browse source definitions");
        }
        148 => {
            assert!(!button_exists(world, &format!("Review Steel · {STEEL_B}")));
            click(world, &format!("Review Bracket · {BRACKET_B}"));
        }
        152 => {
            visible(world, "Unavailable · Required definition");
            assert!(!button_exists(world, "Import reviewed definitions"));
            click(world, "Allow this definition to be imported");
        }
        156 => click(world, "Save scene"),
        160 => click(world, "Browse source definitions"),
        164 => click(world, &format!("Review Bracket · {BRACKET_B}")),
        168 => {
            visible(world, "Reuse · Bracket");
            visible(world, "Reuse · Steel");
            visible(world, "Reuse · Millimetre");
            let s = disk();
            verify_imports(&s.scene, &read(&prior));
            let b = &s.scene.slots[4];
            assert_eq!(s.last_receipt.as_ref().unwrap().handle, s.scene.handle(b));
            write(
                &evidence,
                &json!({"head":s.revision,"source_document":b.source,"slots":s.scene.slots,"definitions":s.scene.parts,"receipt":s.last_receipt,"republishing_permission":"withheld, blocked dependent import, then restored and verified"}),
            );
            assert!(!offline.exists());
            fs::rename(&source, &offline).unwrap();
            screenshot(world, directory.join("placed-import.png"));
        }
        _ => {}
    }
}
