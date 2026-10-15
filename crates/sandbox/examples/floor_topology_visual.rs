use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    math::DVec2,
    prelude::*,
    render::{
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use lince_sandbox::{
    actions::{Action, ActionButton},
    container::BoxRoot,
    edit_mode::EditAction,
    topology::{terrain::Effect, view::View},
};
use std::{path::PathBuf, time::Instant};

#[derive(Resource)]
struct Output(RenderTarget);
#[derive(Resource, Default)]
struct Captured(bool);

fn output(world: &mut World) {
    let target = RenderTarget::Image(ImageRenderTarget {
        handle: world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1280,
                960,
                TextureFormat::Bgra8UnormSrgb,
                None,
            )),
        scale_factor: 1.0,
    });
    let scene = world
        .resource::<lince_sandbox::topology::presentation::SceneCamera>()
        .0;
    let background = world
        .resource::<lince_sandbox::topology::presentation::BackgroundCamera>()
        .0;
    world.entity_mut(scene).insert(target.clone());
    world.entity_mut(background).insert(target.clone());
    world.insert_resource(Output(target));
}

fn step(app: &mut App) {
    app.update();
    app.world()
        .resource::<RenderDevice>()
        .wgpu_device()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
}

fn activate(world: &mut World, label: &str) {
    let button = world
        .query::<(&Text, &ChildOf)>()
        .iter(world)
        .find(|(text, parent)| {
            text.0 == label && world.get::<ActionButton>(parent.parent()).is_some()
        })
        .map(|(_, parent)| parent.parent())
        .unwrap_or_else(|| panic!("Control {label} is missing"));
    let action = world.get::<ActionButton>(button).unwrap();
    let (target, actions) = (action.target, action.actions.clone());
    actions.run(world, target);
}

fn capture(app: &mut App, path: PathBuf) {
    for _ in 0..12 {
        step(app);
    }
    app.world_mut().resource_mut::<Captured>().0 = false;
    let target = app.world().resource::<Output>().0.clone();
    app.world_mut()
        .spawn(Screenshot(target))
        .observe(save_to_disk(path.clone()))
        .observe(
            |capture: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                let data = capture.image.data.as_ref().unwrap();
                assert!(data.chunks_exact(4).any(|pixel| pixel != &data[..4]));
                captured.0 = true;
            },
        );
    let started = Instant::now();
    while !app.world().resource::<Captured>().0 {
        assert!(started.elapsed().as_secs() < 30);
        step(app);
    }
    println!("Captured {}", path.display());
}

#[tokio::main]
async fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/floor-topology-review".into()),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut app = lince_sandbox::app::offscreen_interface_app();
    app.add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(lince_sandbox::workspace::WorkspaceFile::new(
            workspace.path().join("interface.json"),
        ))
        .init_resource::<Captured>()
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1280.0, 960.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(PostStartup, output);
    app.finish();
    app.cleanup();
    for _ in 0..8 {
        step(&mut app);
    }
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    EditAction::Open.apply(app.world_mut(), root);
    activate(app.world_mut(), "Floor");
    for _ in 0..3 {
        step(&mut app);
    }
    activate(app.world_mut(), "Add valley");
    for _ in 0..3 {
        step(&mut app);
    }
    activate(app.world_mut(), "Add hill");
    let effects: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Effect)>()
        .iter(app.world())
        .map(|(e, f)| (e, f.0.preset))
        .collect();
    assert_eq!(effects.len(), 2, "Native controls must create both effects");
    for (entity, preset) in effects {
        app.world_mut()
            .get_mut::<lince_sandbox::canvas::CanvasItem>(entity)
            .unwrap()
            .position = DVec2::new(
            if preset == nucleus::terrain::Preset::Hill {
                -250.0
            } else {
                250.0
            },
            0.0,
        );
    }
    activate(app.world_mut(), "Undo");
    for _ in 0..3 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut().query::<&Effect>().iter(app.world()).count(),
        1
    );
    activate(app.world_mut(), "Redo");
    for _ in 0..3 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut().query::<&Effect>().iter(app.world()).count(),
        2
    );
    capture(&mut app, directory.join("flat.png"));
    app.world_mut().get_mut::<View>(root).unwrap().spatial = true;
    app.world_mut().get_mut::<View>(root).unwrap().position = [0.0, 750.0, 1200.0];
    app.world_mut().get_mut::<View>(root).unwrap().pitch = -0.6;
    capture(&mut app, directory.join("spatial.png"));
    assert!(
        app.world_mut()
            .query::<&lince_sandbox::topology::terrain::Fault>()
            .iter(app.world())
            .next()
            .is_none()
    );
    println!("Floor topology native controls and rendering passed");
}
