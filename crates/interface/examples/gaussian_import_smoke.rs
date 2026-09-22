use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    container::BoxRoot,
    topology::{assets, splats, view::View},
    workspace::WorkspaceMember,
};

#[derive(Resource, Clone, Default, bevy::render::extract_resource::ExtractResource)]
struct RenderCheck {
    hidden: bool,
}

fn check_hidden_work(
    state: Res<RenderCheck>,
    clouds: Query<
        &bevy_gaussian_splatting::CloudSettings,
        With<bevy_gaussian_splatting::PlanarGaussian3dHandle>,
    >,
) {
    if state.hidden {
        assert_eq!(
            clouds.iter().count(),
            0,
            "Hidden clouds must not remain in GPU sorting queries"
        );
    }
}
#[derive(Resource)]
struct Exercise {
    source: std::path::PathBuf,
    started: std::time::Instant,
    requested: bool,
    entity: Option<Entity>,
    next: u32,
    waiting: bool,
    images: Vec<Vec<u8>>,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    assert!(
        world.resource::<Exercise>().started.elapsed().as_secs() < 300,
        "Gaussian import smoke timed out"
    );
    let Some(root) = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if !world.resource::<Exercise>().requested && frame >= 8 {
        let path = world.resource::<Exercise>().source.clone();
        assets::import(world, root, path).unwrap();
        world.resource_mut::<Exercise>().requested = true;
        world.insert_resource(lince_interface::theme::idle_settings());
    }
    if world.resource::<Exercise>().entity.is_none() {
        let entity = world
            .query_filtered::<Entity, (With<splats::SplatHandle>, With<assets::Ready>)>()
            .iter(world)
            .next();
        let Some(entity) = entity else {
            return;
        };
        let mut spaces = world
            .get_mut::<lince_interface::workspace::Workspaces>(root)
            .unwrap();
        let mut hidden = spaces.entries[0].clone();
        hidden.id = 2;
        hidden.name = "Hidden cloud".into();
        spaces.entries.push(hidden);
        let saved = assets::snapshot(world, root);
        assert_eq!(saved.len(), 1);
        let serialized = serde_json::to_vec(&saved).unwrap();
        let restored: Vec<assets::SavedAsset> = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored[0].asset.file, "source.gcloud");
        let copy = assets::duplicate(world, entity).unwrap();
        assets::discard(world, copy);
        assert_eq!(
            world
                .query::<&lince_interface::topology::physics::Body>()
                .iter(world)
                .count(),
            0
        );
        let handle = &world.get::<splats::SplatHandle>(entity).unwrap().0;
        let splat = world
            .resource::<Assets<splats::SplatAsset>>()
            .get(handle)
            .unwrap();
        println!(
            "Loaded {} splats; bounds {:?} to {:?}; GPU storage binding limit {} bytes",
            splat.count,
            splat.bounds.min,
            splat.bounds.max,
            world
                .resource::<bevy::render::renderer::RenderDevice>()
                .limits()
                .max_storage_buffer_binding_size
        );
        let mut state = world.resource_mut::<Exercise>();
        state.entity = Some(entity);
        state.next = frame + 30;
    }
    world.resource::<lince_interface::wake::WakeSignal>().ring();
    let state = world.resource::<Exercise>();
    if state.waiting || frame < state.next {
        return;
    }
    let entity = state.entity.unwrap();
    let stage = state.images.len();
    if stage == 7 {
        for (shown, hidden) in [(0, 3), (1, 2), (4, 2), (6, 2)] {
            let changed = state.images[shown]
                .chunks_exact(4)
                .zip(state.images[hidden].chunks_exact(4))
                .filter(|(a, b)| {
                    a.iter()
                        .zip(b.iter())
                        .take(3)
                        .any(|(a, b)| a.abs_diff(*b) > 12)
                })
                .count();
            assert!(
                changed > 1000,
                "Gaussian rendering did not change enough pixels: {changed}"
            );
        }
        assets::discard(world, entity);
        assert_eq!(
            world.query::<&assets::ImportedAsset>().iter(world).count(),
            0
        );
        println!(
            "Gaussian import, persistence data, duplication, removal, flat and spatial rendering, workspace and frustum sorting culling, and reappearance passed"
        );
        world.write_message(AppExit::Success);
        return;
    }
    let path = match stage {
        0 => "/tmp/lince-gaussian-flat.png",
        1 => "/tmp/lince-gaussian-spatial.png",
        2 => "/tmp/lince-gaussian-hidden-spatial.png",
        3 => "/tmp/lince-gaussian-hidden-flat.png",
        4 => "/tmp/lince-gaussian-restored-workspace.png",
        5 => "/tmp/lince-gaussian-offscreen.png",
        _ => "/tmp/lince-gaussian-restored-frustum.png",
    };
    world.resource_mut::<RenderCheck>().hidden = matches!(stage, 2 | 3 | 5);
    world.resource_mut::<Exercise>().waiting = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>, mut commands: Commands| {
                let pixels = event.image.data.as_ref().unwrap().clone();
                commands.queue(move |world: &mut World| {
                    world.resource_mut::<Exercise>().images.push(pixels);
                    match stage {
                        0 => {
                            world.get_mut::<View>(root).unwrap().spatial = true;
                            assets::frame(world, entity);
                        }
                        1 => {
                            world.get_mut::<WorkspaceMember>(entity).unwrap().0 = 2;
                        }
                        2 => {
                            world.get_mut::<View>(root).unwrap().spatial = false;
                        }
                        3 => {
                            world.resource_mut::<RenderCheck>().hidden = false;
                            world.get_mut::<WorkspaceMember>(entity).unwrap().0 = 1;
                            world.get_mut::<View>(root).unwrap().spatial = true;
                            assets::frame(world, entity);
                        }
                        4 => {
                            world.get_mut::<View>(root).unwrap().position[0] += 1_000_000.0;
                        }
                        5 => {
                            world.resource_mut::<RenderCheck>().hidden = false;
                            assets::frame(world, entity);
                        }
                        _ => {}
                    }
                    let next = world.resource::<FrameCount>().0 + 20;
                    let mut state = world.resource_mut::<Exercise>();
                    state.next = next;
                    state.waiting = false;
                });
            },
        );
}

fn main() {
    let source = std::env::args_os()
        .nth(1)
        .expect("Usage: gaussian_import_smoke path.gcloud")
        .into();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let mut app = interface_app();
    app.add_plugins(bevy::render::extract_resource::ExtractResourcePlugin::<
        RenderCheck,
    >::default())
        .init_resource::<RenderCheck>();
    app.get_sub_app_mut(bevy::render::RenderApp)
        .unwrap()
        .add_systems(
            bevy::render::Render,
            check_hidden_work.after(bevy::render::RenderSystems::Prepare),
        );
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Exercise {
            source,
            started: std::time::Instant::now(),
            requested: false,
            entity: None,
            next: 0,
            waiting: false,
            images: Vec::new(),
        })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(
            Update,
            exercise.after(lince_interface::workspace::PrepareWorkspaces),
        )
        .run();
}
