use super::*;
use std::collections::BTreeSet;

fn prepare(world: &mut World, owners: &[Entity], runtime: &mut Runtime, now: Instant) -> bool {
    let _preparation = nucleus::performance::Span::new("extension-feed-preparation");
    let mut groups: HashMap<Arc<FeedKey>, Vec<Entity>> = HashMap::new();
    for &owner in owners {
        let settings = world.get::<Settings>(owner).unwrap();
        let key = world
            .get::<View>(owner)
            .and_then(|view| view.feed.as_ref())
            .filter(|key| key.matches(settings))
            .cloned()
            .unwrap_or_else(|| key(settings));
        groups.entry(key.clone()).or_default().push(owner);
        if runtime.feeds.len() < 256 || runtime.feeds.contains_key(&key) {
            runtime.feeds.entry(key.clone()).or_insert_with(|| Feed {
                schemas: vec![],
                data: None,
                error: String::new(),
                version: 1,
                next: now,
                pending: None,
            });
            let mut view = world.get_mut::<View>(owner).unwrap();
            if view.feed.as_ref() != Some(&key) {
                view.feed = Some(key);
                view.seen = 0;
                view.render = true;
            }
        }
    }
    runtime.feeds.retain(|key, _| groups.contains_key(key));
    for (key, feed) in &mut runtime.feeds {
        let schemas: BTreeSet<_> = groups[key]
            .iter()
            .filter_map(|owner| world.get::<Settings>(*owner)?.schema.as_deref())
            .collect();
        if !feed
            .schemas
            .iter()
            .map(String::as_str)
            .eq(schemas.iter().copied())
        {
            feed.schemas = schemas.into_iter().map(str::to_owned).collect();
            feed.next = now;
            nucleus::performance::add("extension-feed-schema-lists-updated", 1);
        }
    }
    !groups.is_empty()
}

pub(super) fn update(world: &mut World, owners: &[Entity]) {
    let now = Instant::now();
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    let active = prepare(world, owners, &mut runtime, now);
    let expired = runtime
        .pending
        .iter()
        .filter(|(_, pending)| now.duration_since(pending.started) > Duration::from_secs(20))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for id in expired {
        if let Some(pending) = runtime.pending.remove(&id) {
            match pending.kind {
                PendingKind::Picker(owner) => {
                    if let Some(sender) = sender(world, &pending.source) {
                        let _ = sender.try_send(ClientMessage::Unsubscribe { id: id.clone() });
                    }
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        if !view
                            .picker
                            .as_ref()
                            .is_some_and(|picker| picker.request.as_ref() == Some(&id))
                        {
                            continue;
                        }
                        view.message = "Picker timed out. Search again.".into();
                        view.render = true;
                    }
                }
                PendingKind::Inspect(key) => {
                    if let Some(feed) = runtime.feeds.get_mut(&key) {
                        feed.pending = None;
                        feed.data = None;
                        feed.error = "Loading timed out; retrying".into();
                        feed.version += 1;
                        feed.next = now + Duration::from_secs(3);
                    }
                }
                PendingKind::Change(owner) => {
                    if let Some(mut view) = world.get_mut::<View>(owner) {
                        view.pending = None;
                        view.message = "No confirmation received. Retry this same change to confirm its result.".into();
                        view.render = true;
                    }
                }
            }
        }
    }
    if crate::native::laboratory::normal(world) {
        for (key, feed) in &mut runtime.feeds {
            if feed.pending.is_some() || feed.next > now || runtime.pending.len() >= 128 {
                continue;
            }
            if let Some(sender) = sender(world, &key.source) {
                let id = nucleus::new_uid("extension");
                if sender
                    .try_send(ClientMessage::Act {
                        id: id.clone(),
                        action: engine::actions::Action::RecordExtensions {
                            target: key.target.clone(),
                            request: Request::Inspect {
                                catalog: key.catalog,
                                schemas: feed.schemas.clone(),
                            },
                        },
                    })
                    .is_ok()
                {
                    feed.pending = Some(id.clone());
                    runtime.pending.insert(
                        id,
                        Pending {
                            source: key.source.clone(),
                            kind: PendingKind::Inspect(key.clone()),
                            started: now,
                        },
                    );
                }
            } else if feed.data.is_some() || feed.error.is_empty() {
                feed.data = None;
                feed.error = "Connect to the Organ to load extensions".into();
                feed.version += 1;
            }
            feed.next = now + Duration::from_secs(3);
        }
    }
    if active && runtime.wake.is_none_or(|deadline| deadline <= now) {
        let deadline = now + Duration::from_secs(3);
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(Duration::from_secs(3));
        }
        runtime.wake = Some(deadline);
    }
    world.insert_resource(runtime);
}
