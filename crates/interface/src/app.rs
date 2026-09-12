use crate::{
    canvas::CanvasPlugin,
    effect::EffectPlugin,
    sand::SandPlugin,
    theme::{PAPER, ThemePlugin, idle_settings},
};
use bevy::{
    core_pipeline::tonemapping::Tonemapping, input_focus::tab_navigation::TabNavigationPlugin,
    log::LogPlugin, prelude::*, render::RenderPlugin,
};

#[derive(Resource, Clone)]
pub struct CellHandle(pub cell::CellRuntime);

pub fn run_native_interface(
    runtime: cell::CellRuntime,
    instance: &crate::instance::InstanceGuard,
    data_dir: &std::path::Path,
) -> std::io::Result<()> {
    let storage = tokio::runtime::Handle::current().block_on(runtime.interface_storage())?;
    let close_suspends =
        tokio::runtime::Handle::current().block_on(runtime.interface_close_suspends())?;
    let mut app = connected_app(runtime);
    app.insert_resource(crate::workspace::WorkspaceFile::with_settings(
        data_dir.join("interface.json"),
        storage,
    )?);
    app.insert_resource(crate::tray::InterfaceWindowSettings { close_suspends });
    app.insert_non_send(instance.events());
    app.run();
    Ok(())
}

pub fn connected_app(runtime: cell::CellRuntime) -> App {
    let mut app = interface_app();
    app.insert_resource(CellHandle(runtime)).add_plugins((
        crate::cell_bridge::CellBridgePlugin,
        crate::record_view::RecordViewPlugin,
        crate::tray::TrayPlugin,
    ));
    app
}

pub fn interface_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<LogPlugin>()
            .set(RenderPlugin {
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
            }),
    )
    .add_plugins((
        TabNavigationPlugin,
        ThemePlugin,
        crate::castle::CastlePlugin,
        crate::icons::IconPlugin,
        SandPlugin,
        crate::sand_text::SandTextPlugin,
        EffectPlugin,
        CanvasPlugin,
        crate::canvas_controls::CanvasControlsPlugin,
        crate::workspace::WorkspacePlugin,
        crate::edit_mode::EditModePlugin,
        crate::notifications::NotificationsPlugin,
        crate::inspection::InspectionPlugin,
        crate::area::AreasPlugin,
        crate::physics::WorkspacePhysicsPlugin,
    ))
    .insert_resource(ClearColor(PAPER))
    .add_plugins((
        crate::information::InformationPlugin,
        crate::laboratory::LaboratoryPlugin,
    ))
    .insert_resource(idle_settings())
    .add_systems(Startup, camera);
    let wake = crate::wake::WakeSignal::from_proxy(
        app.world().resource::<bevy::winit::EventLoopProxyWrapper>(),
    );
    app.insert_resource(wake);
    app
}

fn camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Projection::Orthographic(OrthographicProjection::default_3d()),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
