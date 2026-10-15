use super::*;
use crate::server::{SurfaceNode, WindowSnapshot};

pub(super) struct SurfaceView {
    entity: Entity,
    image: Handle<Image>,
    material: Handle<ClientMaterial>,
    texture: Option<wgpu::Texture>,
    version: Option<u64>,
    opaque: bool,
}

pub(super) fn remove(world: &mut World, node: SurfaceView) {
    world.despawn(node.entity);
    world.resource_mut::<Textures>().0.remove(&node.image.id());
    world
        .resource_mut::<Assets<Image>>()
        .remove(node.image.id());
    world
        .resource_mut::<Assets<ClientMaterial>>()
        .remove(node.material.id());
}

impl Presentation {
    pub fn sync_windows(&mut self, windows: &[WindowSnapshot]) -> Result<bool, ProbeFailure> {
        let live: HashSet<_> = windows.iter().map(|window| window.id).collect();
        self.window_requests.retain(|id, _| live.contains(id));
        let destroyed: Vec<_> = self
            .identities
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        for id in destroyed {
            self.close_window(id);
        }
        let mut pending = false;
        for window in windows {
            if self.rejected.contains(&window.id) {
                continue;
            }
            if !window.mapped {
                self.hide_window(window.id);
                continue;
            }
            if !self.windows.contains_key(&window.id) {
                self.open_window(window.id, &window.title, &window.app_id)?;
            }
            if let Some(current) = self.windows.get_mut(&window.id) {
                current.parent = window.parent;
            }
            self.window_metadata(window.id, &window.title, &window.app_id)?;
            match self.window_tree(window) {
                Ok(waiting) => pending |= waiting,
                Err(error) => {
                    self.rejected.insert(window.id);
                    native::notifications::report(
                        self.app.world(),
                        "Park application",
                        &error.to_string(),
                    );
                }
            }
        }
        Ok(pending)
    }

    fn window_tree(&mut self, snapshot: &WindowSnapshot) -> Result<bool, ProbeFailure> {
        let Some(bounds) = snapshot.bounds() else {
            return Ok(false);
        };
        let window = self.windows.get_mut(&snapshot.id).unwrap();
        let world = self.app.world_mut();
        world
            .entity_mut(window.entity)
            .remove::<MaterialNode<ClientMaterial>>();
        world.get_mut::<Node>(window.entity).unwrap().overflow = bevy::ui::Overflow::clip();
        let mut layout = world
            .get::<native::layout::LayoutBox>(window.entity)
            .copied()
            .unwrap();
        for axis in 0..2 {
            let minimum = snapshot.minimum[axis].max(1) as f32;
            let maximum = if snapshot.maximum[axis] == 0 {
                native::layout::LIMIT
            } else {
                snapshot.maximum[axis].max(snapshot.minimum[axis]).max(1) as f32
            };
            layout.rules.axes[axis].min = minimum.min(native::layout::LIMIT);
            layout.rules.axes[axis].max = maximum.min(native::layout::LIMIT);
            layout.rules.axes[axis].size = layout.rules.axes[axis]
                .size
                .clamp(layout.rules.axes[axis].min, layout.rules.axes[axis].max);
        }
        if world.get::<native::layout::LayoutBox>(window.entity) != Some(&layout) {
            world.entity_mut(window.entity).insert(layout);
        }
        let content_size = [bounds.size.w.ceil() as u32, bounds.size.h.ceil() as u32];
        if world
            .get::<native::layout::desktop::ApplicationWindow>(window.entity)
            .unwrap()
            .content_size
            != content_size
        {
            world
                .get_mut::<native::layout::desktop::ApplicationWindow>(window.entity)
                .unwrap()
                .content_size = content_size;
        }
        let live: HashSet<_> = snapshot.nodes.iter().map(|node| &node.surface).collect();
        let removed: Vec<_> = window
            .nodes
            .keys()
            .filter(|surface| !live.contains(surface))
            .cloned()
            .collect();
        for surface in removed {
            remove(world, window.nodes.remove(&surface).unwrap());
        }
        let mut pending = false;
        for (index, node) in snapshot.nodes.iter().rev().enumerate() {
            let view = window
                .nodes
                .entry(node.surface.clone())
                .or_insert_with(|| create(world, window.entity));
            if view.version != Some(node.content.version) {
                if let Some((texture, opaque)) = self
                    .renderer
                    .import_client(&node.content, view.texture.as_ref())?
                {
                    if view.texture.as_ref() != Some(&texture) {
                        world
                            .resource_mut::<Textures>()
                            .0
                            .insert(view.image.id(), texture.clone());
                        *world
                            .resource_mut::<Assets<Image>>()
                            .get_mut(&view.image)
                            .unwrap() = Image::new_target_texture(
                            texture.width(),
                            texture.height(),
                            texture.format(),
                            None,
                        );
                        view.texture = Some(texture);
                    }
                    view.version = Some(node.content.version);
                    view.opaque = opaque;
                    self.uploads.push(node.content.clone());
                } else {
                    pending = true;
                }
            }
            place(world, view, node, bounds, index);
        }
        if window.nodes.values().all(|view| view.texture.is_some()) {
            world
                .entity_mut(window.entity)
                .remove::<native::castle::Pending>();
        }
        Ok(pending)
    }
}

fn create(world: &mut World, parent: Entity) -> SurfaceView {
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            1,
            1,
            wgpu::TextureFormat::Bgra8Unorm,
            None,
        ));
    let material = world
        .resource_mut::<Assets<ClientMaterial>>()
        .add(ClientMaterial {
            content: image.clone(),
            mapping: Default::default(),
        });
    let entity = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            MaterialNode(material.clone()),
            ChildOf(parent),
        ))
        .id();
    SurfaceView {
        entity,
        image,
        material,
        texture: None,
        version: None,
        opaque: false,
    }
}

fn place(
    world: &mut World,
    view: &SurfaceView,
    source: &SurfaceNode,
    bounds: smithay::utils::Rectangle<f64, smithay::utils::Logical>,
    index: usize,
) {
    let geometry = Node {
        position_type: PositionType::Absolute,
        left: percent(
            ((f64::from(source.origin[0]) - bounds.loc.x) / bounds.size.w * 100.0) as f32,
        ),
        top: percent(((f64::from(source.origin[1]) - bounds.loc.y) / bounds.size.h * 100.0) as f32),
        width: percent((source.layout.size[0] / bounds.size.w * 100.0) as f32),
        height: percent((source.layout.size[1] / bounds.size.h * 100.0) as f32),
        ..default()
    };
    world
        .entity_mut(view.entity)
        .insert((geometry, ZIndex(index as i32)));
    let mapping = Mapping {
        u: Vec4::new(
            source.layout.uv_origin[0],
            source.layout.uv_x[0],
            source.layout.uv_y[0],
            if view.opaque { 1.0 } else { 0.0 },
        ),
        v: Vec4::new(
            source.layout.uv_origin[1],
            source.layout.uv_x[1],
            source.layout.uv_y[1],
            0.0,
        ),
    };
    world
        .resource_mut::<Assets<ClientMaterial>>()
        .get_mut(&view.material)
        .unwrap()
        .mapping = mapping;
}
