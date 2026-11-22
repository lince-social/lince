use super::*;
use lince_sandbox::{sand_store::StoredSand, sand_text, workspace::WorkspaceMember};

pub(super) fn run(directory: PathBuf) {
    assert!(directory.is_dir());
    let mut app = lince_sandbox::app::interface_app();
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, move |world: &mut World| {
            if world.resource::<FrameCount>().0 == 4 {
                verify(world, &directory);
                world.write_message(AppExit::Success);
            }
        });
    app.run();
}

fn verify(world: &mut World, directory: &std::path::Path) {
    let active = world.query::<&Workspaces>().single(world).unwrap().active;
    assert_eq!(active, 1);
    let mut observed = serde_json::Map::new();
    let mut sands = world.query::<(&StoredSand, &CanvasItem, &WorkspaceMember, &Spatial)>();
    for (sand, item, member, spatial) in sands.iter(world) {
        let Some(content) = sand.content else {
            continue;
        };
        let label = sand_text::value(world, content);
        match label.as_str() {
            "Move this work" => {
                assert_eq!(member.0, active);
                assert!(item.position.distance(AREA_TARGET) <= 1.0);
            }
            "Gravity and settling" => {
                assert_eq!(member.0, active);
                assert!((spatial.elevation - 10.0).abs() < 1.0);
            }
            "Other workspace stays still" => {
                assert_ne!(member.0, active);
                assert_eq!(item.position, SAND_START);
            }
            _ => continue,
        }
        let pose = json!({
            "workspace": member.0,
            "position": item.position.to_array(),
            "elevation": spatial.elevation
        });
        assert!(observed.insert(label, pose).is_none());
    }
    assert_eq!(observed.len(), 3);
    fs::write(
        directory.join("reopen.json"),
        serde_json::to_vec_pretty(&observed).unwrap(),
    )
    .unwrap();
}
