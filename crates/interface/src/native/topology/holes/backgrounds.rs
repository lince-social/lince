// non-lean-yet
// Scope: struct Preparation, fn update.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 71, 90.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   70, 75.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (70, 93, 94, 98, 100, 107, 113, 120).

use super::*;

#[derive(Default)]
pub(super) struct Preparation {
    layers: Vec<(Entity, usize, u64, usize)>,
    live: HashSet<(Entity, u64)>,
    old: Vec<(Entity, Entity, u64)>,
    first: HashMap<(Entity, u64), Entity>,
    points: Vec<[f32; 3]>,
}

impl Preparation {
    fn clear(&mut self) {
        if self.live.is_empty() {
            *self = Self::default();
        } else {
            self.layers.clear();
            self.live.clear();
            self.old.clear();
            self.first.clear();
            self.points.clear();
        }
    }
}

pub(super) fn update(
    world: &mut World,
    layers_query: &mut QueryState<(Entity, &'static Layers)>,
    backgrounds_query: &mut QueryState<(Entity, &'static Background)>,
    mut preparation: Local<Preparation>,
) {
    let _backgrounds = nucleus::performance::Span::new("native-hole-backgrounds");
    let Preparation {
        layers,
        live,
        old,
        first,
        points,
    } = &mut *preparation;
    layers.extend(layers_query.iter(world).flat_map(|(root, layers)| {
        layers
            .0
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.depth > 0)
            .map(move |(index, layer)| (root, index, layer.workspace, layer.depth))
    }));
    live.extend(
        layers
            .iter()
            .map(|(root, _, workspace, _)| (*root, *workspace)),
    );
    for (entity, background) in backgrounds_query.iter(world) {
        old.push((entity, background.root, background.workspace));
        first
            .entry((background.root, background.workspace))
            .or_insert(entity);
    }
    for (entity, root, workspace) in old.drain(..) {
        if !live.contains(&(root, workspace)) {
            world.despawn(entity);
        }
    }
    nucleus::performance::add("native-hole-background-layers", layers.len() as u64);
    for (root, index, workspace, depth) in layers.drain(..) {
        let color = crate::native::canvas_background::color(
            crate::native::canvas_background::for_workspace(world, root, workspace).background,
        );
        let origin = super::super::presentation::origin(world, root);
        let layer = &world.get::<Layers>(root).unwrap().0[index];
        let pieces = remove(layer.openings.clone(), &layer.cuts);
        points.clear();
        for piece in pieces {
            for index in 1..piece.len().saturating_sub(1) {
                for point in [piece[0], piece[index + 1], piece[index]] {
                    points.push([
                        (point.x - origin.x) as f32,
                        -(depth as f32) * 0.2 - 0.15,
                        (point.y - origin.z) as f32,
                    ]);
                }
            }
        }
        let mesh = || {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; points.len()])
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points.clone())
        };
        if let Some(entity) = first.get(&(root, workspace)).copied() {
            if world.get::<Background>(entity).unwrap().points != *points {
                let mesh_id = world.get::<Mesh3d>(entity).unwrap().0.id();
                world
                    .resource_mut::<Assets<Mesh>>()
                    .insert(mesh_id, mesh())
                    .unwrap();
                std::mem::swap(
                    &mut world.get_mut::<Background>(entity).unwrap().points,
                    points,
                );
                nucleus::performance::add("native-hole-background-mesh-writes", 1);
            }
            let material = world
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0
                .id();
            if world
                .resource::<Assets<StandardMaterial>>()
                .get(material)
                .unwrap()
                .base_color
                != color
            {
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(material)
                    .unwrap()
                    .base_color = color;
                nucleus::performance::add("native-hole-background-material-writes", 1);
            }
        } else {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                });
            world.spawn((
                Background {
                    root,
                    workspace,
                    points: std::mem::take(points),
                },
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::default(),
                Pickable::IGNORE,
            ));
        }
    }
    preparation.clear();
}
