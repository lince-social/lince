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
    tray_enabled: bool,
) -> std::io::Result<()> {
    let storage = tokio::runtime::Handle::current().block_on(runtime.interface_storage())?;
    let close_suspends =
        tokio::runtime::Handle::current().block_on(runtime.interface_close_suspends())?;
    let mut app = interface_app_at(data_dir.join("interface-assets"));
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
        std::env::temp_dir().join(format!("lince-interface-assets-{}", std::process::id())),
    )
}

fn interface_app_at(directory: std::path::PathBuf) -> App {
    use bevy::asset::{
        AssetApp,
        io::{AssetSource, AssetSourceBuilder},
    };
    let mut app = App::new();
    let reader_path = directory.to_string_lossy().into_owned();
    app.register_asset_source(
        "topology",
        AssetSourceBuilder::new(move || AssetSource::get_default_reader(reader_path.clone())()),
    );
    app.insert_resource(crate::topology::assets::AssetDirectory(directory));
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
        crate::access_control::AccessControlPlugin,
        crate::sync_castle::SyncCastlePlugin,
        crate::protein_castle::ProteinCastlePlugin,
        crate::protein_area::ProteinAreaPlugin,
        crate::record_binding::RecordBindingPlugin,
        crate::work_timer::WorkTimerPlugin,
        crate::calendar::CalendarPlugin,
        crate::kanban::KanbanPlugin,
        crate::laboratory::LaboratoryPlugin,
        crate::topology::TopologyPlugin,
        crate::description::DescriptionPlugin,
        crate::instinct::InstinctPlugin,
        crate::thread_castle::ThreadCastlePlugin,
        crate::tutorial::TutorialPlugin,
    ))
    .insert_resource(idle_settings())
    .add_systems(Startup, camera);
    let wake = crate::wake::WakeSignal::from_proxy(
        app.world().resource::<bevy::winit::EventLoopProxyWrapper>(),
    );
    app.insert_resource(wake);
    app
}

fn canvas(mut commands: Commands) {
    commands.spawn((crate::container::BoxRoot, crate::instinct::SeedInstinct));
}

fn camera(mut commands: Commands) {
    let camera = commands
        .spawn((
            Camera3d::default(),
            crate::topology::presentation::WorldCamera,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::NONE),
                output_mode: bevy::camera::CameraOutputMode::Write {
                    blend_state: Some(bevy::render::render_resource::BlendState::ALPHA_BLENDING),
                    clear_color: ClearColorConfig::None,
                },
                ..default()
            },
            IsDefaultUiCamera,
            Tonemapping::None,
            Projection::Orthographic(OrthographicProjection::default_3d()),
            Transform::from_xyz(0.0, 10000.0, 0.0).looking_at(Vec3::ZERO, -Vec3::Z),
        ))
        .id();
    commands.insert_resource(crate::topology::presentation::SceneCamera(camera));
    let background = commands
        .spawn((
            Camera2d,
            Camera {
                order: -2,
                ..default()
            },
        ))
        .id();
    commands.insert_resource(crate::topology::presentation::BackgroundCamera(background));
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_startup_has_one_empty_canvas_without_automatic_record_view() {
        let mut app = App::new();
        app.add_systems(Startup, canvas);
        app.add_message::<crate::cell_bridge::CellMessage>();
        app.update();
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(cell::ServerMessage::Snapshot {
                id: crate::cell_bridge::RECORDS.into(),
                rows: vec![serde_json::json!({"uid":"record-a", "head":"Apple", "slug":"apple"})],
            }));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<crate::container::BoxRoot>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&crate::area::RecordProperties>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
