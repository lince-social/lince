use bevy::{math::DVec2, prelude::*, winit::WinitSettings};
use lince_desktop::{
    app::interface_app,
    canvas::CanvasItem,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    tokens::{ThemeSettings, file::ThemeFile},
    workspace::Workspaces,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Preview {
    kind: SandKind,
    mobile: bool,
    theme: Option<ThemeFile>,
    polled: Instant,
    status: String,
}

#[derive(Component)]
struct Subject;

fn setup(world: &mut World) {
    let root = world.spawn((BoxRoot, Workspaces::default())).id();
    let kind = world.resource::<Preview>().kind;
    let subject = spawn_sand(world, root, 1, kind, "Design preview", DVec2::ZERO);
    world.entity_mut(subject).insert(Subject);
    let mobile = world.resource::<Preview>().mobile;
    let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
    window.title = format!("Lince design · {} · isolated preview", kind.name());
    window.resolution.set(
        if mobile { 390.0 } else { 1000.0 },
        if mobile { 844.0 } else { 800.0 },
    );
}

fn update(world: &mut World) {
    let due = world.resource::<Preview>().polled.elapsed() >= Duration::from_millis(350);
    if due {
        let result = {
            let mut preview = world.resource_mut::<Preview>();
            preview.polled = Instant::now();
            preview.theme.as_mut().map(ThemeFile::poll)
        };
        match result {
            Some(Ok(Some(theme))) => {
                world.insert_resource(theme);
                eprintln!("Preview theme reloaded");
                world.resource_mut::<Preview>().status = "Theme reloaded".into();
            }
            Some(Err(error)) => {
                eprintln!("Theme edit rejected; retaining the last valid theme: {error}");
                world.resource_mut::<Preview>().status = format!("Theme error: {error}");
            }
            _ => {}
        }
    }
    let preview = world.resource::<Preview>();
    let title = format!(
        "Lince design · {} · isolated · {}",
        preview.kind.name(),
        preview.status
    );
    let mobile = preview.mobile;
    let window_size = {
        let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
        window.title = title;
        window.resolution.size()
    };
    if mobile {
        for mut item in world
            .query_filtered::<&mut CanvasItem, With<Subject>>()
            .iter_mut(world)
        {
            item.size.x = (window_size.x - 24.0).max(48.0);
            item.size.y = item.size.y.min((window_size.y - 48.0).max(48.0));
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let component = args.next().unwrap_or_else(|| "square".into());
    let kind = match component.as_str() {
        "square" => SandKind::Square,
        "text" => SandKind::Text,
        "editable-text" => SandKind::EditableText,
        "organ" => SandKind::Organ,
        "time-castle" => SandKind::WorkTimer,
        "todo" => SandKind::Todo,
        "configuration" => SandKind::Configuration,
        "access-control" => SandKind::AccessControl,
        "sync" => SandKind::Sync,
        "operation" => SandKind::Operation,
        "ontology" => SandKind::Ontology,
        _ => panic!("Unknown preview component"),
    };
    let mut theme = None;
    let mut mobile = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--theme" => {
                theme = Some(ThemeFile::new(PathBuf::from(
                    args.next().expect("Missing theme path"),
                )))
            }
            "--mobile" => mobile = true,
            _ => panic!("Unknown preview argument: {argument}"),
        }
    }
    let initial_theme = theme
        .as_mut()
        .map(ThemeFile::poll)
        .transpose()
        .expect("Valid preview theme")
        .flatten()
        .unwrap_or_else(ThemeSettings::default);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    interface_app()
        .insert_resource(initial_theme)
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Preview {
            kind,
            mobile,
            theme,
            polled: Instant::now(),
            status: "Synthetic local state; no connected Cell".into(),
        })
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .add_systems(Update, close)
        .run();
}

fn close(
    mut events: MessageReader<bevy::window::WindowCloseRequested>,
    mut exit: MessageWriter<AppExit>,
) {
    if events.read().next().is_some() {
        exit.write(AppExit::Success);
    }
}
