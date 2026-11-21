// non-lean-yet
// Scope: type Pixels, trait Request, struct Job, struct Workers.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 117, 145, 151.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   61.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (114).
// - [review: §5 Panics] Verify panic isolation boundary and preserve the failure contract. Sites
//   156.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 47.

use bevy::prelude::*;
use std::sync::Arc;
use tokio::sync::oneshot;

pub(super) type Pixels = Result<(u32, u32, Vec<u8>), String>;

pub(super) trait Request: Component {
    const CAPACITY: usize;
    const STOPPED: &'static str;
    const PANICKED: &'static str = Self::STOPPED;

    fn source(&self) -> &Arc<str>;
    fn label(&self) -> Entity;
    fn render(source: &str) -> Pixels;
    fn layout(width: u32) -> Node;
    fn failure(source: &str, error: &str) -> String;
}

struct Job {
    source: Arc<str>,
    result: oneshot::Receiver<Pixels>,
}

#[derive(Resource)]
struct Workers<T: Request> {
    active: Vec<Job>,
    requests: QueryState<(Entity, &'static T)>,
    targets: Vec<(Entity, Entity)>,
}

impl<T: Request> FromWorld for Workers<T> {
    fn from_world(world: &mut World) -> Self {
        Self {
            active: Vec::with_capacity(T::CAPACITY),
            requests: world.query(),
            targets: Vec::new(),
        }
    }
}

fn same_source(first: &Arc<str>, second: &Arc<str>) -> bool {
    Arc::ptr_eq(first, second) || first.as_ref() == second.as_ref()
}

pub(super) fn update<T: Request>(world: &mut World) {
    world.init_resource::<Workers<T>>();
    world.resource_scope(|world, mut workers: Mut<Workers<T>>| {
        workers.complete(world);
        workers.admit(world);
    });
}

impl<T: Request> Workers<T> {
    fn complete(&mut self, world: &mut World) {
        if self.active.is_empty() {
            return;
        }
        let _completion = nucleus::performance::Span::new("description-raster-completion");
        let mut index = 0;
        while index < self.active.len() {
            let result = match self.active[index].result.try_recv() {
                Ok(result) => result,
                Err(oneshot::error::TryRecvError::Closed) => Err(T::STOPPED.into()),
                Err(oneshot::error::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
            };
            let job = self.active.remove(index);
            self.publish(world, &job.source, result);
        }
    }

    fn publish(&mut self, world: &mut World, source: &Arc<str>, result: Pixels) {
        self.targets.clear();
        self.targets.extend(
            self.requests
                .iter(world)
                .filter(|(_, request)| same_source(request.source(), source))
                .map(|(owner, request)| (owner, request.label())),
        );
        if self.targets.is_empty() {
            nucleus::performance::add("description-raster-orphaned-completions", 1);
            return;
        }
        let prepared = result
            .map(|(width, height, pixels)| {
                nucleus::performance::add(
                    "description-raster-pixel-bytes-moved",
                    pixels.len() as u64,
                );
                world.init_resource::<Assets<Image>>();
                let image = world.resource_mut::<Assets<Image>>().add(Image::new(
                    bevy::render::render_resource::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    bevy::render::render_resource::TextureDimension::D2,
                    pixels,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                ));
                (width, image)
            })
            .map_err(|error| T::failure(source, &error));
        for (owner, label) in self.targets.drain(..) {
            world.entity_mut(owner).remove::<T>();
            match &prepared {
                Ok((width, image)) => {
                    world.despawn(label);
                    let content = world
                        .get::<crate::native::embedded_media::Media>(owner)
                        .unwrap()
                        .content;
                    world.spawn((
                        ImageNode::new(image.clone()),
                        T::layout(*width),
                        ChildOf(content),
                    ));
                }
                Err(error) => {
                    if let Some(mut text) = world.get_mut::<Text>(label) {
                        text.0.clone_from(error);
                    }
                }
            }
            nucleus::performance::add("description-raster-views-published", 1);
        }
    }

    fn admit(&mut self, world: &World) {
        if self.active.len() >= T::CAPACITY {
            return;
        }
        let _admission = nucleus::performance::Span::new("description-raster-admission");
        for (_, request) in self.requests.iter(world) {
            if self
                .active
                .iter()
                .any(|job| same_source(&job.source, request.source()))
            {
                continue;
            }
            let source = request.source().clone();
            let (sender, result) = oneshot::channel();
            let wake = world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned();
            self.active.push(Job {
                source: source.clone(),
                result,
            });
            nucleus::performance::add("description-raster-jobs-admitted", 1);
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(|| T::render(&source))
                    .unwrap_or_else(|_| Err(T::PANICKED.into()));
                let _ = sender.send(result);
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
            if self.active.len() == T::CAPACITY {
                break;
            }
        }
        if self.active.is_empty() {
            self.targets = Vec::new();
        }
    }
}
