use bevy::{log::LogPlugin, prelude::*, render::RenderPlugin};
pub use lince_interface::native::app::CellHandle;
use lince_interface::native::app::canvas;

pub fn run_native_interface(
    runtime: cell::CellRuntime,
    instance: &crate::instance::InstanceGuard,
    data_dir: &std::path::Path,
    tray_enabled: bool,
    backup_handoff: crate::owner_backup::BackupHandoff,
    laboratory: Option<cell::fiote::laboratory::Options>,
    showoff: bool,
) -> std::io::Result<()> {
    tokio::runtime::Handle::current()
        .block_on(lince_interface::native::app::initialize_language(&runtime))?;
    let storage = tokio::runtime::Handle::current().block_on(runtime.interface_storage())?;
    let close_suspends =
        tokio::runtime::Handle::current().block_on(runtime.interface_close_suspends())?;
    let mut app = interface_app_at(data_dir.join("interface-assets"), false);
    if showoff {
        app.insert_resource(crate::laboratory::showoff::Launch);
    }
    if let Some(options) = laboratory {
        app.insert_resource(crate::laboratory::fiote::Launch(options));
    }
    app.insert_resource(backup_handoff);
    app.insert_resource(crate::sound::Audio::open(
        data_dir.to_path_buf(),
        app.world()
            .get_resource::<crate::wake::WakeSignal>()
            .cloned(),
    ));
    app.insert_resource(CellHandle(runtime)).add_plugins((
        crate::cell_bridge::CellBridgePlugin,
        crate::tray::TrayPlugin,
    ));
    app.insert_resource(crate::workspace::WorkspaceFile::with_settings(
        data_dir.join("interface.json"),
        storage,
    )?);
    app.insert_resource(crate::tray::InterfaceWindowSettings {
        close_suspends,
        tray_enabled,
    });
    app.add_systems(Startup, canvas);
    app.insert_non_send(instance.events());
    app.run();
    Ok(())
}

pub fn connected_app(runtime: cell::CellRuntime) -> App {
    let mut app = interface_app();
    app.insert_resource(CellHandle(runtime)).add_plugins((
        crate::cell_bridge::CellBridgePlugin,
        crate::tray::TrayPlugin,
    ));
    app.add_systems(Startup, canvas);
    app
}

pub fn interface_app() -> App {
    interface_app_at(
        std::env::temp_dir().join(format!("lince-sandbox-assets-{}", std::process::id())),
        false,
    )
}

pub fn offscreen_interface_app() -> App {
    interface_app_at(
        std::env::temp_dir().join(format!("lince-sandbox-assets-{}", std::process::id())),
        true,
    )
}

fn interface_app_at(directory: std::path::PathBuf, offscreen: bool) -> App {
    let plugins = DefaultPlugins
        .build()
        .disable::<LogPlugin>()
        .set(RenderPlugin {
            render_creation: bevy::render::settings::WgpuSettings {
                memory_hints: bevy::render::settings::MemoryHints::MemoryUsage,
                ..default()
            }
            .into(),
            synchronous_pipeline_compilation: true,
            ..default()
        })
        .set(WindowPlugin {
            close_when_requested: false,
            exit_condition: bevy::window::ExitCondition::DontExit,
            primary_window: Some(Window {
                title: "Lince".into(),
                resolution: (800, 640).into(),
                ..default()
            }),
            ..default()
        });
    let plugins = if offscreen {
        plugins
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
    } else {
        plugins
    };
    let mut app = lince_interface::native::app::presentation_app(directory, plugins);
    let wake = if offscreen {
        crate::wake::WakeSignal::new(|| {})
    } else {
        crate::wake::from_proxy(app.world().resource::<bevy::winit::EventLoopProxyWrapper>())
    };
    app.insert_resource(wake)
        .add_systems(Update, file_drag_support);
    app
}

fn file_drag_support(
    mut commands: Commands,
    handles: Query<
        &bevy::window::RawHandleWrapper,
        (
            With<bevy::window::PrimaryWindow>,
            Added<bevy::window::RawHandleWrapper>,
        ),
    >,
) {
    for handle in &handles {
        let unavailable = matches!(
            handle.get_window_handle(),
            raw_window_handle::RawWindowHandle::Wayland(_)
        );
        commands.insert_resource(lince_interface::host::FileDragSupport {
            available: !unavailable,
            reason: unavailable.then(|| "This window host cannot provide file-drag events in Wayland Sandbox. File-hover activation is available in Park; item dragging and edge scrolling remain available here.".into()),
        });
    }
}
