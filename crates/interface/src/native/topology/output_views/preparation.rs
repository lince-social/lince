// non-lean-yet
// Scope: struct Queries, fn query, struct Preparation.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use bevy::{
    asset::AssetId,
    ecs::query::{QueryData, QueryFilter, QueryState},
};

#[derive(Default)]
pub(super) struct Queries {
    pub roots: Option<QueryState<Entity, With<Workspaces>>>,
    pub backgrounds: Option<QueryState<(Entity, &'static Background)>>,
    pub cameras: Option<QueryState<(Entity, &'static OutputCamera)>>,
    pub instances: Option<QueryState<(Entity, &'static Instance)>>,
    pub fills: Option<QueryState<(Entity, &'static Fill)>>,
    pub areas: Option<
        QueryState<(
            Entity,
            &'static crate::native::area::InfluenceArea,
            &'static ChildOf,
            &'static WorkspaceMember,
        )>,
    >,
    pub members: Option<
        QueryState<(
            Entity,
            &'static CanvasItem,
            &'static ChildOf,
            &'static WorkspaceMember,
            &'static Surface,
        )>,
    >,
}

pub(super) fn query<'a, D: QueryData, F: QueryFilter>(
    world: &mut World,
    prepared: &'a mut Option<QueryState<D, F>>,
) -> &'a mut QueryState<D, F> {
    prepared.get_or_insert_with(|| {
        nucleus::performance::add("native-output-sync-query-plans", 1);
        world.query_filtered()
    })
}

#[derive(Resource, Default)]
pub(super) struct Preparation {
    pub queries: Queries,
    pub views: Vec<RenderView>,
    pub passive: HashSet<ViewKey>,
    pub backgrounds: HashMap<
        ViewKey,
        (
            Entity,
            Entity,
            AssetId<crate::native::canvas_background::PatternMaterial>,
        ),
    >,
    pub cameras: Vec<(Entity, ViewKey)>,
    pub instances: HashMap<(ViewKey, Entity), Entity>,
    pub living: HashSet<(ViewKey, Entity)>,
    pub fills: HashMap<Fill, Entity>,
    pub live_fills: HashSet<Fill>,
    pub depths: HashMap<u64, usize>,
    pub areas: Vec<(Entity, u64)>,
    pub members: Vec<(Entity, CanvasItem, u64, super::super::stacking::Order)>,
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
}

impl Preparation {
    pub fn clear_batch(&mut self) {
        let no_views = self.views.is_empty();
        let no_instances = self.living.is_empty() && self.live_fills.is_empty();
        self.views.clear();
        self.passive.clear();
        self.backgrounds.clear();
        self.cameras.clear();
        self.instances.clear();
        self.living.clear();
        self.fills.clear();
        self.live_fills.clear();
        self.depths.clear();
        self.areas.clear();
        self.members.clear();
        self.positions.clear();
        self.uvs.clear();
        if no_views {
            let queries = std::mem::take(&mut self.queries);
            *self = Self {
                queries,
                ..Default::default()
            };
        } else if no_instances {
            self.positions = Vec::new();
            self.uvs = Vec::new();
        }
    }
}
