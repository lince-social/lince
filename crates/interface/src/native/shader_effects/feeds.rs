use super::*;
use bevy::platform::collections::{Equivalent, HashMap as BorrowedMap};
use std::collections::hash_map::RandomState;

pub(super) type Map = BorrowedMap<(Source, String), Feed, RandomState>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key<'a>(pub &'a Source, pub &'a str);

impl std::hash::Hash for Key<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&(self.0, self.1), state);
    }
}

impl Equivalent<(Source, String)> for Key<'_> {
    fn equivalent(&self, key: &(Source, String)) -> bool {
        self.0 == &key.0 && self.1 == key.1.as_str()
    }
}

impl From<&Key<'_>> for (Source, String) {
    fn from(key: &Key<'_>) -> Self {
        (key.0.clone(), key.1.to_owned())
    }
}

impl Feeds {
    pub(super) fn get(&self, source: &Source, uid: &str) -> Option<&Feed> {
        self.0.get(&Key(source, uid))
    }
}

#[derive(Clone, Copy)]
pub(super) enum Owner {
    Editor(Entity),
    Surface(Entity),
}

impl Owner {
    fn binding(self, world: &World) -> Option<(Key<'_>, Entity)> {
        match self {
            Self::Editor(entity) => {
                let Target::Record(binding) = &world.get::<Editor>(entity)?.target else {
                    return None;
                };
                Some((Key(&binding.source, &binding.uid), binding.area))
            }
            Self::Surface(entity) => {
                let surface = world.get::<Surface>(entity)?;
                Some((
                    Key(&surface.source, surface.record.as_deref()?),
                    Entity::PLACEHOLDER,
                ))
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Demands(pub Vec<Owner>);

pub(super) fn release_obsolete(
    world: &mut World,
    feeds: &mut Feeds,
    demands: &Demands,
) -> Vec<Owner> {
    let desired = {
        let _preparation = nucleus::performance::Span::new("shader-feed-demand-preparation");
        let mut desired = HashMap::with_capacity(feeds.0.len().min(demands.0.len()));
        for &owner in &demands.0 {
            if let Some((key, _)) = owner.binding(world) {
                desired.insert(key, owner);
            }
        }
        let remaining = 32usize.saturating_sub(feeds.1.len());
        for (_, feed) in feeds
            .0
            .extract_if(|key, _| !desired.contains_key(&Key(&key.0, &key.1)))
            .take(remaining)
        {
            feeds.1.push((feed.binding, feed.id));
        }
        desired.into_values().collect()
    };
    feeds.1.retain(|(binding, id)| {
        if let Some(sender) = sender(world, binding) {
            match sender.try_reserve() {
                Ok(permit) => {
                    crate::native::protein_area::send_auxiliary(
                        world,
                        &binding.source,
                        permit,
                        ClientMessage::Unsubscribe { id: id.clone() },
                    );
                    return false;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => return true,
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
            }
        }
        crate::native::protein_area::auxiliary_subscribed(
            world,
            &binding.source,
            &ClientMessage::Unsubscribe { id: id.clone() },
        );
        false
    });
    desired
}

pub(super) fn maintain(world: &mut World, demands: &Demands) {
    world.init_resource::<Feeds>();
    if demands.0.is_empty()
        && world.resource::<Feeds>().0.is_empty()
        && world.resource::<Feeds>().1.is_empty()
    {
        return;
    }
    let _maintenance = nucleus::performance::Span::new("shader-feed-maintenance");
    let mut feeds = world.remove_resource::<Feeds>().unwrap();
    let desired = super::release_obsolete_feeds(world, &mut feeds, demands);
    let retiring = !feeds.1.is_empty();
    for owner in desired {
        let Some((key, area)) = owner.binding(world) else {
            continue;
        };
        if feeds.0.len() >= 64 && !feeds.0.contains_key(&key) {
            continue;
        }
        let feed = feeds.0.entry_ref(&key).or_insert_with(|| {
            nucleus::performance::add("shader-record-feeds-created", 1);
            Feed {
                binding: RecordBinding {
                    area,
                    uid: key.1.to_owned(),
                    source: key.0.clone(),
                },
                id: nucleus::new_uid("shader-feed"),
                sent: false,
                row: None,
                error: String::new(),
                retry: Instant::now(),
            }
        });
        crate::native::protein_area::ensure_auxiliary(world, &feed.binding.source);
        if !retiring
            && !feed.sent
            && feed.retry <= Instant::now()
            && crate::native::laboratory::normal(world)
            && let Some(sender) = sender(world, &feed.binding)
        {
            if let Ok(permit) = sender.try_reserve() {
                let message = ClientMessage::Subscribe {
                    id: feed.id.clone(),
                    protein: protein::Protein {
                        source: protein::Source::Record,
                        filter: vec![protein::Predicate::UidEq(feed.binding.uid.clone())],
                        fields: Some(
                            ["uid", "quantity", "shader_effect"]
                                .map(str::to_owned)
                                .into(),
                        ),
                        include: protein::Include {
                            shader_effect: true,
                            ..default()
                        },
                        aggregate: None,
                        order: vec![],
                        limit: Some(1),
                    },
                };
                crate::native::protein_area::send_auxiliary(
                    world,
                    &feed.binding.source,
                    permit,
                    message,
                );
                feed.sent = true;
                nucleus::performance::add("shader-record-subscriptions-prepared", 1);
            }
            feed.retry = Instant::now() + Duration::from_secs(3);
        }
    }
    world.insert_resource(feeds);
}

struct CheckedRow {
    row: Arc<EffectRecord>,
    bounded: std::sync::OnceLock<bool>,
    current: bool,
}

#[derive(Resource, Default)]
pub(super) struct CheckedRows(HashMap<usize, CheckedRow>);

fn row_key(row: &Arc<EffectRecord>) -> usize {
    Arc::as_ptr(row) as usize
}

fn bounded(extension: &Extension) -> bool {
    let _validation = nucleus::performance::Span::new("shader-feed-catalog-validation");
    nucleus::performance::add("shader-feed-catalog-validations", 1);
    extension.bounded()
}

impl CheckedRows {
    pub(super) fn extension<'a>(&self, row: &'a Arc<EffectRecord>) -> Option<&'a Extension> {
        let extension = row.extension.as_ref()?;
        self.0
            .get(&row_key(row))
            .filter(|checked| Arc::ptr_eq(&checked.row, row))
            .map_or_else(
                || bounded(extension),
                |checked| *checked.bounded.get_or_init(|| bounded(extension)),
            )
            .then_some(extension)
    }
}

pub(super) fn prepare_rows(world: &mut World) {
    world.init_resource::<CheckedRows>();
    world
        .run_system_cached(check_rows)
        .expect("prepare current shader Record catalogs");
}

fn check_rows(feeds: Option<Res<Feeds>>, mut checked: ResMut<CheckedRows>) {
    let Some(feeds) = feeds else {
        if !checked.0.is_empty() {
            checked.0 = HashMap::new();
        }
        return;
    };
    if !feeds.is_changed() && !checked.is_added() {
        return;
    }
    let _preparation = nucleus::performance::Span::new("shader-feed-catalog-preparation");
    for row in checked.0.values_mut() {
        row.current = false;
    }
    for row in feeds.0.values().filter_map(|feed| feed.row.as_ref()) {
        checked
            .0
            .entry(row_key(row))
            .or_insert_with(|| CheckedRow {
                row: row.clone(),
                bounded: std::sync::OnceLock::new(),
                current: true,
            })
            .current = true;
    }
    checked.0.retain(|_, row| row.current);
    if checked.0.is_empty() {
        checked.0 = HashMap::new();
    }
}

pub(super) fn default_extension() -> &'static Extension {
    static DEFAULT: std::sync::OnceLock<Extension> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| Extension::single(0, Effect::default()))
}

pub(super) fn unavailable_surface() -> &'static Effect {
    static DEFAULT: std::sync::OnceLock<Effect> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(|| preset(model::GRADIENT))
}
