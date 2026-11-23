use super::{
    Replay, click, fill, inspect,
    pointer::{button, pointer},
    screenshot, selected,
};
use bevy::{math::DVec2, prelude::*};
use lince_sandbox::{
    canvas::CanvasItem,
    canvas_host::Identity,
    canvas_selection::SandSelection,
    container::BoxRoot,
    item_properties::Attachment,
    sand_store::{SandKind, spawn_sand},
    topology::{
        presentation,
        rooms::{Member, Room},
    },
};
use nucleus::canvas::{room, scene::Scene};
use std::{fs, path::Path, sync::atomic::Ordering};

#[derive(Resource)]
struct Fixture {
    item: Entity,
    room_id: String,
    item_id: String,
    record: String,
    start: DVec2,
    origin: DVec2,
    screen: Vec2,
    saved: Option<Scene>,
}

const START: DVec2 = DVec2::new(240.0, -100.0);
const ROOM: DVec2 = DVec2::new(-260.0, -200.0);

fn make_fixture(world: &mut World, root: Entity) {
    let r = spawn_sand(world, root, 1, SandKind::Text, "Room", ROOM);
    let item = spawn_sand(world, root, 1, SandKind::Text, "Linked fixture", START);
    let room_id = nucleus::new_uid("placement");
    let item_id = nucleus::new_uid("placement");
    let record = nucleus::new_uid("r");
    world
        .entity_mut(r)
        .insert((Identity(room_id.clone()), Room(room::Room::default())));
    let mut local = room::identity_pose();
    local.position = [500.0, 0.0, 100.0];
    world.entity_mut(item).insert((
        Identity(item_id.clone()),
        Attachment {
            uid: record.clone(),
        },
        Member(room::Member {
            room: room_id.clone(),
            parent: None,
            local,
            role: room::Role::Content,
        }),
    ));
    world.insert_resource(Fixture {
        item,
        room_id,
        item_id,
        record,
        start: START,
        origin: ROOM,
        screen: Vec2::ZERO,
        saved: None,
    });
}

fn disk(directory: &Path, workspace: u64) -> Scene {
    store::workspace_scene::open(&directory.join(format!("workspaces/{workspace}/scene.workspace")))
        .unwrap()
        .scene
}

// Read the installed exhibit from the same persisted workspace a user opens.
fn showcase_fixture(world: &mut World, root: Entity) {
    let spaces = world
        .get::<lince_sandbox::workspace::Workspaces>(root)
        .unwrap();
    let ws = spaces
        .entries
        .iter()
        .find(|e| e.id == spaces.active)
        .unwrap();
    assert_eq!(ws.name, "showoff");
    let id = ws.showoff_bracket.clone().unwrap();
    let mut query = world.query::<(Entity, &Identity)>();
    let mut found = query.iter(world).filter(|(_, i)| i.0 == id);
    let item = found.next().unwrap().0;
    assert!(found.next().is_none());
    let member = world.get::<Member>(item).unwrap();
    let room_id = member.0.room.clone();
    let room = query.iter(world).find(|(_, i)| i.0 == room_id).unwrap().0;
    world.insert_resource(Fixture {
        item,
        item_id: id,
        room_id,
        record: world.get::<Attachment>(item).unwrap().uid.clone(),
        start: world.get::<CanvasItem>(item).unwrap().position,
        origin: world.get::<CanvasItem>(room).unwrap().position,
        screen: Vec2::ZERO,
        saved: None,
    });
}

fn point(world: &mut World, root: Entity) {
    let e = world.resource::<Fixture>().item;
    let center = if world.get::<presentation::SpatialRoot>(root).is_some() {
        presentation::projected_corners(world, e)
            .unwrap()
            .iter()
            .map(|p| DVec2::from_array(*p))
            .sum::<DVec2>()
            .as_vec2()
            / 4.0
    } else {
        super::bounds(world, e).center()
    };
    world.resource_mut::<Fixture>().screen = center;
    pointer(world, center);
}

pub(super) fn replay(world: &mut World, frame: u32, directory: &Path, reopen: bool) {
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    let workspace = world
        .get::<lince_sandbox::workspace::Workspaces>(root)
        .unwrap()
        .active;
    let showcase = world.resource::<Replay>().showcase;
    if frame == 24 && showcase {
        showcase_fixture(world, root);
    }
    if reopen {
        match frame {
            24 => click(world, "Open directory"),
            28 => {
                let saved = disk(directory, workspace);
                let binding = saved.canvas.as_ref().unwrap();
                if showcase {
                    assert_eq!(
                        binding.placement,
                        world.resource::<Fixture>().item_id,
                        "revisiting the showcase must retain the edited exhibit"
                    );
                }
                let item = world
                    .query::<(Entity, &Identity)>()
                    .iter(world)
                    .find(|(_, i)| i.0 == binding.placement)
                    .unwrap()
                    .0;
                assert_eq!(world.get::<Member>(item).unwrap().0.room, binding.room);
                assert_eq!(
                    world.get::<Attachment>(item).unwrap().uid,
                    saved.slot(&binding.handle).unwrap().record.clone().unwrap()
                );
                let expected = if showcase {
                    DVec2::new(-240., 640.)
                } else {
                    START
                };
                assert_eq!(world.get::<CanvasItem>(item).unwrap().position, expected);
                assert_eq!(inspect(world, "X (m)"), "5");
                assert_eq!(selected(world), binding.handle.slot.as_str());
                click(world, &format!("Selected {}", binding.handle.slot.as_str()));
            }
            32 => {
                assert_eq!(world.get::<SandSelection>(root).unwrap().0.len(), 1);
                let item = world.get::<SandSelection>(root).unwrap().0[0];
                assert_eq!(
                    world.get::<Identity>(item).unwrap().0,
                    disk(directory, workspace).canvas.unwrap().placement
                );
                if showcase {
                    use lince_sandbox::topology::assets::{AssetDirectory, ImportedAsset, Ready};
                    let mut assets = world.query_filtered::<&ImportedAsset, With<Ready>>();
                    let sculpture = assets
                        .iter(world)
                        .find(|asset| asset.name == "Showoff sculpture")
                        .expect("the shipped sculpture loaded in this fresh process");
                    assert!(
                        world
                            .resource::<AssetDirectory>()
                            .0
                            .join(&sculpture.id)
                            .join(&sculpture.file)
                            .is_file()
                    );
                    if !world.resource::<Replay>().recover
                        && world.resource::<Replay>().library.is_none()
                    {
                        world
                            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                            .observe(bevy::render::view::screenshot::save_to_disk(
                                directory.join("reopened.png"),
                            ));
                    }
                } else {
                    screenshot(world, directory.join("reopened.png"));
                }
            }
            _ if showcase && frame >= 36 => {
                if let Some(phase) = world.resource::<Replay>().library {
                    super::library::replay(world, frame, directory, workspace, phase);
                } else {
                    super::receipts::replay(world, frame, directory, workspace);
                }
            }
            _ => {}
        }
        return;
    }
    match frame {
        24 => {
            if !showcase {
                make_fixture(world, root);
            }
            click(world, "New scene");
        }
        28 => {
            fill(world, "New part name", "Bracket");
            click(world, "Create part and place it");
        }
        32 => click(world, "Place another copy of this part"),
        36 => point(world, root),
        38 => button(world, true),
        40 => button(world, false),
        44 => {
            assert_eq!(
                world.get::<SandSelection>(root).unwrap().0,
                [world.resource::<Fixture>().item]
            );
            click(world, "Link selected Room member to this placement");
        }
        48 => {
            assert_eq!(inspect(world, "X (m)"), "5");
            click(world, "Save scene");
        }
        52 => {
            let scene = disk(directory, workspace);
            let f = world.resource::<Fixture>();
            let b = scene.canvas.as_ref().unwrap();
            assert_eq!((&b.room, &b.placement), (&f.room_id, &f.item_id));
            assert_eq!(scene.slots[1].record.as_ref(), Some(&f.record));
            assert_eq!(scene.handle(&scene.slots[1]), b.handle);
            assert_eq!(scene.slots[0].part, scene.slots[1].part);
            click(world, &format!("Placement {}", scene.slots[0].id.as_str()));
            world.resource_mut::<Fixture>().saved = Some(scene);
        }
        56 => point(world, root),
        58 => button(world, true),
        60 | 62 => {
            if frame == 60 {
                let slot = world.resource::<Fixture>().saved.as_ref().unwrap().slots[1]
                    .id
                    .as_str()
                    .to_owned();
                assert_eq!(
                    selected(world),
                    slot,
                    "canvas selection must resolve the linked occurrence before movement"
                );
            }
            let delta = if frame == 60 {
                Vec2::new(35.0, 17.0)
            } else {
                Vec2::new(80.0, 32.0)
            };
            pointer(world, world.resource::<Fixture>().screen + delta);
        }
        64 => button(world, false),
        68 => {
            let f = world.resource::<Fixture>();
            let moved = world.get::<CanvasItem>(f.item).unwrap().position;
            let (start, origin) = (f.start, f.origin);
            assert!(moved.distance(start) > 10.0);
            let slot = f.saved.as_ref().unwrap().slots[1].id.as_str().to_owned();
            assert_eq!(selected(world), slot);
            let x = inspect(world, "X (m)").parse::<f64>().unwrap();
            let z = inspect(world, "Z (m)").parse::<f64>().unwrap();
            assert!((moved - origin - DVec2::new(x, z) * 100.0).length() < 1e-8);
            fs::write(directory.join("drag.json"), serde_json::to_vec_pretty(&serde_json::json!({
                "canvas_before": start.to_array(), "canvas_after": moved.to_array(), "room_origin":origin.to_array(), "model_xz_metres":[x,z]
            })).unwrap()).unwrap();
            click(world, "Undo scene edit");
        }
        72 => {
            let f = world.resource::<Fixture>();
            assert_eq!(world.get::<CanvasItem>(f.item).unwrap().position, f.start);
            assert_eq!(inspect(world, "X (m)"), "5");
            click(
                world,
                if showcase {
                    "Redo scene edit"
                } else {
                    "Save scene"
                },
            );
        }
        74 if showcase => click(world, "Save scene"),
        78 if showcase => {
            let moved = disk(directory, workspace);
            let original = world.resource::<Fixture>().saved.as_ref().unwrap();
            assert_eq!(original.canvas, moved.canvas);
            assert_eq!(original.slots[0], moved.slots[0]);
            assert_eq!(original.slots[1].record, moved.slots[1].record);
            assert_ne!(original.slots[1].metres, moved.slots[1].metres);
            click(world, "Undo scene edit");
        }
        80 if showcase => click(world, "Save scene"),
        84 if showcase => super::edit_action(world, super::EditAction::SwitchWorkspace(1)),
        88 if showcase => {
            assert_eq!(workspace, 1);
            let spaces = world
                .get::<lince_sandbox::workspace::Workspaces>(root)
                .unwrap();
            let id = spaces
                .entries
                .iter()
                .find(|e| e.name == "showoff")
                .unwrap()
                .id;
            assert!(
                spaces
                    .entries
                    .iter()
                    .find(|e| e.id == 1)
                    .unwrap()
                    .showoff_bracket
                    .is_none()
            );
            super::edit_action(world, super::EditAction::SwitchWorkspace(id));
        }
        76 if !showcase => {
            assert_eq!(
                &disk(directory, workspace),
                world.resource::<Fixture>().saved.as_ref().unwrap()
            );
            world
                .resource::<Replay>()
                .complete
                .store(true, Ordering::Release);
            world.write_message(AppExit::Success);
        }
        96 if showcase => {
            assert_ne!(workspace, 1);
            assert_eq!(
                &disk(directory, workspace),
                world.resource::<Fixture>().saved.as_ref().unwrap()
            );
            {
                let mut falling = world.query::<(&Name, &lince_sandbox::topology::Spatial)>();
                let (_, body) = falling
                    .iter(world)
                    .find(|(n, _)| n.as_str() == "Showoff · gravity")
                    .unwrap();
                assert!(
                    body.elevation < 60.0,
                    "gravity should lower the released Sand, observed {}",
                    body.elevation
                );
                let mut pebbles =
                    world.query::<(&CanvasItem, &lince_sandbox::area::RecordProperties)>();
                let moved = pebbles
                    .iter(world)
                    .filter(|(_, p)| p.0["kind"] == "showoff-physics")
                    .any(|(item, _)| {
                        [[435., 270.], [800., 300.], [590., 475.]]
                            .iter()
                            .all(|p| item.position.distance(DVec2::from_array(*p)) > 1.)
                    });
                assert!(
                    moved,
                    "the showcase Area should attract its matching Sands during the model edit"
                );
            }
            world
                .resource::<Replay>()
                .complete
                .store(true, Ordering::Release);
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}
