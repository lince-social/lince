use super::{data::*, run::Run};
use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
};
use std::collections::HashMap;

#[derive(Component)]
pub(super) struct Scene {
    pub owner: Entity,
    pub layer: usize,
}

pub(super) struct View {
    pub scene: Entity,
    pub camera: Entity,
    pub image: Handle<Image>,
    player: Entity,
    map: Entity,
    objects: Entity,
    parts: HashMap<(u64, IVec2), Entity>,
    level: u64,
    geometry_revision: u64,
    layer: usize,
}

impl View {
    pub fn new(world: &mut World, owner: Entity, run: &Run) -> Result<Self, &'static str> {
        let used: Vec<_> = world
            .query::<&Scene>()
            .iter(world)
            .map(|scene| scene.layer)
            .collect();
        let layer = crate::native::render_layers::KAMALIE
            .indices()
            .find(|layer| !used.contains(layer))
            .ok_or("The native Kamalie scene limit has been reached")?;
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<ColorMaterial>>();
        let scene = world
            .spawn((
                Scene { owner, layer },
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let mut image = Image::new_target_texture(
            (WIDTH as f32 * TILE) as u32,
            (HEIGHT as f32 * TILE) as u32,
            TextureFormat::Rgba8UnormSrgb,
            None,
        );
        image.sampler = bevy::image::ImageSampler::nearest();
        let image = world.resource_mut::<Assets<Image>>().add(image);
        let camera = world
            .spawn((
                ChildOf(scene),
                Camera2d,
                Camera {
                    order: -20,
                    clear_color: ClearColorConfig::Custom(FLOOR),
                    ..default()
                },
                RenderTarget::Image(image.clone().into()),
                RenderLayers::layer(layer),
                Transform::from_xyz(
                    WIDTH as f32 * TILE * 0.5,
                    -(HEIGHT as f32) * TILE * 0.5,
                    0.0,
                ),
                Msaa::Off,
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            ))
            .id();
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Circle::new(PLAYER_RADIUS));
        let material = world
            .resource_mut::<Assets<ColorMaterial>>()
            .add(Color::WHITE);
        let player = world
            .spawn((
                ChildOf(scene),
                Mesh2d(mesh),
                MeshMaterial2d(material),
                Transform::from_xyz(run.player.x.round(), -run.player.y.round(), 4.0),
                RenderLayers::layer(layer),
                Pickable::IGNORE,
            ))
            .id();
        let map = Self::group(world, scene);
        let objects = Self::group(world, scene);
        let mut view = Self {
            scene,
            camera,
            image,
            player,
            map,
            objects,
            parts: HashMap::new(),
            level: 0,
            geometry_revision: 0,
            layer,
        };
        view.synchronize(world, run);
        Ok(view)
    }

    fn group(world: &mut World, scene: Entity) -> Entity {
        world
            .spawn((ChildOf(scene), Transform::default(), Visibility::default()))
            .id()
    }

    fn rectangle(
        world: &mut World,
        parent: Entity,
        layer: usize,
        color: Color,
        center: Vec2,
        size: Vec2,
        z: f32,
    ) -> Entity {
        world
            .spawn((
                ChildOf(parent),
                Sprite::from_color(color, size),
                Transform::from_xyz(center.x, -center.y, z),
                RenderLayers::layer(layer),
                Pickable::IGNORE,
            ))
            .id()
    }

    pub fn synchronize(&mut self, world: &mut World, run: &Run) {
        if self.level != run.level {
            world.despawn(self.map);
            self.map = Self::group(world, self.scene);
            for y in 0..HEIGHT {
                let mut x = 0;
                while x < WIDTH {
                    if run.dungeon.floor[y * WIDTH + x] {
                        x += 1;
                        continue;
                    }
                    let start = x;
                    while x < WIDTH && !run.dungeon.floor[y * WIDTH + x] {
                        x += 1;
                    }
                    Self::rectangle(
                        world,
                        self.map,
                        self.layer,
                        WALL,
                        Vec2::new((start + x) as f32 * TILE * 0.5, (y as f32 + 0.5) * TILE),
                        Vec2::new((x - start) as f32 * TILE, TILE),
                        0.0,
                    );
                }
            }
            Self::rectangle(
                world,
                self.map,
                self.layer,
                EXIT,
                (run.dungeon.exit.as_vec2() + Vec2::splat(0.5)) * TILE,
                Vec2::splat(TILE),
                0.5,
            );
            self.level = run.level;
        }
        if self.geometry_revision != run.geometry_revision {
            world.despawn(self.objects);
            self.objects = Self::group(world, self.scene);
            self.parts.clear();
            for thing in &run.things {
                let material = run.catalog.material(thing.kind);
                for part in &thing.parts {
                    let [r, g, b] = material.color;
                    let entity = Self::rectangle(
                        world,
                        self.objects,
                        self.layer,
                        Color::srgb(r, g, b),
                        Vec2::ZERO,
                        Vec2::splat(material.part_size),
                        2.0,
                    );
                    self.parts.insert((thing.id, part.cell), entity);
                }
            }
            self.geometry_revision = run.geometry_revision;
        }
        for thing in &run.things {
            let size = run.catalog.material(thing.kind).part_size;
            for part in &thing.parts {
                let Some(entity) = self.parts.get(&(thing.id, part.cell)) else {
                    continue;
                };
                if let Some(mut transform) = world.get_mut::<Transform>(*entity) {
                    let point = thing.point(part.cell, size);
                    let position = Vec3::new(point.x, -point.y, 2.0);
                    let rotation = Quat::from_rotation_z(-thing.angle);
                    if transform.translation != position || transform.rotation != rotation {
                        transform.translation = position;
                        transform.rotation = rotation;
                    }
                }
            }
        }
        if let Some(mut transform) = world.get_mut::<Transform>(self.player) {
            let position = Vec3::new(run.player.x.round(), -run.player.y.round(), 4.0);
            if transform.translation != position {
                transform.translation = position;
            }
        }
    }

    pub fn active(&self, world: &mut World, active: bool) {
        if let Some(mut camera) = world.get_mut::<Camera>(self.camera)
            && camera.is_active != active
        {
            camera.is_active = active;
        }
    }

    pub fn restart(&mut self) {
        self.level = 0;
        self.geometry_revision = 0;
    }
}

pub(super) fn preview(world: &mut World, owner: Entity) {
    world.entity_mut(owner).insert(BackgroundColor(FLOOR));
    for (left, top, width, height, color) in [
        (0.0, 0.0, 100.0, 8.0, WALL),
        (0.0, 92.0, 100.0, 8.0, WALL),
        (0.0, 0.0, 6.0, 100.0, WALL),
        (94.0, 0.0, 6.0, 100.0, WALL),
        (30.0, 0.0, 6.0, 66.0, WALL),
        (60.0, 36.0, 6.0, 64.0, WALL),
        (82.0, 12.0, 6.0, 8.0, EXIT),
        (17.0, 48.0, 3.0, 4.0, Color::WHITE),
        (20.0, 46.0, 12.0, 2.0, Color::srgb(0.46, 0.28, 0.12)),
    ] {
        world.spawn((
            ChildOf(owner),
            Pickable::IGNORE,
            BackgroundColor(color),
            Node {
                position_type: PositionType::Absolute,
                left: percent(left),
                top: percent(top),
                width: percent(width),
                height: percent(height),
                border_radius: if color == Color::WHITE {
                    BorderRadius::all(percent(50))
                } else {
                    BorderRadius::ZERO
                },
                ..default()
            },
        ));
    }
}
