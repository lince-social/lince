use bevy::{
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PrimaryWindow,
    winit::WinitSettings,
};
use lince_interface::{
    container::BoxRoot,
    document_viewer::{DocumentViewer, spawn},
    workspace::{WorkspaceFile, Workspaces},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Exercise {
    directory: PathBuf,
    restore: bool,
    stage: u8,
    started: Instant,
    delay: Instant,
}

fn setup(world: &mut World) {
    world.spawn(BoxRoot);
    if let Ok(mut window) = world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .single_mut(world)
    {
        window.resolution.set(1640.0, 1020.0);
    }
}

fn descendant(world: &World, child: Entity, parent: Entity) -> bool {
    let mut entity = child;
    while let Some(ancestor) = world.get::<ChildOf>(entity) {
        entity = ancestor.parent();
        if entity == parent {
            return true;
        }
    }
    false
}

fn capture(world: &mut World, name: &str, exit: bool) {
    let path = world.resource::<Exercise>().directory.join(name);
    let mut screenshot = world.spawn(Screenshot::primary_window());
    screenshot.observe(save_to_disk(path));
    if exit {
        screenshot.observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    }
}

fn exercise(world: &mut World) {
    let test = world.resource::<Exercise>();
    assert!(
        test.started.elapsed() < Duration::from_secs(120),
        "Document viewer smoke timed out"
    );
    let stage = test.stage;
    let restore = test.restore;
    let Ok(root) = world
        .query_filtered::<Entity, (With<BoxRoot>, With<Workspaces>)>()
        .single(world)
    else {
        return;
    };
    if stage == 0 {
        if !restore {
            let directory = world.resource::<Exercise>().directory.clone();
            for (name, x) in [("sample.pdf", -390.0), ("sample.epub", 390.0)] {
                spawn(
                    world,
                    root,
                    1,
                    DVec2::new(x, 0.0),
                    DocumentViewer::with_path(directory.join(name).to_string_lossy()),
                );
            }
        }
        world.resource_mut::<Exercise>().stage = 1;
        return;
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<DocumentViewer>>()
        .iter(world)
        .collect();
    assert_eq!(owners.len(), 2);
    let images: Vec<_> = world
        .query_filtered::<Entity, With<ImageNode>>()
        .iter(world)
        .collect();
    let ready = owners.iter().all(|owner| {
        images.iter().any(|image| descendant(world, *image, *owner))
            && world
                .get::<InheritedVisibility>(*owner)
                .is_some_and(|visibility| visibility.get())
    });
    if !ready {
        return;
    }
    if stage == 1 {
        world.resource_mut::<Exercise>().stage = 2;
        world.resource_mut::<Exercise>().delay = Instant::now();
        return;
    }
    if world.resource::<Exercise>().delay.elapsed() < Duration::from_secs(2) {
        return;
    }
    if stage == 2 && restore {
        for owner in owners {
            let state = world.get::<DocumentViewer>(owner).unwrap();
            let position = state.position();
            if state.path.ends_with(".epub") {
                assert!((position.fraction - 0.6).abs() < 0.02);
            } else {
                assert_eq!(position.mode, lince_document::Mode::Pages);
                assert_eq!(position.section, 1);
            }
        }
        capture(world, "native-restored.png", true);
        world.resource_mut::<Exercise>().stage = 4;
    } else if stage == 2 {
        capture(world, "native-open.png", false);
        world.resource_mut::<Exercise>().stage = 5;
        world.resource_mut::<Exercise>().delay = Instant::now();
    } else if stage == 5 {
        for owner in owners {
            let state = world.get::<DocumentViewer>(owner).unwrap();
            if state.path.ends_with(".epub") {
                let viewport = world
                    .query::<(Entity, &ScrollPosition, &ComputedNode)>()
                    .iter(world)
                    .find(|(entity, _, _)| descendant(world, *entity, owner))
                    .map(|(entity, _, node)| {
                        (
                            entity,
                            (node.content_size().y - node.size().y) * node.inverse_scale_factor,
                        )
                    })
                    .unwrap();
                world.get_mut::<ScrollPosition>(viewport.0).unwrap().0.y = viewport.1 * 0.6;
            } else {
                let buttons: Vec<_> = world
                    .query::<(
                        &lince_interface::icons::Tooltip,
                        &lince_interface::actions::ActionButton,
                    )>()
                    .iter(world)
                    .filter(|(_, button)| button.target == owner)
                    .map(|(tip, button)| (tip.0.clone(), button.actions.clone()))
                    .collect();
                for title in ["Scroll mode", "Next"] {
                    buttons
                        .iter()
                        .find(|(tip, _)| tip == title)
                        .unwrap()
                        .1
                        .run(world, owner);
                }
            }
        }
        world.resource_mut::<Exercise>().stage = 3;
        world.resource_mut::<Exercise>().delay = Instant::now();
    } else if stage == 3 {
        for owner in &owners {
            let count = images
                .iter()
                .filter(|image| descendant(world, **image, *owner))
                .count();
            assert!(count <= 10, "Document tiles were not evicted");
        }
        capture(world, "native-progress.png", true);
        world.resource_mut::<Exercise>().stage = 4;
    }
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide the generated fixture directory"),
    );
    let restore = std::env::args().nth(2).as_deref() == Some("restore");
    let mut app = lince_interface::app::interface_app();
    app.insert_resource(WorkspaceFile::new(directory.join("interface.json")))
        .insert_resource(Exercise {
            directory,
            restore,
            stage: 0,
            started: Instant::now(),
            delay: Instant::now(),
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            exercise.after(lince_interface::workspace::PrepareWorkspaces),
        );
    app.run();
}
