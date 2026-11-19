use super::*;
use bevy::ecs::query::{QueryData, QueryFilter, QueryState};

#[derive(Resource, Default)]
pub(super) struct Queries {
    pub(super) new_roots: Option<QueryState<Entity, (With<BoxRoot>, Without<Workspaces>)>>,
    pub(super) owners: Option<
        QueryState<
            Entity,
            (
                With<Workspaces>,
                Without<crate::native::laboratory::LaboratoryRoot>,
                Without<crate::native::component_push::composition::GeneratedCanvas>,
            ),
        >,
    >,
    pub(super) items: Option<
        QueryState<(
            Entity,
            &'static ChildOf,
            &'static CanvasItem,
            &'static WorkspaceMember,
            Option<&'static StoredSand>,
            Option<&'static RecordPlacement>,
        )>,
    >,
    pub(super) members: Option<QueryState<(Entity, &'static ChildOf, &'static WorkspaceMember)>>,
    pub(super) areas: Option<
        QueryState<
            (
                Entity,
                &'static crate::native::area::InfluenceArea,
                &'static WorkspaceMember,
                &'static ChildOf,
            ),
            Without<crate::native::protein_area::grouping::GeneratedGroup>,
        >,
    >,
}

pub(super) fn with<R>(world: &mut World, run: impl FnOnce(&mut World, &mut Queries) -> R) -> R {
    world.init_resource::<Queries>();
    world.resource_scope(|world, mut queries: Mut<Queries>| run(world, &mut queries))
}

pub(super) fn query<'a, D: QueryData, F: QueryFilter>(
    world: &mut World,
    cached: &'a mut Option<QueryState<D, F>>,
) -> &'a mut QueryState<D, F> {
    cached.get_or_insert_with(|| {
        nucleus::performance::add("workspace-query-plans-created", 1);
        world.query_filtered()
    })
}
