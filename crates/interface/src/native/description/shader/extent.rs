use super::*;
use crate::native::topology::surface_budget::{Entry, Grant, Request};

#[cfg(test)]
mod pressure;

#[derive(Clone, Copy)]
struct Consumer {
    size: Vec2,
    captured: Option<Entity>,
    camera: Option<Entity>,
    scale: f32,
    seen: u64,
}

#[derive(Resource, Default)]
struct Consumers {
    epoch: u64,
    entries: HashMap<(Entity, Entity), Consumer>,
}

#[derive(Component, PartialEq)]
pub(crate) struct SurfaceConsumer {
    pub image: Handle<Image>,
    pub surface: Entity,
}

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(super) struct Raster {
    pub pixels: UVec2,
    pub requested: UVec2,
    pub visible: bool,
    pub device_limited: bool,
    pub budget_limited: bool,
    pub priority: u8,
}

pub(crate) fn request_extent(world: &mut World, source: Entity, consumer: Entity, pixels: UVec2) {
    world.init_resource::<Consumers>();
    if pixels.min_element() == 0
        || pixels.max_element() > 1_048_576
        || world.get::<EffectPreview>(source).is_none()
        || world.get_entity(consumer).is_err()
    {
        return;
    }
    let mut consumers = world.resource_mut::<Consumers>();
    if consumers.entries.len() >= 4096 && !consumers.entries.contains_key(&(source, consumer)) {
        return;
    }
    let seen = consumers.epoch;
    consumers.entries.insert(
        (source, consumer),
        Consumer {
            size: pixels.as_vec2(),
            captured: None,
            camera: None,
            scale: 1.0,
            seen,
        },
    );
}

pub(crate) fn release_extent(world: &mut World, source: Entity, consumer: Entity) {
    if let Some(mut consumers) = world.get_resource_mut::<Consumers>() {
        consumers.entries.remove(&(source, consumer));
    }
}

fn material_consumers(world: &mut World, native: &HashMap<Entity, (Request, u8)>) {
    let images: HashMap<_, _> = world
        .get_resource::<EffectAssets>()
        .map(|assets| {
            assets
                .owned
                .iter()
                .map(|(owner, (_, _, image, _))| (image.id(), *owner))
                .collect()
        })
        .unwrap_or_default();
    let consumers: Vec<_> = world
        .query::<(Entity, &SurfaceConsumer, Option<&Visibility>)>()
        .iter(world)
        .filter_map(|(entity, consumer, visibility)| {
            images.get(&consumer.image.id()).map(|source| {
                (
                    entity,
                    *source,
                    consumer.surface,
                    visibility != Some(&Visibility::Hidden),
                )
            })
        })
        .collect();
    for (entity, source, surface, visible) in consumers {
        let Some((request, _)) = native
            .get(&surface)
            .filter(|(request, _)| visible && request.visible && request.size.is_finite())
        else {
            release_extent(world, source, entity);
            continue;
        };
        request_extent(
            world,
            source,
            entity,
            request
                .size
                .ceil()
                .clamp(Vec2::ONE, Vec2::splat(1_048_576.0))
                .as_uvec2(),
        );
        if let Some(consumer) = world
            .resource_mut::<Consumers>()
            .entries
            .get_mut(&(source, entity))
        {
            consumer.size = request.size;
            consumer.captured = Some(surface);
        }
    }
}

pub(crate) fn measure(world: &mut World) {
    world.init_resource::<Consumers>();
    let images: HashMap<_, _> = world
        .resource::<EffectAssets>()
        .owned
        .iter()
        .map(|(owner, (_, _, image, _))| (image.id(), *owner))
        .collect();
    let nodes: Vec<_> = world
        .query::<(
            Entity,
            &ImageNode,
            &ComputedNode,
            Option<&UiGlobalTransform>,
            Option<&bevy::ui::ComputedUiTargetCamera>,
        )>()
        .iter(world)
        .filter_map(|(entity, image, node, transform, camera)| {
            images.get(&image.image.id()).map(|source| {
                (
                    entity,
                    *source,
                    node.size(),
                    transform.copied(),
                    camera.and_then(|camera| camera.get()),
                )
            })
        })
        .collect();
    for (entity, source, size, transform, camera) in nodes {
        if !visible(world, entity) || !size.is_finite() || size.min_element() <= 0.0 {
            release_extent(world, source, entity);
            continue;
        }
        let stretch = transform.map_or(Vec2::ONE, |transform| {
            let affine = transform.affine();
            Vec2::new(
                affine.matrix2.x_axis.length(),
                affine.matrix2.y_axis.length(),
            )
        });
        let size = size * stretch;
        if !size.is_finite() || size.min_element() <= 0.0 {
            release_extent(world, source, entity);
            continue;
        }
        let mut captured = None;
        let mut parent = Some(entity);
        for _ in 0..128 {
            let Some(current) = parent else { break };
            if world
                .get::<crate::native::topology::presentation::Surface>(current)
                .is_some()
            {
                captured = Some(current);
                break;
            }
            parent = world.get::<ChildOf>(current).map(ChildOf::parent);
        }
        let scale = camera
            .and_then(|camera| world.get::<Camera>(camera))
            .and_then(Camera::target_scaling_factor)
            .unwrap_or(1.0)
            .max(f32::MIN_POSITIVE);
        request_extent(
            world,
            source,
            entity,
            size.ceil()
                .clamp(Vec2::ONE, Vec2::splat(1_048_576.0))
                .as_uvec2(),
        );
        if let Some(consumer) = world
            .resource_mut::<Consumers>()
            .entries
            .get_mut(&(source, entity))
        {
            consumer.size = size / scale;
            consumer.captured = captured;
            consumer.camera = camera;
            consumer.scale = scale;
        }
    }
}

pub(crate) fn requests(
    world: &mut World,
    native: &HashMap<Entity, (Request, u8)>,
) -> Vec<(Entity, Entry)> {
    world.init_resource::<Consumers>();
    material_consumers(world, native);
    let epoch = world.resource::<Consumers>().epoch.saturating_add(1);
    let entries: Vec<_> = world
        .resource::<Consumers>()
        .entries
        .iter()
        .filter(|((source, consumer), entry)| {
            entry.seen.saturating_add(1) >= epoch
                && world.get::<EffectPreview>(*source).is_some()
                && world.get_entity(*consumer).is_ok()
        })
        .map(|(key, entry)| (*key, *entry))
        .collect();
    {
        let mut consumers = world.resource_mut::<Consumers>();
        consumers.epoch = epoch;
        consumers.entries = entries.iter().copied().collect();
    }
    let window_scale = world
        .query::<&Window>()
        .iter(world)
        .next()
        .map_or(1.0, Window::scale_factor);
    let mut owners: Vec<_> = world
        .get_resource::<EffectAssets>()
        .map(|assets| assets.owned.keys().copied().collect())
        .unwrap_or_default();
    owners.sort_unstable_by_key(|entity| entity.to_bits());
    owners
        .into_iter()
        .filter_map(|owner| {
            let preview = world.get::<EffectPreview>(owner)?;
            let mut size = Vec2::ONE;
            let base = entries
                .iter()
                .find(|((source, consumer), _)| {
                    *source == owner && *consumer == world.get::<Preview>(owner).unwrap().surface
                })
                .map(|(_, consumer)| consumer.size);
            let mut priority = if preview.priority { 4 } else { 1 };
            let mut showing = false;
            for ((source, _), consumer) in &entries {
                if *source != owner {
                    continue;
                }
                let (scale, present, rank) = if let Some(captured) = consumer.captured {
                    let Some((request, priority)) = native.get(&captured) else {
                        continue;
                    };
                    (request.density, request.visible, *priority)
                } else {
                    let scale = consumer
                        .camera
                        .and_then(|camera| world.get::<RenderTarget>(camera))
                        .map_or(consumer.scale, |target| match target {
                            RenderTarget::Image(image) => image.scale_factor,
                            RenderTarget::Window(_) => window_scale,
                            _ => consumer.scale,
                        });
                    (scale, true, 2)
                };
                if present {
                    let desired = consumer.size * scale;
                    let desired =
                        base.map_or(desired, |base| base * (desired / base).max_element());
                    size = size.max(desired);
                    showing = true;
                    priority = priority.max(rank);
                }
            }
            showing &= !preview.disabled && !crate::native::shader_effects::disabled(world);
            let modes = preview
                .working
                .iter()
                .map(|effect| &effect.mode)
                .chain(std::iter::once(&preview.effect.mode));
            let bytes_per_pixel = modes
                .map(|mode| match mode {
                    nucleus::shader_effect::spatial::Mode::Surface => 12,
                    nucleus::shader_effect::spatial::Mode::Mesh(_) => 16,
                    nucleus::shader_effect::spatial::Mode::Volume(_) => 20,
                })
                .max()
                .unwrap();
            Some((
                owner,
                Entry {
                    request: Request {
                        size,
                        density: 1.0,
                        previous: 0.0,
                        visible: showing,
                        text: false,
                        exact: true,
                    },
                    priority,
                    bytes_per_pixel,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::gpu_readback::{Readback, ReadbackComplete};
    use std::collections::VecDeque;

    #[derive(Resource, Default)]
    struct Samples(VecDeque<(usize, [u8; 4], bool)>);

    fn sampled(
        event: On<ReadbackComplete>,
        readbacks: Query<&Readback>,
        images: Res<Assets<Image>>,
        mut samples: ResMut<Samples>,
    ) {
        let Ok(Readback::Texture(texture)) = readbacks.get(event.entity) else {
            return;
        };
        let Some(image) = images.get(texture) else {
            return;
        };
        let size = image.size();
        let stride = (size.x as usize * 4).div_ceil(256) * 256;
        let sharp = if size.min_element() >= 128 && event.data.len() == stride * size.y as usize {
            let first = event.data[64 * stride + 64 * 4];
            (64..66).all(|y| {
                (64..128).all(|x| {
                    let pixel = &event.data[y * stride + x * 4..][..4];
                    let white = (first > 127) ^ ((x + y) % 2 != 0);
                    pixel[..3]
                        .iter()
                        .all(|value| if white { *value > 250 } else { *value < 5 })
                        && pixel[3] == 255
                })
            })
        } else {
            false
        };
        if event.data.len() >= 4 {
            samples
                .0
                .push_back((event.data.len(), event.data[..4].try_into().unwrap(), sharp));
            if samples.0.len() > 16 {
                samples.0.pop_front();
            }
        }
    }

    fn wait(app: &mut App, source: Entity, image: &Handle<Image>, size: UVec2, bright: bool) {
        app.world_mut().resource_mut::<Samples>().0.clear();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.update();
            let state = app.world().get::<EffectPreview>(source).unwrap();
            let camera = app.world().get::<Camera>(state.camera).unwrap();
            if camera.is_active {
                assert_eq!(
                    camera.physical_target_size(),
                    app.world()
                        .resource::<Assets<Image>>()
                        .get(image)
                        .map(Image::size),
                    "The source camera must use this frame's allocated raster"
                );
            }
            let expected_bytes = (size.x as usize * 4).div_ceil(256) * 256 * size.y as usize;
            let arrived =
                app.world()
                    .resource::<Samples>()
                    .0
                    .iter()
                    .any(|(bytes, pixel, sharp)| {
                        *bytes == expected_bytes
                            && *sharp
                            && pixel[1] > 100
                            && pixel[2] > 150
                            && if bright {
                                pixel[0] > 200
                            } else {
                                (100..180).contains(&pixel[0])
                            }
                    });
            if state.ready
                && state.painted >= 3
                && arrived
                && app
                    .world()
                    .resource::<Assets<Image>>()
                    .get(image)
                    .unwrap()
                    .size()
                    == size
                && app
                    .world()
                    .get::<Camera>(state.camera)
                    .unwrap()
                    .physical_target_size()
                    == Some(size)
            {
                assert_eq!(
                    app.world()
                        .get::<ComputedNode>(output(app.world(), source))
                        .unwrap()
                        .size(),
                    size.as_vec2()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Source extent {size:?}: {:?}; {:?}; {:?}",
                app.world().get::<Raster>(source),
                app.world()
                    .get::<ComputedNode>(output(app.world(), source))
                    .map(ComputedNode::size),
                app.world().resource::<Samples>().0
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_usage(app: &mut App, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.update();
            let usage = app
                .world()
                .resource::<crate::native::topology::surface_budget::residency::Usage>()
                .snapshot();
            assert!(!usage.lower_bound);
            if usage.current_bytes == expected && usage.retained_bytes() == 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Producer texture cleanup: {usage:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn gpu_sources_follow_physical_size_scale_and_frozen_repaints_without_changing_their_identity()
    {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::log::LogPlugin>(),
        );
        app.init_resource::<crate::native::theme::Typography>();
        app.init_resource::<Samples>();
        install(&mut app);
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.plugins_state() != bevy::app::PluginsState::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        app.finish();
        app.cleanup();
        let scene = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                1024,
                768,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
        let camera = app
            .world_mut()
            .spawn((
                Camera2d,
                Camera {
                    order: 0,
                    ..default()
                },
                RenderTarget::Image(scene.clone().into()),
                Msaa::Off,
            ))
            .id();
        let parent = app
            .world_mut()
            .spawn((
                Node {
                    width: px(640),
                    height: px(360),
                    ..default()
                },
                UiTargetCamera(camera),
            ))
            .id();
        let definition = Effect { programs: [("color".into(),
            "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let pixel = vec2<u32>(floor(uv * inputs.size)); if pixel.x >= 64u && pixel.y >= 64u { let value = f32((pixel.x + pixel.y) % 2u); return vec4<f32>(value, value, value, 1.0); } return vec4<f32>(inputs.quantity, 0.25, 0.5, 1.0); }".into())].into(),
            rules: Vec::new(), fallback: "color".into(), ..default() };
        let source = effect(app.world_mut(), parent, definition, Some("0.25".into()));
        app.world_mut().get_mut::<Node>(source).unwrap().height = percent(100);
        let surface = output(app.world(), source);
        app.world_mut().get_mut::<Node>(surface).unwrap().height = percent(100);
        app.world_mut()
            .get_mut::<EffectPreview>(source)
            .unwrap()
            .motion = false;
        let image = app.world().resource::<EffectAssets>().owned[&source]
            .2
            .clone();
        let readback = app
            .world_mut()
            .spawn(Readback::texture(image.clone()))
            .observe(sampled)
            .id();
        wait(&mut app, source, &image, UVec2::new(640, 360), false);
        let usage = app
            .world()
            .resource::<crate::native::topology::surface_budget::residency::Usage>()
            .snapshot();
        assert!(usage.current_bytes >= 640 * 360 * 4, "{usage:?}");
        assert!(!usage.lower_bound);
        let shader = app
            .world()
            .get::<Preview>(source)
            .unwrap()
            .material
            .clone()
            .unwrap();
        let program = app
            .world()
            .resource::<Assets<ShaderMaterial>>()
            .get(&shader)
            .unwrap()
            .shader
            .clone();
        app.world_mut()
            .entity_mut(camera)
            .insert(RenderTarget::Image(bevy::camera::ImageRenderTarget {
                handle: scene.clone(),
                scale_factor: 1.25,
            }));
        wait(&mut app, source, &image, UVec2::new(800, 450), false);
        {
            let mut state = app.world_mut().get_mut::<EffectPreview>(source).unwrap();
            state.frozen = true;
            state.quantity = Some("0.75".into());
        }
        {
            let mut node = app.world_mut().get_mut::<Node>(parent).unwrap();
            node.width = px(384);
            node.height = px(216);
        }
        app.world_mut()
            .entity_mut(camera)
            .insert(RenderTarget::Image(bevy::camera::ImageRenderTarget {
                handle: scene.clone(),
                scale_factor: 1.5,
            }));
        wait(&mut app, source, &image, UVec2::new(576, 324), false);
        app.update();
        let state = app.world().get::<EffectPreview>(source).unwrap();
        assert!(state.snapshot.frozen());
        assert!(!app.world().get::<Camera>(state.camera).unwrap().is_active);
        assert_eq!(
            app.world()
                .resource::<Assets<ShaderMaterial>>()
                .get(&shader)
                .unwrap()
                .shader,
            program
        );
        assert_eq!(
            app.world().resource::<EffectAssets>().owned[&source].2,
            image
        );
        app.world_mut()
            .get_mut::<EffectPreview>(source)
            .unwrap()
            .frozen = false;
        wait(&mut app, source, &image, UVec2::new(576, 324), true);
        app.world_mut()
            .entity_mut(camera)
            .insert(RenderTarget::Image(bevy::camera::ImageRenderTarget {
                handle: scene,
                scale_factor: 2.0,
            }));
        wait(&mut app, source, &image, UVec2::new(768, 432), true);
        app.world_mut().get_mut::<Node>(parent).unwrap().display = Display::None;
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&image)
                .unwrap()
                .size(),
            UVec2::ONE
        );
        assert!(
            !app.world()
                .get::<Camera>(app.world().get::<EffectPreview>(source).unwrap().camera)
                .unwrap()
                .is_active
        );
        wait_usage(&mut app, 4);
        app.world_mut().despawn(readback);
        let camera = app.world().get::<EffectPreview>(source).unwrap().camera;
        app.world_mut().despawn(source);
        app.update();
        assert!(app.world().get_entity(camera).is_err());
        assert!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&image)
                .is_none()
        );
        drop(image);
        wait_usage(&mut app, 0);
    }

    fn setup() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Assets<Shader>>();
        world.init_resource::<Assets<ShaderMaterial>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<crate::native::theme::Typography>();
        let parent = world.spawn(Node::default()).id();
        let source = effect(&mut world, parent, Effect::default(), Some("1".into()));
        (world, source)
    }

    #[test]
    fn windowless_consumers_resize_the_same_source_and_release_hidden_and_destroyed_targets() {
        let (mut world, source) = setup();
        let consumer = world.spawn_empty().id();
        let original = world.resource::<EffectAssets>().owned[&source].2.clone();
        for size in [UVec2::new(800, 450), UVec2::new(960, 720)] {
            request_extent(&mut world, source, consumer, size);
            crate::native::topology::presentation::synchronize(&mut world);
            assert_eq!(
                world
                    .resource::<Assets<Image>>()
                    .get(&original)
                    .unwrap()
                    .size(),
                size
            );
            assert_eq!(world.resource::<EffectAssets>().owned[&source].2, original);
            assert!(
                world
                    .resource::<Assets<Image>>()
                    .get(&original)
                    .unwrap()
                    .asset_usage
                    .contains(RenderAssetUsages::MAIN_WORLD)
            );
            assert_eq!(world.resource::<Assets<Shader>>().len(), 0);
        }
        world.despawn(consumer);
        crate::native::topology::presentation::synchronize(&mut world);
        assert_eq!(
            world
                .resource::<Assets<Image>>()
                .get(&original)
                .unwrap()
                .size(),
            UVec2::ONE
        );
        assert!(!world.get::<Raster>(source).unwrap().visible);
        world.despawn(source);
        crate::native::topology::presentation::synchronize(&mut world);
        assert!(world.resource::<EffectAssets>().owned.is_empty());
        assert!(world.resource::<Assets<Image>>().is_empty());
    }

    #[test]
    fn material_consumers_share_unconstrained_surface_demand_and_release_hidden_views() {
        let (mut world, source) = setup();
        let image = world.resource::<EffectAssets>().owned[&source].2.clone();
        world.get_mut::<EffectPreview>(source).unwrap().priority = true;
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let material = world
            .spawn((
                SurfaceConsumer {
                    image: image.clone(),
                    surface: first,
                },
                Visibility::Visible,
            ))
            .id();
        let other = world
            .spawn((
                SurfaceConsumer {
                    image,
                    surface: second,
                },
                Visibility::Visible,
            ))
            .id();
        let mut native = HashMap::from([
            (
                first,
                (
                    Request {
                        size: Vec2::new(300.0, 150.0),
                        density: 2.0,
                        previous: 0.125,
                        visible: true,
                        text: false,
                        exact: true,
                    },
                    5,
                ),
            ),
            (
                second,
                (
                    Request {
                        size: Vec2::new(800.0, 400.0),
                        density: 1.5,
                        previous: 0.125,
                        visible: true,
                        text: false,
                        exact: true,
                    },
                    1,
                ),
            ),
        ]);
        for _ in 0..3 {
            let effects = requests(&mut world, &native);
            assert_eq!(effects.len(), 1);
            assert_eq!(effects[0].1.request.size, Vec2::new(1200.0, 600.0));
            assert_eq!(effects[0].1.priority, 5);
        }
        world.entity_mut(other).insert(Visibility::Hidden);
        assert_eq!(
            requests(&mut world, &native)[0].1.request.size,
            Vec2::new(600.0, 300.0)
        );
        native.get_mut(&first).unwrap().0.density = 3.0;
        assert_eq!(
            requests(&mut world, &native)[0].1.request.size,
            Vec2::new(900.0, 450.0)
        );
        native.get_mut(&first).unwrap().0.visible = false;
        assert!(!requests(&mut world, &native)[0].1.request.visible);
        world.despawn(material);
        world.despawn(other);
        requests(&mut world, &native);
        assert!(world.resource::<Consumers>().entries.is_empty());
    }

    #[test]
    fn smaller_consumers_cannot_reduce_the_source_or_change_its_primary_aspect() {
        let (mut world, source) = setup();
        let output = super::super::output(&world, source);
        let other = world.spawn_empty().id();
        request_extent(&mut world, source, output, UVec2::new(400, 200));
        request_extent(&mut world, source, other, UVec2::new(300, 500));
        crate::native::topology::presentation::synchronize(&mut world);
        assert_eq!(
            world.get::<Raster>(source).unwrap().pixels,
            UVec2::new(1000, 500)
        );
        request_extent(&mut world, source, output, UVec2::new(400, 200));
        request_extent(&mut world, source, other, UVec2::new(100, 50));
        crate::native::topology::presentation::synchronize(&mut world);
        assert_eq!(
            world.get::<Raster>(source).unwrap().pixels,
            UVec2::new(400, 200)
        );
        crate::native::topology::presentation::synchronize(&mut world);
        assert!(!world.get::<Raster>(source).unwrap().visible);
    }
}

pub(crate) fn apply(world: &mut World, effects: &[(Entity, Entry)], grants: &[Grant]) {
    for ((owner, entry), grant) in effects.iter().zip(grants) {
        let Some((camera, surface, image, _)) = world
            .get_resource::<EffectAssets>()
            .and_then(|assets| assets.owned.get(owner))
            .cloned()
        else {
            continue;
        };
        let pixels = grant.pixels.max(UVec2::ONE);
        let changed = world
            .resource::<Assets<Image>>()
            .get(&image)
            .is_some_and(|image| image.size() != pixels);
        if changed {
            world
                .resource_mut::<Assets<Image>>()
                .get_mut(&image)
                .unwrap()
                .resize(Extent3d {
                    width: pixels.x,
                    height: pixels.y,
                    depth_or_array_layers: 1,
                });
            let mut node = world.get_mut::<Node>(surface).unwrap();
            node.width = px(pixels.x as f32);
            node.height = px(pixels.y as f32);
            world.get_mut::<EffectPreview>(*owner).unwrap().painted = 0;
        }
        if world
            .get::<Camera>(camera)
            .is_some_and(|camera| camera.viewport.is_some())
        {
            world.get_mut::<Camera>(camera).unwrap().viewport = None;
        }
        let raster = Raster {
            pixels,
            requested: grant.requested,
            visible: entry.request.visible && grant.pixels != UVec2::ZERO,
            device_limited: grant.device_limited,
            budget_limited: grant.budget_limited,
            priority: entry.priority,
        };
        if world.get::<Raster>(*owner) != Some(&raster) {
            world.entity_mut(*owner).insert(raster);
        }
    }
}

fn summary(world: &World, owner: Entity, mut text: String) -> String {
    let Some(raster) = world
        .get::<Raster>(owner)
        .filter(|raster| raster.device_limited || raster.budget_limited)
    else {
        return text;
    };
    if !text.is_empty() {
        text.push_str(" · ");
    }
    text.push_str(&format!(
        "Rendering {}×{} of {}×{}",
        raster.pixels.x, raster.pixels.y, raster.requested.x, raster.requested.y
    ));
    if let Some(budget) =
        world.get_resource::<crate::native::topology::surface_budget::Diagnostics>()
    {
        if raster.device_limited {
            text.push_str(&format!(" (device extent limit {})", budget.maximum_extent));
        }
        if raster.budget_limited {
            text.push_str(&format!(
                " (target budget: {} MiB, {} reduced)",
                budget.granted_bytes / (1024 * 1024),
                budget.limited_targets
            ));
        }
    }
    text
}

pub(super) fn label(world: &World, owner: Entity, text: String) -> String {
    let mut text = summary(world, owner, text);
    if let Some(capture) = crate::native::topology::surface_budget::reduction(world, owner) {
        if !text.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(&capture);
    }
    if let Some(source) = crate::native::shader_effects::inspected_source(world, owner)
        .filter(|source| *source != owner)
    {
        let mut live = summary(world, source, String::new());
        if let Some(capture) = crate::native::topology::surface_budget::reduction(world, source) {
            if !live.is_empty() {
                live.push_str(" · ");
            }
            live.push_str(&capture);
        }
        if !live.is_empty() {
            if !text.is_empty() {
                text.push_str(" · ");
            }
            text.push_str("Live Sand: ");
            text.push_str(&live);
        }
    }
    text
}
