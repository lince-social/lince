use crate::native::{
    canvas::CanvasPlugin,
    effect::EffectPlugin,
    sand::SandPlugin,
    theme::{PAPER, ThemePlugin, idle_settings},
};
use bevy::{
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    input_focus::tab_navigation::TabNavigationPlugin,
    prelude::*,
};

#[derive(Resource, Clone)]
pub struct CellHandle(pub cell::CellRuntime);

pub async fn initialize_language(runtime: &cell::CellRuntime) -> std::io::Result<()> {
    let language = store::config::get(&runtime.store.pool)
        .await
        .map_err(std::io::Error::other)?
        .and_then(|configuration| crate::i18n::Language::from_tag(&configuration.language))
        .unwrap_or_default();
    crate::i18n::initialize(language).map_err(std::io::Error::other)
}

#[derive(Resource, Clone)]
pub struct OutputTarget(pub bevy::camera::RenderTarget);

#[derive(Resource)]
pub struct LinearOutput;

pub fn configure_output_color(world: &mut World, camera: Entity, target: &Handle<Image>) {
    if !world.contains_resource::<LinearOutput>()
        || world
            .resource::<Assets<Image>>()
            .get(target)
            .is_none_or(|image| {
                image.texture_descriptor.format
                    != bevy::render::render_resource::TextureFormat::Rgba16Float
            })
    {
        return;
    }
    if world.get::<bevy::camera::Hdr>(camera).is_none() {
        world.entity_mut(camera).insert(bevy::camera::Hdr);
    }
    if world.get::<bevy::camera::CompositingSpace>(camera)
        != Some(&bevy::camera::CompositingSpace::Linear)
    {
        world
            .entity_mut(camera)
            .insert(bevy::camera::CompositingSpace::Linear);
    }
    if world.get::<Tonemapping>(camera) != Some(&Tonemapping::None) {
        world.entity_mut(camera).insert(Tonemapping::None);
    }
    if world.get::<DebandDither>(camera) != Some(&DebandDither::Disabled) {
        world.entity_mut(camera).insert(DebandDither::Disabled);
    }
}

pub fn text_ready_to_close(world: &mut World) -> bool {
    if world
        .query::<&bevy::text::EditableText>()
        .iter(world)
        .any(|text| text.is_composing() || crate::native::record_view::pending_text(text))
    {
        return false;
    }
    !world
        .query::<(
            &crate::native::record_view::RecordEditor,
            &bevy::text::EditableText,
        )>()
        .iter(world)
        .any(|(record, text)| {
            record.pending.is_some() || text.value().to_string() != record.confirmed
        })
}

pub fn cancel_close(world: &mut World) {
    world.resource_mut::<Messages<AppExit>>().clear();
    crate::native::ide::cancel_exit(world);
}

pub fn presentation_app(directory: std::path::PathBuf, plugins: impl PluginGroup) -> App {
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
    app.insert_resource(crate::native::topology::assets::AssetDirectory(directory));
    app.add_plugins(plugins);
    app.add_plugins(crate::native::topology::splats::SplatPlugin)
        .add_plugins(crate::controls::editing::TextEditingPlugin)
        .add_plugins((
            TabNavigationPlugin,
            ThemePlugin,
            crate::native::castle::CastlePlugin,
            crate::native::icons::IconPlugin,
            SandPlugin,
            crate::native::sand_text::SandTextPlugin,
            EffectPlugin,
            CanvasPlugin,
            crate::native::canvas_controls::CanvasControlsPlugin,
            crate::native::workspace::WorkspacePlugin,
            crate::native::edit_mode::EditModePlugin,
            crate::native::notifications::NotificationsPlugin,
            crate::native::inspection::InspectionPlugin,
            crate::native::area::AreasPlugin,
            crate::native::physics::WorkspacePhysicsPlugin,
        ))
        .insert_resource(ClearColor(PAPER))
        .add_plugins((
            crate::native::information::InformationPlugin,
            crate::native::access_control::AccessControlPlugin,
            crate::native::sync_castle::SyncCastlePlugin,
            crate::native::protein_castle::ProteinCastlePlugin,
            crate::native::protein_area::ProteinAreaPlugin,
            crate::native::record_binding::RecordBindingPlugin,
            crate::native::work_timer::WorkTimerPlugin,
            crate::native::calendar::CalendarPlugin,
            crate::native::kanban::KanbanPlugin,
            crate::native::laboratory::LaboratoryPlugin,
            crate::native::topology::TopologyPlugin,
            crate::native::description::DescriptionPlugin,
            crate::native::instinct::InstinctPlugin,
            crate::native::thread_castle::ThreadCastlePlugin,
        ))
        .add_plugins(crate::native::workspace_sync::WorkspaceSyncPlugin)
        .add_plugins(crate::native::location::LocationPlugin)
        .add_plugins(crate::native::map_castle::MapPlugin)
        .add_plugins(crate::native::visibility_castle::VisibilityCastlePlugin)
        .add_plugins(crate::native::trust_castle::TrustCastlePlugin)
        .add_plugins(crate::native::hand_camera::HandCameraPlugin)
        .add_plugins(crate::native::video_castle::VideoCastlePlugin)
        .add_plugins(crate::native::tutorial::TutorialPlugin)
        .add_plugins(crate::native::karma_castle::KarmaCastlePlugin)
        .add_plugins(crate::native::accessibility::AccessibilityPlugin)
        .add_plugins(crate::native::component_push::ComponentPushPlugin)
        .add_plugins(crate::native::simulation_castle::SimulationCastlePlugin)
        .add_plugins(crate::native::sound::SoundPlugin)
        .add_plugins(crate::native::sound_area::SoundAreaPlugin)
        .add_plugins(crate::native::recorder_castle::RecorderCastlePlugin)
        .add_plugins(crate::native::document_viewer::DocumentViewerPlugin)
        .add_plugins(crate::native::source_view::SourceViewPlugin)
        .add_plugins(crate::native::demeter_castle::DemeterCastlePlugin)
        .add_plugins(crate::native::media_sand::MediaSandPlugin)
        .add_plugins(crate::native::drawing::DrawingPlugin)
        .add_plugins(crate::native::record_extensions::ExtensionPlugin)
        .add_plugins(crate::native::external_drop::ExternalDropPlugin)
        .add_plugins(crate::native::file_explorer::FileExplorerPlugin)
        .add_plugins(crate::native::ide::IdePlugin)
        .add_plugins(crate::native::frequency_castle::FrequencyCastlePlugin)
        .add_plugins(crate::native::transfer_castle::TransferCastlePlugin)
        .add_plugins(crate::native::castle_feed::FeedPlugin)
        .add_plugins(crate::native::assertion_castle::AssertionCastlePlugin)
        .add_plugins(crate::native::shader_castle::ShaderCastlePlugin)
        .add_plugins(crate::native::shader_effects::ShaderEffectsPlugin)
        .add_plugins(crate::native::fiote::session::Plugin)
        .add_plugins(crate::native::fiote::character::CharacterPlugin)
        .add_plugins((
            crate::native::terminal::TerminalPlugin,
            crate::native::command_castle::CommandCastlePlugin,
            crate::native::configuration::ConfigurationPlugin,
            crate::native::owner_backup::BackupPlugin,
            crate::native::organ_castle::OrganCastlePlugin,
            crate::native::todo::TodoPlugin,
            crate::native::ontology::OntologyPlugin,
        ))
        .insert_resource(idle_settings())
        .add_systems(Startup, camera);
    #[cfg(feature = "stuff")]
    app.add_plugins(crate::native::freedoom::FreedoomPlugin);
    app
}

pub fn canvas(mut commands: Commands) {
    commands.spawn((
        crate::native::container::BoxRoot,
        crate::native::instinct::SeedInstinct,
    ));
}

fn camera(
    mut commands: Commands,
    target: Option<Res<OutputTarget>>,
    linear: Option<Res<LinearOutput>>,
) {
    let target = target.map(|target| target.0.clone()).unwrap_or_default();
    let camera = commands
        .spawn((
            Camera3d::default(),
            target.clone(),
            crate::native::topology::presentation::WorldCamera,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::NONE),
                output_mode: bevy::camera::CameraOutputMode::Write {
                    blend_state: Some(
                        bevy::render::render_resource::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                    ),
                    clear_color: ClearColorConfig::None,
                },
                ..default()
            },
            IsDefaultUiCamera,
            Tonemapping::None,
            DebandDither::Disabled,
            Projection::Orthographic(OrthographicProjection::default_3d()),
            Transform::from_xyz(0.0, 10000.0, 0.0).looking_at(Vec3::ZERO, -Vec3::Z),
        ))
        .id();
    commands.insert_resource(crate::native::topology::presentation::SceneCamera(camera));
    let background = commands
        .spawn((
            Camera2d,
            target,
            Camera {
                order: -2,
                ..default()
            },
        ))
        .id();
    commands.insert_resource(crate::native::topology::presentation::BackgroundCamera(
        background,
    ));
    if linear.is_some() {
        commands
            .entity(camera)
            .insert((bevy::camera::Hdr, bevy::camera::CompositingSpace::Linear));
        commands.entity(background).insert((
            bevy::camera::Hdr,
            bevy::camera::CompositingSpace::Linear,
            Tonemapping::None,
            DebandDither::Disabled,
        ));
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 250.0,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::from_layers(
            &std::iter::once(0)
                .chain(crate::native::render_layers::OUTPUTS.indices())
                .chain(crate::native::render_layers::OVERVIEW.indices())
                .collect::<Vec<_>>(),
        ),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_camera_does_not_add_screen_space_dither_to_moving_sands() {
        let mut app = App::new();
        app.add_systems(Startup, camera);
        app.update();
        let camera = app
            .world()
            .resource::<crate::native::topology::presentation::SceneCamera>()
            .0;
        assert_eq!(
            app.world().get::<DebandDither>(camera),
            Some(&DebandDither::Disabled)
        );
    }

    #[test]
    fn native_startup_has_one_empty_canvas_without_automatic_record_view() {
        let mut app = App::new();
        app.add_systems(Startup, canvas);
        app.add_message::<crate::native::cell_bridge::CellMessage>();
        app.update();
        app.world_mut()
            .write_message(crate::native::cell_bridge::CellMessage(
                cell::ServerMessage::Snapshot {
                    id: crate::native::cell_bridge::RECORDS.into(),
                    rows: vec![
                        serde_json::json!({"uid":"record-a", "head":"Apple", "slug":"apple"}),
                    ],
                },
            ));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<crate::native::container::BoxRoot>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&crate::native::area::RecordProperties>()
                .iter(app.world())
                .count(),
            0
        );
    }
}
