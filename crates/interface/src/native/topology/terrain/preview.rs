use super::*;

struct Prepared {
    input: Arc<Input>,
    formula: Option<Arc<Formula>>,
}

#[derive(Default)]
pub(super) struct Preparation {
    entries: HashMap<Entity, Prepared>,
}

impl Preparation {
    fn field(
        &mut self,
        entity: Entity,
        definition: &TopologyEffect,
        id: std::borrow::Cow<'_, str>,
        center: DVec2,
        heading: f64,
    ) -> Option<Field> {
        let previous = self.entries.get(&entity);
        let same_definition = previous.is_some_and(|p| p.input.definition == *definition);
        if same_definition {
            nucleus::performance::add("terrain-preview-formula-reuses", 1);
        }
        if previous.is_none_or(|p| {
            !same_definition
                || p.input.id != id.as_ref()
                || p.input.center != center
                || p.input.heading != heading
        }) {
            let formula = if same_definition {
                previous.unwrap().formula.clone()
            } else {
                nucleus::performance::add("terrain-formula-compilations", 1);
                definition.compile().ok().map(Arc::new)
            };
            self.entries.insert(
                entity,
                Prepared {
                    input: Arc::new(Input {
                        entity,
                        id: id.into_owned(),
                        definition: definition.clone(),
                        center,
                        heading,
                    }),
                    formula,
                },
            );
            nucleus::performance::add("terrain-input-snapshots", 1);
        }
        let prepared = &self.entries[&entity];
        prepared.formula.as_ref().map(|formula| Field {
            input: prepared.input.clone(),
            formula: formula.clone(),
        })
    }
}

pub(super) fn prepare(
    world: &mut World,
    terrain: &mut Terrain,
) -> HashMap<(Entity, u64), Option<Vec<Field>>> {
    let _preparation = nucleus::performance::Span::new("terrain-preview-preparation");
    let mut query = world.query::<(
        Entity,
        &Effect,
        Option<&Preview>,
        &ChildOf,
        &crate::native::workspace::WorkspaceMember,
    )>();
    let views = world.get_resource::<super::super::previews::Previews>();
    let previews: HashMap<_, _> = query
        .iter(world)
        .filter(|(_, _, preview, parent, member)| {
            preview.is_some()
                || views.is_some_and(|views| {
                    views.0.iter().any(|view| {
                        view.root == parent.parent()
                            && super::super::holes::live_in(
                                world,
                                view.root,
                                view.workspace,
                                member.0,
                            )
                    })
                })
        })
        .map(|(entity, _, preview, _, _)| (entity, preview.is_some()))
        .collect();
    terrain
        .previews
        .entries
        .retain(|entity, _| previews.contains_key(entity));
    let mut physical_fields = HashMap::<(Entity, u64), Option<Vec<Field>>>::new();
    for (entity, preview) in previews {
        let definition = if preview {
            &world.get::<Preview>(entity).unwrap().0
        } else {
            &world.get::<Effect>(entity).unwrap().0
        };
        if !definition.enabled
            || definition.amplitude == 0.0
            || world
                .get::<Fault>(entity)
                .is_some_and(|f| f.1.same_surface(definition))
        {
            terrain.previews.entries.remove(&entity);
            continue;
        }
        let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            terrain.previews.entries.remove(&entity);
            continue;
        };
        let Some(workspace) = world
            .get::<crate::native::workspace::WorkspaceMember>(entity)
            .map(|m| m.0)
        else {
            terrain.previews.entries.remove(&entity);
            continue;
        };
        let Some(center) = world
            .get::<crate::native::canvas::CanvasItem>(entity)
            .map(|item| item.position)
        else {
            terrain.previews.entries.remove(&entity);
            continue;
        };
        let id = world
            .get::<crate::native::canvas_host::Identity>(entity)
            .map_or_else(
                || entity.to_bits().to_string().into(),
                |id| id.0.as_str().into(),
            );
        let heading = super::super::spatial(world, entity)
            .rotation()
            .to_euler(EulerRot::YXZ)
            .0;
        let Some(field) = terrain
            .previews
            .field(entity, definition, id, center, heading)
        else {
            continue;
        };
        if world
            .get::<Fault>(entity)
            .zip(world.get::<Effect>(entity))
            .is_some_and(|(fault, effect)| !fault.1.same_surface(&effect.0))
        {
            world.entity_mut(entity).remove::<Fault>();
        }
        if let std::collections::hash_map::Entry::Vacant(saved) =
            physical_fields.entry((root, workspace))
        {
            let original = terrain.fields.remove(&(root, workspace));
            if let Some(fields) = &original {
                terrain.fields.insert((root, workspace), fields.clone());
            }
            saved.insert(original);
            nucleus::performance::add("terrain-preview-workspace-overrides", 1);
        }
        let fields = terrain.fields.entry((root, workspace)).or_default();
        fields.retain(|field| field.input.entity != entity);
        fields.push(field);
        fields.sort_by(|a, b| a.input.id.cmp(&b.input.id));
    }
    if terrain.previews.entries.is_empty() {
        terrain.previews.entries = HashMap::new();
    }
    physical_fields
}
