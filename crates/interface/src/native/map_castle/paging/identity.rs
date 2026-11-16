use super::*;
use crate::native::protein_area::{Config, FeedVersion};

#[derive(PartialEq, Eq)]
struct Scope {
    person: String,
    authority: Option<String>,
    invitation: Option<nucleus::location::Invitation>,
    records: Vec<String>,
}

impl Scope {
    fn matches(&self, sand: &MapSand) -> bool {
        self.person == sand.person
            && self.authority == sand.authority_node_id
            && self.invitation == sand.invitation
            && self.records == sand.records
    }

    fn new(sand: &MapSand) -> Self {
        Self {
            person: sand.person.clone(),
            authority: sand.authority_node_id.clone(),
            invitation: sand.invitation.clone(),
            records: sand.records.clone(),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Context {
    scope: Arc<Scope>,
    query: Option<Arc<str>>,
    version: Option<FeedVersion<'static>>,
}

#[derive(PartialEq, Eq)]
pub(in super::super) struct Key {
    context: Arc<Context>,
    bounds: [Option<u64>; 4],
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Request equality must not expose viewer/invitation data in diagnostics.
        f.debug_struct("MapRequestKey").finish_non_exhaustive()
    }
}

pub(super) struct Prepared {
    pub key: Arc<Key>,
    pub query: Option<Arc<Value>>,
    pub bounds: [f64; 4],
}

#[derive(Component)]
struct Snapshot {
    config: Option<Arc<Config>>,
    prepared: Arc<Prepared>,
    query_reusable: bool,
}

fn source(world: &World, owner: Entity) -> (Option<&Config>, Option<FeedVersion<'_>>) {
    let sand = world.get::<MapSand>(owner).unwrap();
    let Some(id) = &sand.area else {
        return (None, None);
    };
    // Resolve current scope and source every time; only pure preparation is retained.
    let scope = ui::scope(world, owner);
    let mut stack = scope.map(|(root, _)| root).into_iter().collect::<Vec<_>>();
    while let Some(entity) = stack.pop() {
        if let Some(area) = world.get::<crate::native::area::InfluenceArea>(entity)
            && area.id == *id
            && ui::scope(world, entity) == scope
        {
            return (
                area.protein.as_ref(),
                crate::native::protein_area::calendar_feed(world, entity)
                    .map(|(_, version)| version),
            );
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    (None, None)
}

fn bounds(sand: &MapSand) -> [f64; 4] {
    let size = sand.size.map(f64::from);
    let southwest = sand.camera.coordinate([0.0, size[1]], size);
    let northeast = sand.camera.coordinate([size[0], 0.0], size);
    let whole_world = size[0] >= 256.0 * 2.0_f64.powf(sand.camera.zoom);
    [
        if whole_world {
            -180.0
        } else {
            southwest.longitude
        },
        southwest.latitude,
        if whole_world {
            180.0
        } else {
            northeast.longitude
        },
        northeast.latitude,
    ]
}

pub(super) fn prepare(world: &mut World, owner: Entity) -> Arc<Prepared> {
    let sand = world.get::<MapSand>(owner).unwrap();
    let (config, version) = source(world, owner);
    let bounds = bounds(sand);
    let previous = world.get::<Snapshot>(owner);
    let same_config = previous.is_some_and(|previous| match (previous.config.as_deref(), config) {
        (Some(previous), Some(config)) => previous.same_query_inputs(config),
        (None, None) => true,
        _ => false,
    });
    let same_scope =
        previous.is_some_and(|previous| previous.prepared.key.context.scope.matches(sand));
    let same_version =
        previous.is_some_and(|previous| previous.prepared.key.context.version == version);
    let same_bounds = previous.is_some_and(|previous| {
        previous.prepared.bounds.map(f64::to_bits) == bounds.map(f64::to_bits)
    });
    let reusable = same_config && previous.is_some_and(|previous| previous.query_reusable);
    if reusable && same_scope && same_version && same_bounds {
        return previous.unwrap().prepared.clone();
    }
    let _prepare = nucleus::performance::Span::new("map-request-preparation");
    let query = if reusable {
        previous.unwrap().prepared.query.clone()
    } else {
        config
            .and_then(|config| config.query().ok())
            .and_then(|query| serde_json::to_value(query).ok())
            .map(Arc::new)
    };
    let query_reusable = config.is_none() || query.is_some();
    let query_identity = if reusable {
        previous.unwrap().prepared.key.context.query.clone()
    } else {
        query
            .as_deref()
            .map(|query| Arc::<str>::from(serde_json::to_string(query).unwrap()))
    };
    let same_query =
        previous.is_some_and(|previous| previous.prepared.key.context.query == query_identity);
    // Rejected drafts remain live; an unchanged failure has no new page identity.
    if same_config && same_scope && same_version && same_bounds && same_query {
        return previous.unwrap().prepared.clone();
    }
    let context = if same_scope && same_version && same_query {
        previous.unwrap().prepared.key.context.clone()
    } else {
        let scope = if same_scope {
            previous.unwrap().prepared.key.context.scope.clone()
        } else {
            Arc::new(Scope::new(sand))
        };
        Arc::new(Context {
            scope,
            query: query_identity,
            version: version.map(FeedVersion::into_owned),
        })
    };
    // Match the former JSON identity: non-finite coordinates encoded as null;
    // finite signed zero retains its distinct representation. The raw bounds
    // above remain the actual request values.
    let identity_bounds = bounds.map(|value| value.is_finite().then(|| value.to_bits()));
    let key = previous
        .filter(|previous| {
            Arc::ptr_eq(&previous.prepared.key.context, &context)
                && previous.prepared.key.bounds == identity_bounds
        })
        .map_or_else(
            || {
                Arc::new(Key {
                    context,
                    bounds: identity_bounds,
                })
            },
            |previous| previous.prepared.key.clone(),
        );
    let config = if same_config {
        previous.unwrap().config.clone()
    } else {
        config.cloned().map(Arc::new)
    };
    let prepared = Arc::new(Prepared { key, query, bounds });
    world.entity_mut(owner).insert(Snapshot {
        config,
        prepared: prepared.clone(),
        query_reusable,
    });
    nucleus::performance::add("map-request-identities-prepared", 1);
    prepared
}
