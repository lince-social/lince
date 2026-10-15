use super::*;
use bevy::{
    asset::AssetId,
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderPlugin, RenderSystems,
        render_asset::RenderAssets,
        render_resource::AsBindGroup,
        renderer::{
            RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
            WgpuWrapper,
        },
        settings::RenderCreation,
        texture::GpuImage,
    },
    shader::ShaderRef,
};

const CLIENT_SHADER: Handle<Shader> =
    bevy::asset::uuid_handle!("ea275d71-dff9-47fa-a876-af27e5e9be74");

#[derive(AsBindGroup, Asset, TypePath, Clone)]
struct ClientMaterial {
    #[texture(0)]
    #[sampler(1)]
    content: Handle<Image>,
}

impl UiMaterial for ClientMaterial {
    fn fragment_shader() -> ShaderRef {
        CLIENT_SHADER.into()
    }
}

#[derive(Resource, Clone)]
struct ExternalTextures(Vec<(AssetId<Image>, wgpu::Texture)>);

fn prepare_external_images(
    external: Res<ExternalTextures>,
    device: Res<RenderDevice>,
    mut images: ResMut<RenderAssets<GpuImage>>,
) {
    for (id, texture) in &external.0 {
        images.insert(
            *id,
            GpuImage {
                texture: texture.clone().into(),
                texture_view: texture.create_view(&Default::default()).into(),
                sampler: device.create_sampler(&Default::default()),
                texture_descriptor: wgpu::TextureDescriptor {
                    label: Some("park_proof_texture"),
                    size: texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: texture.format(),
                    usage: texture.usage(),
                    view_formats: &[],
                },
                texture_view_descriptor: None,
                had_data: false,
            },
        );
    }
}

pub(super) fn run(report: &mut ProbeReport) -> super::Result<()> {
    let renderer = VulkanRenderer::new()?;
    let (mut backend, mut events) = smithay::backend::winit::init::<GlesRenderer>()
        .map_err(|error| ProbeFailure::at("nested_display", error))?;
    backend.window().set_title("Lince Park graphics proof");
    super::live::with_client(&renderer, report, |texture| {
        let image = render(&renderer, texture)?;
        let _ = events.dispatch_new_events(|_| {});
        let size = backend.window_size();
        let damage = smithay::utils::Rectangle::from_size(size);
        {
            let (gles, mut target) = backend
                .bind()
                .map_err(|error| ProbeFailure::at("nested_target", error))?;
            let texture = gles
                .import_dmabuf(&image.dmabuf, None)
                .map_err(|error| ProbeFailure::at("nested_import", error))?;
            let mut frame = gles
                .render(&mut target, size, smithay::utils::Transform::Flipped180)
                .map_err(|error| ProbeFailure::at("nested_render", error))?;
            frame
                .clear(
                    smithay::backend::renderer::Color32F::new(0.1, 0.1, 0.1, 1.0),
                    &[damage],
                )
                .map_err(|error| ProbeFailure::at("nested_clear", error))?;
            frame
                .render_texture_at(
                    &texture,
                    (0, 0).into(),
                    1,
                    1.0,
                    smithay::utils::Transform::Normal,
                    &[damage],
                    &[],
                    1.0,
                )
                .map_err(|error| ProbeFailure::at("nested_draw", error))?;
            frame
                .finish()
                .map_err(|error| ProbeFailure::at("nested_finish", error))?
                .wait()
                .map_err(|error| ProbeFailure::at("nested_sync", error))?;
        }
        backend
            .submit(Some(&[damage]))
            .map_err(|error| ProbeFailure::at("nested_present", error))?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(())
    })?;
    report.completed.push(
        "shared native Bevy controls and live application pixels composed and presented in a nested Wayland window"
            .into(),
    );
    Ok(())
}

fn render(
    renderer: &VulkanRenderer,
    client_texture: &wgpu::Texture,
) -> super::Result<ExportedImage> {
    let mut app = App::new();
    let resources = RenderCreation::manual(
        RenderDevice::from(renderer.device.clone()),
        RenderQueue(Arc::new(WgpuWrapper::new(renderer.queue.clone()))),
        RenderAdapterInfo(WgpuWrapper::new(renderer.adapter.get_info())),
        RenderAdapter(Arc::new(WgpuWrapper::new(renderer.adapter.clone()))),
        RenderInstance(Arc::new(WgpuWrapper::new(renderer._instance.clone()))),
    );
    let plugins = DefaultPlugins
        .build()
        .disable::<bevy::log::LogPlugin>()
        .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        })
        .set(RenderPlugin {
            render_creation: resources,
            synchronous_pipeline_compilation: true,
            ..default()
        });
    #[cfg(feature = "native-presentation")]
    let plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    app.add_plugins(plugins);
    app.add_plugins((
        lince_interface::theme::TypographyPlugin,
        UiMaterialPlugin::<ClientMaterial>::default(),
    ));
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            CLIENT_SHADER.id(),
            Shader::from_wgsl(include_str!("client.wgsl"), "park/client.wgsl"),
        )
        .map_err(|error| ProbeFailure::at("client_material", error))?;
    app.finish();
    app.cleanup();
    let mut target = Image::new_target_texture(640, 360, wgpu::TextureFormat::Bgra8UnormSrgb, None);
    target.texture_descriptor.usage |= wgpu::TextureUsages::COPY_SRC;
    let output = app.world_mut().resource_mut::<Assets<Image>>().add(target);
    let content = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            client_texture.width(),
            client_texture.height(),
            wgpu::TextureFormat::Bgra8Unorm,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((
            Camera2d,
            RenderTarget::Image(output.clone().into()),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::linear_rgba(0.05, 0.05, 0.05, 1.0)),
                ..default()
            },
        ))
        .id();
    let typography = app.world().resource::<lince_interface::theme::Typography>();
    let text_font = typography.text(22.0);
    app.world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                top: px(16),
                width: px(280),
                height: px(300),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                row_gap: px(12),
                ..default()
            },
            UiTargetCamera(camera),
            BackgroundColor(Color::linear_rgba(0.4, 0.0, 0.4, 1.0)),
        ))
        .with_children(|parent| {
            parent.spawn((Text::new("Park native Sand"), text_font.clone()));
            parent.spawn((
                lince_interface::controls::button(0),
                Node {
                    width: px(200),
                    height: px(80),
                    ..default()
                },
                BackgroundColor(Color::linear_rgba(0.0, 0.5, 0.0, 1.0)),
            ));
        });
    let material = app
        .world_mut()
        .resource_mut::<Assets<ClientMaterial>>()
        .add(ClientMaterial {
            content: content.clone(),
        });
    app.world_mut().spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(320),
            top: px(16),
            width: px(256),
            height: px(256),
            ..default()
        },
        UiTargetCamera(camera),
        MaterialNode(material),
    ));
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        prepare_external_images
            .in_set(RenderSystems::PrepareAssets)
            .after(bevy::render::render_asset::prepare_assets::<GpuImage>)
            .before(
                bevy::render::render_asset::prepare_assets::<
                    bevy::ui_render::PreparedUiMaterial<ClientMaterial>,
                >,
            ),
    );
    for _ in 0..4 {
        app.sub_app_mut(RenderApp)
            .insert_resource(ExternalTextures(vec![(
                content.id(),
                client_texture.clone(),
            )]));
        app.update();
    }
    let output_texture = app
        .sub_app(RenderApp)
        .world()
        .resource::<RenderAssets<GpuImage>>()
        .get(output.id())
        .ok_or_else(|| ProbeFailure::at("bevy_output", "Bevy output texture is missing"))?
        .texture
        .clone();
    renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|error| ProbeFailure::at("bevy_render", error))?;
    let exported = renderer.export_copy(&output_texture)?;
    let egl_device = EGLDevice::enumerate()
        .map_err(|error| ProbeFailure::at("bevy_egl", error))?
        .find(|device| !device.is_software())
        .ok_or_else(|| ProbeFailure::at("bevy_egl", "no hardware EGL device"))?;
    let display = unsafe { EGLDisplay::new(egl_device) }
        .map_err(|error| ProbeFailure::at("bevy_egl", error))?;
    let context = EGLContext::new(&display).map_err(|error| ProbeFailure::at("bevy_egl", error))?;
    let mut gles = unsafe { GlesRenderer::new(context) }
        .map_err(|error| ProbeFailure::at("bevy_egl", error))?;
    let texture = gles
        .import_dmabuf(&exported.dmabuf, None)
        .map_err(|error| ProbeFailure::at("bevy_egl_import", error))?;
    let mapping = gles
        .copy_texture(
            &texture,
            smithay::utils::Rectangle::from_size((640, 360).into()),
            smithay::backend::allocator::Fourcc::Abgr8888,
        )
        .map_err(|error| ProbeFailure::at("bevy_pixels", error))?;
    let pixels = gles
        .map_texture(&mapping)
        .map_err(|error| ProbeFailure::at("bevy_pixels", error))?;
    let native = &pixels[(260 * 640 + 40) * 4..][..4];
    let client_pixel = &pixels[(80 * 640 + 400) * 4..][..4];
    let expected = if client_texture.width() == 192 {
        [44_i16, 64, 191, 255]
    } else {
        [0, 64, 255, 255]
    };
    if native[0] < 90
        || native[2] < 90
        || client_pixel
            .iter()
            .zip(expected)
            .any(|(actual, expected)| (i16::from(*actual) - expected).abs() > 4)
    {
        return Err(ProbeFailure::at(
            "bevy_pixels",
            format!("unexpected native {native:?} or imported {client_pixel:?} pixels"),
        ));
    }
    Ok(exported)
}
