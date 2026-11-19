use super::*;

pub(super) type Corners = [[f64; 2]; 4];

pub(super) struct ResolvedItem {
    pub(super) id: ItemId,
    pub(super) output: OutputId,
    pub(super) mapping: ContentMapping,
    pub(super) floating: bool,
    pub(super) clip: Option<Corners>,
}

fn clip_matches(previous: Option<&Vec<Vec<[f64; 2]>>>, corners: Option<&Corners>) -> bool {
    match (previous, corners) {
        (None, None) => true,
        (Some(polygons), Some(corners)) => {
            polygons.len() == 1 && polygons[0].as_slice() == corners.as_slice()
        }
        _ => false,
    }
}

fn assign_clip(previous: &mut Option<Vec<Vec<[f64; 2]>>>, corners: Option<Corners>) {
    let Some(corners) = corners else {
        *previous = None;
        return;
    };
    let polygons = previous.get_or_insert_with(Vec::new);
    polygons.truncate(1);
    if let Some(polygon) = polygons.first_mut() {
        polygon.clear();
        polygon.extend_from_slice(&corners);
    } else {
        polygons.push(corners.to_vec());
    }
}

impl ResolvedItem {
    fn matches(&self, previous: &ItemGeometry) -> bool {
        self.id == previous.id
            && self.output == previous.output
            && self.mapping == previous.mapping
            && self.floating == previous.floating
            && clip_matches(previous.clip.as_ref(), self.clip.as_ref())
    }

    fn apply(&self, previous: &mut ItemGeometry) {
        previous.id = self.id;
        previous.output = self.output;
        previous.mapping = self.mapping;
        previous.floating = self.floating;
        if !clip_matches(previous.clip.as_ref(), self.clip.as_ref()) {
            assign_clip(&mut previous.clip, self.clip);
            engine::performance::add("desktop-geometry-clips-prepared", 1);
        }
    }
}

pub(super) fn publish_resolved(
    world: &mut World,
    outputs: &mut Vec<OutputViewport>,
    items: &[ResolvedItem],
    focused: Option<ItemId>,
) {
    let previous = &world.resource::<PublishedGeometry>().0;
    if previous.as_ref().is_some_and(|previous| {
        previous.outputs == *outputs
            && previous.focused == focused
            && previous.items.len() == items.len()
            && items
                .iter()
                .zip(&previous.items)
                .all(|(item, previous)| item.matches(previous))
    }) {
        return;
    }
    let revision = previous
        .as_ref()
        .map_or(1, |previous| previous.revision.saturating_add(1));
    let mut published = world.resource_mut::<PublishedGeometry>();
    let snapshot = published.0.get_or_insert_with(|| GeometrySnapshot {
        revision,
        outputs: Vec::new(),
        items: Vec::new(),
        focused,
    });
    std::mem::swap(&mut snapshot.outputs, outputs);
    engine::performance::add(
        "desktop-resolved-rows-removed",
        snapshot.items.len().saturating_sub(items.len()) as u64,
    );
    snapshot.items.truncate(items.len());
    for (index, item) in items.iter().enumerate() {
        if let Some(previous) = snapshot.items.get_mut(index) {
            if item.matches(previous) {
                continue;
            }
            item.apply(previous);
        } else {
            let mut next = ItemGeometry {
                id: item.id,
                output: item.output,
                mapping: item.mapping,
                floating: item.floating,
                clip: None,
            };
            item.apply(&mut next);
            snapshot.items.push(next);
        }
        engine::performance::add("desktop-resolved-rows-written", 1);
    }
    snapshot.focused = focused;
    snapshot.revision = revision;
}

#[derive(Resource)]
struct Publication {
    managed: QueryState<(
        Entity,
        &'static Managed,
        &'static ChildOf,
        &'static WorkspaceMember,
        Option<&'static ApplicationWindow>,
    )>,
    entities: HashMap<ItemId, (Entity, Entity, u64)>,
    outputs: HashMap<(OutputId, u64), Rectangle>,
    views: HashMap<(Entity, u64), CanvasView>,
    patches: Vec<Patch>,
}

impl FromWorld for Publication {
    fn from_world(world: &mut World) -> Self {
        Self {
            managed: world.query(),
            entities: HashMap::new(),
            outputs: HashMap::new(),
            views: HashMap::new(),
            patches: Vec::new(),
        }
    }
}

struct Patch {
    index: usize,
    mapping: ContentMapping,
    clip: Clip,
}

enum Clip {
    Keep,
    Replace(Option<Corners>),
}

impl Clip {
    fn prepare(previous: Option<&Vec<Vec<[f64; 2]>>>, corners: Option<Corners>) -> Self {
        if clip_matches(previous, corners.as_ref()) {
            Self::Keep
        } else {
            engine::performance::add("desktop-geometry-clips-prepared", 1);
            Self::Replace(corners)
        }
    }
}

impl Publication {
    fn prepare(&mut self, world: &World, snapshot: &GeometrySnapshot) -> bool {
        self.entities.clear();
        self.outputs.clear();
        self.views.clear();
        self.patches.clear();
        if snapshot.items.is_empty() {
            return false;
        }
        self.entities.extend(self.managed.iter(world).map(
            |(entity, managed, parent, member, application)| {
                (
                    application.map_or(ItemId::Native(managed.id.0), |application| {
                        ItemId::Application(application.surface)
                    }),
                    (entity, parent.parent(), member.0),
                )
            },
        ));
        for output in &snapshot.outputs {
            self.outputs
                .entry((output.id, output.workspace))
                .or_insert(output.bounds);
        }
        let mut changed = false;
        engine::performance::add(
            "desktop-geometry-rows-examined",
            snapshot.items.len() as u64,
        );
        for (index, geometry) in snapshot.items.iter().enumerate() {
            changed |= geometry.mapping.corners.as_ref().is_some_and(|corners| {
                corners
                    .iter()
                    .flatten()
                    .any(|coordinate| coordinate.is_nan())
            }) || geometry.clip.as_ref().is_some_and(|polygons| {
                polygons
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|coordinate| coordinate.is_nan())
            });
            let Some((entity, root, workspace)) = self.entities.get(&geometry.id).copied() else {
                continue;
            };
            let Some(bounds) = self.outputs.get(&(geometry.output, workspace)) else {
                continue;
            };
            let Some(item) = crate::native::canvas::presented(world, entity) else {
                continue;
            };
            let view = *self
                .views
                .entry((root, workspace))
                .or_insert_with(|| crate::native::output_views::canvas(world, root, workspace));
            let offset = super::super::viewport::visual_offset(world, entity);
            let scale = if world
                .get::<Managed>(entity)
                .is_some_and(|managed| managed.floating)
            {
                f64::from(
                    world
                        .get::<crate::native::area_effects::AreaScale>(entity)
                        .map_or(1.0, |scale| scale.0),
                )
            } else {
                1.0
            };
            let size = item.size.as_dvec2() * scale;
            let mut mapping = geometry.mapping;
            let mut clip = Clip::Keep;
            if let Some(rectangle) = Rectangle::new(
                ((item.position - offset.as_dvec2() - view.center - size * 0.5) * view.zoom
                    + bevy::math::DVec2::from_array(bounds.size()) * 0.5)
                    .to_array(),
                (size * view.zoom).to_array(),
            ) {
                mapping.canvas = rectangle;
                if world.get::<CompactViewport>(entity).is_some()
                    || world.get::<CompactViewportPreview>(entity).is_some()
                {
                    clip = Clip::prepare(
                        geometry.clip.as_ref(),
                        compact_corners(world, entity, rectangle),
                    );
                }
            }
            if matches!(geometry.id, ItemId::Native(_))
                && let Some(surface) = Rectangle::new([0.0; 2], item.size.as_dvec2().to_array())
            {
                mapping.surface = surface;
            }
            if geometry.mapping != mapping || matches!(&clip, Clip::Replace(_)) {
                self.patches.push(Patch {
                    index,
                    mapping,
                    clip,
                });
            }
        }
        changed || !self.patches.is_empty()
    }
}

pub(super) fn publish(world: &mut World) {
    let _publication = engine::performance::Span::new("desktop-geometry-publish");
    if world
        .get_resource::<PublishedGeometry>()
        .is_none_or(|geometry| geometry.0.is_none())
    {
        return;
    }
    world.init_resource::<Publication>();
    world.resource_scope(|world, mut publication: Mut<Publication>| {
        let snapshot = world.resource::<PublishedGeometry>().0.as_ref().unwrap();
        if !publication.prepare(world, snapshot) {
            return;
        }
        let mut published = world.resource_mut::<PublishedGeometry>();
        let snapshot = published.0.as_mut().unwrap();
        engine::performance::add(
            "desktop-geometry-rows-written",
            publication.patches.len() as u64,
        );
        for patch in publication.patches.drain(..) {
            let item = &mut snapshot.items[patch.index];
            item.mapping = patch.mapping;
            if let Clip::Replace(clip) = patch.clip {
                assign_clip(&mut item.clip, clip);
            }
        }
        snapshot.revision = snapshot.revision.saturating_add(1);
    });
}
