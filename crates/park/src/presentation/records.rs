use super::*;
use bevy::ecs::message::MessageCursor;
use lince_interface::{host::SurfaceId, layout::desktop::PlacementId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{self, Read},
    path::Path,
    time::{Duration, Instant},
};

const SUBSCRIPTION: &str = "park-window-records";

mod inspection;
use inspection::Inspection;

#[cfg(test)]
#[path = "records/recovery.rs"]
mod recovery;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation: Option<String>,
    pub layout: native::layout::LayoutBox,
    pub workspace: u64,
    pub position: [f64; 2],
    pub size: [f32; 2],
    pub tokens: native::tokens::TokenOverrides,
    pub launch: Option<crate::launch::Description>,
}

impl Saved {
    fn valid(&self) -> bool {
        let uid = match (&self.uid, &self.creation) {
            (Some(uid), None) | (None, Some(uid)) => uid,
            _ => return false,
        };
        nucleus::valid_uid(uid, "r")
            && self.layout.valid()
            && self.workspace > 0
            && self.position.iter().all(|value| value.is_finite())
            && self
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
            && self.tokens.validate()
            && self
                .launch
                .as_ref()
                .is_none_or(crate::launch::Description::valid)
    }
}

pub(super) struct Records {
    path: PathBuf,
    saved: BTreeMap<PlacementId, Saved>,
    creations: BTreeSet<PlacementId>,
    requests: HashMap<String, PlacementId>,
    errors: BTreeMap<PlacementId, String>,
    subscribed: BTreeSet<String>,
    rows: HashMap<String, serde_json::Value>,
    cursor: MessageCursor<native::cell_bridge::CellMessage>,
    dirty: bool,
    last_write: Instant,
    wake_pending: bool,
    inspect: Option<Inspection>,
    sender: Option<tokio::sync::mpsc::Sender<cell::ClientMessage>>,
    subscription: String,
    intents_durable: bool,
    intent_retry: Option<Instant>,
}

impl Records {
    pub fn load(path: PathBuf) -> io::Result<Self> {
        let records = read(&path)?;
        let creations = records.iter().filter(|record| record.creation.is_some()).map(|record| PlacementId(record.layout.id)).collect();
        Ok(Self {
            path,
            saved: records
                .into_iter()
                .map(|record| (PlacementId(record.layout.id), record))
                .collect(),
            creations,
            requests: HashMap::new(),
            errors: BTreeMap::new(),
            subscribed: BTreeSet::new(),
            rows: HashMap::new(),
            cursor: Default::default(),
            dirty: false,
            last_write: Instant::now(),
            wake_pending: false,
            inspect: None,
            sender: None,
            subscription: format!("{SUBSCRIPTION}-{}", nucleus::new_uid("op")),
            intents_durable: true,
            intent_retry: None,
        })
    }

    pub fn profile(&self, identity: PlacementId) -> Option<&Saved> {
        self.saved.get(&identity)
    }

    pub(super) fn capture_metadata_excluded(&self, identity: PlacementId, uid: &str) -> bool {
        self.profile(identity)
            .filter(|profile| profile.uid.as_deref() == Some(uid) || profile.creation.as_deref() == Some(uid))
            .is_none_or(|profile| profile.layout.capture_excluded)
    }

    pub(super) fn capture_exclusion(&mut self, identity: PlacementId, uid: &str, excluded: bool) -> io::Result<()> {
        let profile = self.saved.get_mut(&identity).filter(|profile| profile.uid.as_deref() == Some(uid))
            .ok_or_else(|| io::Error::other("The original window profile association is unavailable"))?;
        let previous = profile.layout.capture_excluded;
        let dirty = self.dirty;
        profile.layout.capture_excluded = excluded;
        self.dirty = true;
        if let Err(error) = self.flush() {
            self.saved.get_mut(&identity).unwrap().layout.capture_excluded = previous;
            self.dirty = dirty;
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn opening_references(&self) -> Vec<String> {
        self.saved.values().filter_map(|saved| saved.uid.clone()).take(512).collect()
    }

    pub fn capture(&mut self, world: &World, window: &clients::Window, identity: PlacementId) {
        if window.parent.is_some() {
            return;
        }
        let Some(layout) = world
            .get::<native::layout::LayoutBox>(window.entity)
            .copied()
        else {
            return;
        };
        let Some(item) = world.get::<native::canvas::CanvasItem>(window.entity) else {
            return;
        };
        let Some(member) = world.get::<native::workspace::WorkspaceMember>(window.entity) else {
            return;
        };
        let previous = self.saved.get(&identity);
        if previous.is_none() && self.saved.len() >= 65536 {
            if self.errors.insert(identity, "Window customization history is full".into()).is_none() {
                native::notifications::report(world, "Park window Record", "Window customization history is full");
            }
            return;
        }
        let creation = previous.map_or_else(|| Some(nucleus::new_uid("r")), |saved| saved.creation.clone());
        if previous.is_none() {
            self.intents_durable = false;
            self.creations.insert(identity);
        }
        let saved = Saved {
            uid: previous.and_then(|record| record.uid.clone()),
            creation,
            launch: previous.and_then(|record| record.launch.clone()),
            layout,
            workspace: member.0,
            position: item.position.to_array(),
            size: item.size.to_array(),
            tokens: world
                .get::<native::tokens::TokenOverrides>(window.entity)
                .cloned()
                .unwrap_or_default(),
        };
        if previous != Some(&saved) {
            self.saved.insert(identity, saved);
            self.dirty = true;
        }
    }

    pub fn tick(
        &mut self,
        world: &mut World,
        windows: &HashMap<SurfaceId, clients::Window>,
        identities: &HashMap<SurfaceId, PlacementId>,
    ) {
        let sender = world.get_non_send::<native::cell_bridge::CellBridge>().map(|bridge| bridge.outgoing.clone());
        let changed = match (&self.sender, &sender) {
            (Some(previous), Some(current)) => !previous.same_channel(current),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.sender = sender.clone();
            self.subscription = format!("{SUBSCRIPTION}-{}", nucleus::new_uid("op"));
            self.requests.clear();
            self.subscribed.clear();
            self.errors.clear();
        }
        let messages: Vec<_> = self
            .cursor
            .read(world.resource::<Messages<native::cell_bridge::CellMessage>>())
            .filter(|message| match &message.0 {
                cell::ServerMessage::ActionOk { id, .. }
                | cell::ServerMessage::Error { id, .. } => {
                    self.requests.contains_key(id) || id == &self.subscription
                }
                cell::ServerMessage::Snapshot { id, .. }
                | cell::ServerMessage::Update { id, .. } => id == &self.subscription,
                _ => false,
            })
            .cloned()
            .collect();
        for message in messages {
            match message.0 {
                cell::ServerMessage::ActionOk {
                    id,
                    created: Some(uid),
                    ..
                } => {
                    if let Some(identity) = self.requests.remove(&id)
                        && let Some(saved) = self.saved.get_mut(&identity)
                    {
                        if saved.creation.as_deref() == Some(uid.as_str()) && nucleus::valid_uid(&uid, "r") {
                            saved.uid = Some(uid);
                            saved.creation = None;
                            self.creations.remove(&identity);
                            self.dirty = true;
                        } else {
                            self.errors.insert(identity, "Record creation returned an unexpected identity".into());
                            native::notifications::report(world, "Park window Record", "Record creation returned an unexpected identity");
                        }
                    }
                }
                cell::ServerMessage::ActionOk {
                    id, created: None, ..
                } => {
                    if let Some(identity) = self.requests.remove(&id) {
                        self.errors.insert(
                            identity,
                            "Record creation did not return an identity".into(),
                        );
                    }
                }
                cell::ServerMessage::Error { id, message, .. } => {
                    if let Some(identity) = self.requests.remove(&id) {
                        self.errors.insert(identity, message.clone());
                    }
                    native::notifications::report(world, "Park window Record", &message);
                }
                cell::ServerMessage::Snapshot { rows, .. }
                | cell::ServerMessage::Update { rows, .. } => {
                    self.rows = rows
                        .into_iter()
                        .filter_map(|row| Some((row["uid"].as_str()?.to_owned(), row)))
                        .collect();
                }
                _ => {}
            }
        }
        let active: BTreeSet<_> = identities.values().copied().collect();
        self.errors
            .retain(|identity, _| self.saved.contains_key(identity) || active.contains(identity));
        let mut live = BTreeSet::new();
        let mut waiting = false;
        for (surface, window) in windows {
            if window.parent.is_some() {
                continue;
            }
            let Some(identity) = identities.get(surface).copied() else {
                continue;
            };
            self.capture(world, window, identity);
            let Some(saved) = self.saved.get(&identity) else {
                continue;
            };
            if let Some(uid) = &saved.uid {
                let uid = uid.clone();
                live.insert(uid.clone());
                let old = world
                    .get::<native::area::RecordProperties>(window.entity)
                    .map(|properties| properties.0.clone())
                    .unwrap_or_default();
                let mut properties = self.rows.get(&uid).cloned().unwrap_or_else(|| serde_json::json!({"uid":uid,"head":"Application window","kind":"plain","quantity":0}));
                properties["window_title"] = old
                    .get("window_title")
                    .cloned()
                    .or_else(|| old.get("head").cloned())
                    .unwrap_or_default();
                properties["app_id"] = old.get("app_id").cloned().unwrap_or_default();
                if old != properties {
                    world
                        .entity_mut(window.entity)
                        .insert(native::area::RecordProperties(properties));
                }
                if world
                    .get::<native::protein_area::RecordBinding>(window.entity)
                    .is_none_or(|binding| binding.uid != uid)
                {
                    world
                        .entity_mut(window.entity)
                        .insert(native::protein_area::RecordBinding {
                            area: window.entity,
                            uid,
                            source: native::protein_area::Source::Local,
                        });
                }
            }
        }
        waiting |= self.create_pending(world, sender.as_ref());
        if live != self.subscribed {
            let message = if live.is_empty() {
                Some(cell::ClientMessage::Unsubscribe {
                    id: self.subscription.clone(),
                })
            } else {
                let mut draft = native::protein_castle::ProteinDraft::default();
                draft.query["where"] = serde_json::json!([{"any":live.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>() }]);
                draft.query["limit"] = serde_json::json!(512);
                draft
                    .compile()
                    .ok()
                    .map(|protein| cell::ClientMessage::Subscribe {
                        id: self.subscription.clone(),
                        protein,
                    })
            };
            if let Some(message) = message
                && sender
                    .as_ref()
                    .is_some_and(|sender| sender.try_send(message).is_ok())
            {
                self.subscribed = live;
            } else {
                waiting = true;
            }
        }
        if self.dirty && self.last_write.elapsed() >= Duration::from_secs(1) {
            self.last_write = Instant::now();
            self.wake_pending = false;
            if let Err(error) = self.flush() {
                native::notifications::report(
                    world,
                    "Park window customization",
                    &error.to_string(),
                );
            }
        }
        if (self.dirty || waiting)
            && !self.wake_pending
            && let Some(wake) = world.get_resource::<WakeSignal>()
        {
            wake.after(if waiting {
                self.intent_retry.map_or(Duration::from_millis(50), |due| due.saturating_duration_since(Instant::now()))
            } else {
                Duration::from_secs(1).saturating_sub(self.last_write.elapsed())
            });
            self.wake_pending = true;
        }
        if waiting {
            self.wake_pending = false;
        }
    }

    fn create_pending(&mut self, world: &World, sender: Option<&tokio::sync::mpsc::Sender<cell::ClientMessage>>) -> bool {
        if !self.intents_durable {
            if self.intent_retry.is_some_and(|due| due > Instant::now()) { return true; }
            if let Err(error) = self.flush() {
                self.last_write = Instant::now();
                self.intent_retry = Some(self.last_write + Duration::from_secs(1));
                native::notifications::report(world, "Park window Record", &format!("Record creation is waiting for its window profile to be saved: {error}"));
                return true;
            }
        }
        let pending: BTreeSet<_> = self.requests.values().copied().collect();
        for identity in &self.creations {
            let Some(saved) = self.saved.get(identity) else { continue; };
            let Some(uid) = &saved.creation else { continue; };
            if pending.contains(identity) || self.errors.contains_key(identity) { continue; }
            if self.requests.len() >= 64 { return true; }
            let request = format!("{}-create-{:032x}-{}", self.subscription, u128::from_be_bytes(identity.0), nucleus::new_uid("op"));
            let message = cell::ClientMessage::Act {
                id: request.clone(),
                action: engine::actions::Action::CreateRecordDraft {
                    draft: engine::record_creation::Draft {
                        uid: uid.clone(), head: "Application window".into(), ..Default::default()
                    },
                },
            };
            if sender.is_none_or(|sender| sender.try_send(message).is_err()) { return true; }
            self.requests.insert(request, *identity);
        }
        false
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let records: Vec<_> = self
            .saved
            .values()
            .collect();
        if records.iter().any(|record| !record.valid()) {
            return Err(io::Error::other("invalid window customization data"));
        }
        super::outputs::save(&self.path, &records)?;
        self.last_write = Instant::now();
        self.dirty = false;
        self.intents_durable = true;
        self.intent_retry = None;
        Ok(())
    }

    pub fn pending(&self) -> bool { !self.requests.is_empty() || !self.creations.is_empty() }

    pub fn retry(&mut self, identity: PlacementId) {
        self.errors.remove(&identity);
        self.intent_retry = None;
    }

    pub fn inspect(&mut self, target: lince_interface::host::WindowInspection, title: String, app_id: String) -> Result<(), &'static str> {
        let identity = PlacementId(target.placement);
        let saved = self.saved.get(&identity).ok_or("This window does not have its own saved Record association.")?;
        let uid = saved.uid.as_ref().or(saved.creation.as_ref())
            .filter(|uid| nucleus::valid_uid(uid, "r"))
            .cloned().ok_or("The window Record identity is unavailable.")?;
        self.retry(identity);
        self.inspect = Some(Inspection { target, uid, title, app_id });
        Ok(())
    }

    pub fn take_inspection(&mut self) -> Option<Inspection> {
        let pending = self.inspect.as_ref()?;
        let profile = self.saved.get(&PlacementId(pending.target.placement))?;
        let uid = profile.uid.as_ref().or(profile.creation.as_ref())?;
        if uid != &pending.uid { return None; }
        self.inspect.take()
    }

    pub fn launch(
        &mut self,
        identity: PlacementId,
        launch: crate::launch::Description,
    ) -> io::Result<()> {
        if !launch.valid() {
            return Err(io::Error::other("invalid window launch description"));
        }
        let saved = self
            .saved
            .get_mut(&identity)
            .ok_or_else(|| io::Error::other("the window Record is unavailable"))?;
        saved.launch = Some(launch);
        self.dirty = true;
        self.flush()
    }
}

impl Presentation {
    pub fn window_launch_description(
        &mut self,
        surface: SurfaceId,
        launch: crate::launch::Description,
    ) -> io::Result<()> {
        let identity = self
            .identities
            .get(&surface)
            .copied()
            .ok_or_else(|| io::Error::other("the window is unavailable"))?;
        self.records
            .as_mut()
            .ok_or_else(|| io::Error::other("Park is not connected to a Cell"))?
            .launch(identity, launch)
    }

}

impl Drop for Records {
    fn drop(&mut self) {
        if let Err(error) = self.flush() {
            eprintln!("Could not save Park window customizations: {error}");
        }
    }
}

fn read(path: &Path) -> io::Result<Vec<Saved>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(io::Error::other(
            "window customization data exceeds its limit",
        ));
    }
    let records: Vec<Saved> = serde_json::from_slice(&bytes)?;
    let mut identities = BTreeSet::new();
    if records.len() > 65536
        || records
            .iter()
            .any(|record| !record.valid() || !identities.insert(record.layout.id))
    {
        return Err(io::Error::other("invalid window customization data"));
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_exclusion_saves_only_the_captured_profile_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park-windows.json");
        let mut records = Records::load(path.clone()).unwrap();
        let mut identities = Vec::new();
        for _ in 0..2 {
            let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
            let uid = nucleus::new_uid("r");
            let identity = PlacementId(layout.id);
            records.saved.insert(identity, Saved { uid: Some(uid.clone()), creation: None, layout, workspace: 1, position: [0.0; 2], size: [300.0; 2], tokens: Default::default(), launch: None });
            identities.push((identity, uid));
        }
        records.dirty = true;
        records.flush().unwrap();
        assert!(records.capture_exclusion(identities[0].0, &identities[1].1, true).is_err());
        records.capture_exclusion(identities[0].0, &identities[0].1, true).unwrap();
        drop(records);
        let records = Records::load(path).unwrap();
        assert!(records.profile(identities[0].0).unwrap().layout.capture_excluded);
        assert!(!records.profile(identities[1].0).unwrap().layout.capture_excluded);
        assert!(records.profile(identities[0].0).unwrap().launch.is_none());
    }

    #[test]
    fn failed_exclusion_write_retains_the_original_profile_for_a_real_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profile");
        let mut records = Records::load(path.clone()).unwrap();
        let layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
        let identity = PlacementId(layout.id);
        let uid = nucleus::new_uid("r");
        records.saved.insert(identity, Saved { uid: Some(uid.clone()), creation: None, layout, workspace: 1, position: [0.0; 2], size: [300.0; 2], tokens: Default::default(), launch: None });
        std::fs::create_dir(&path).unwrap();
        assert!(records.capture_exclusion(identity, &uid, true).is_err());
        assert!(!records.profile(identity).unwrap().layout.capture_excluded);
        std::fs::remove_dir(&path).unwrap();
        records.capture_exclusion(identity, &uid, true).unwrap();
        assert!(Records::load(path).unwrap().profile(identity).unwrap().layout.capture_excluded);
    }

    #[test]
    fn retained_metadata_follows_its_saved_source_after_close_and_refuses_replaced_associations() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park-windows.json");
        let mut records = Records::load(path.clone()).unwrap();
        let mut layout = native::layout::LayoutBox::new(Vec2::splat(300.0));
        layout.capture_excluded = true;
        let identity = PlacementId(layout.id);
        let uid = nucleus::new_uid("r");
        let other = nucleus::new_uid("r");
        records.saved.insert(identity, Saved { uid: Some(uid.clone()), creation: None, layout, workspace: 1, position: [0.0; 2], size: [300.0; 2], tokens: Default::default(), launch: None });
        records.dirty = true;
        records.flush().unwrap();
        drop(records);
        let mut records = Records::load(path).unwrap();
        assert!(records.capture_metadata_excluded(identity, &uid));
        records.capture_exclusion(identity, &uid, false).unwrap();
        assert!(!records.capture_metadata_excluded(identity, &uid));
        assert!(records.capture_metadata_excluded(identity, &other));
        records.saved.get_mut(&identity).unwrap().uid = Some(other.clone());
        assert!(records.capture_metadata_excluded(identity, &uid));
        assert!(!records.capture_metadata_excluded(identity, &other));
        let profile = records.saved.get_mut(&identity).unwrap();
        profile.uid = None;
        profile.creation = Some(other.clone());
        assert!(records.capture_metadata_excluded(identity, &uid));
        assert!(!records.capture_metadata_excluded(identity, &other));
        records.saved.remove(&identity);
        assert!(records.capture_metadata_excluded(identity, &other));
    }

    pub(super) fn window(world: &mut World, root: Entity) -> (SurfaceId, clients::Window, PlacementId) {
        let layout = native::layout::LayoutBox::new(Vec2::new(600.0, 400.0));
        let mut tokens = native::tokens::TokenOverrides::default();
        assert!(tokens.set(
            native::tokens::Token::Accent,
            native::tokens::TokenValue::Color([10, 20, 30, 255])
        ));
        let entity = world.spawn((layout, tokens, native::canvas::CanvasItem { position: bevy::math::DVec2::new(120.0, 80.0), size: Vec2::new(600.0, 400.0) }, native::workspace::WorkspaceMember(1), native::area::RecordProperties(serde_json::json!({"head":"Private browser page title","app_id":"untrusted.example.client"})), ChildOf(root))).id();
        (
            SurfaceId(entity.to_bits()),
            clients::Window::fixture(entity),
            PlacementId(layout.id),
        )
    }

    pub(super) async fn pump(
        world: &mut World,
        records: &mut Records,
        windows: &HashMap<SurfaceId, clients::Window>,
        identities: &HashMap<SurfaceId, PlacementId>,
        done: impl Fn(&Records) -> bool,
    ) {
        for _ in 0..1000 {
            let mut incoming = Vec::new();
            while let Ok(message) = world
                .non_send_mut::<native::cell_bridge::CellBridge>()
                .incoming
                .try_recv()
            {
                incoming.push(native::cell_bridge::CellMessage(message));
            }
            world.write_message_batch(incoming);
            records.tick(world, windows, identities);
            if done(records) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("window Record messages did not settle");
    }

    #[tokio::test]
    async fn window_records_survive_close_without_relaunch_tiles_or_metadata_identity_matching() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            commands: Default::default(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            speech: None,
            fiote: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park-windows.json");
        let mut world = World::new();
        world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
        world.insert_resource(WakeSignal::new(|| {}));
        world.insert_non_send(native::cell_bridge::connect(
            runtime.clone(),
            WakeSignal::new(|| {}),
        ));
        let root = world
            .spawn((
                native::workspace::Workspaces::default(),
                native::canvas::CanvasView::default(),
            ))
            .id();
        let (first, first_window, first_identity) = window(&mut world, root);
        let first_entity = first_window.entity;
        let mut windows = HashMap::from([(first, first_window)]);
        let mut identities = HashMap::from([(first, first_identity)]);
        let mut records = Records::load(path.clone()).unwrap();
        pump(&mut world, &mut records, &windows, &identities, |records| {
            records
                .profile(first_identity)
                .and_then(|profile| profile.uid.as_ref())
                .is_some()
        })
        .await;
        let uid = records
            .profile(first_identity)
            .unwrap()
            .uid
            .clone()
            .unwrap();
        pump(&mut world, &mut records, &windows, &identities, |records| {
            records.rows.contains_key(&uid)
        })
        .await;
        assert_eq!(
            world
                .get::<native::protein_area::RecordBinding>(first_entity)
                .unwrap()
                .uid,
            uid
        );
        assert_eq!(
            world
                .get::<native::area::RecordProperties>(first_entity)
                .unwrap()
                .0["head"],
            "Application window"
        );
        assert_eq!(
            world
                .get::<native::area::RecordProperties>(first_entity)
                .unwrap()
                .0["window_title"],
            "Private browser page title"
        );
        assert!(
            world
                .get::<native::workspace::RecordPlacement>(first_entity)
                .is_none()
        );
        records.capture(&world, &windows[&first], first_identity);
        windows.remove(&first);
        identities.remove(&first);
        world.despawn(first_entity);
        records.tick(&mut world, &windows, &identities);
        records.flush().unwrap();
        let document = std::fs::read_to_string(&path).unwrap();
        assert!(!document.contains("Private browser page title"));
        assert!(!document.contains("untrusted.example.client"));
        let count = world.entities().len();
        let restored = Records::load(path).unwrap();
        assert_eq!(world.entities().len(), count);
        assert_eq!(
            restored.profile(first_identity).unwrap().uid.as_deref(),
            Some(uid.as_str())
        );
        assert_eq!(
            restored.profile(first_identity).unwrap().position,
            [120.0, 80.0]
        );
        assert_eq!(
            restored.profile(first_identity).unwrap().tokens.0[&native::tokens::Token::Accent],
            native::tokens::TokenValue::Color([10, 20, 30, 255])
        );
        let query = native::full_record::config(&uid, native::protein_area::Source::Local)
            .draft
            .compile()
            .unwrap();
        let mut session = runtime.local_session();
        let response = session
            .handle(cell::ClientMessage::Subscribe {
                id: "verify-retained-record".into(),
                protein: query,
            })
            .await;
        assert!(response.iter().any(|message| matches!(message, cell::ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == uid))));
        let (second, second_window, second_identity) = window(&mut world, root);
        assert_ne!(first_identity, second_identity);
        windows.insert(second, second_window);
        identities.insert(second, second_identity);
        pump(&mut world, &mut records, &windows, &identities, |records| {
            records
                .profile(second_identity)
                .and_then(|profile| profile.uid.as_ref())
                .is_some()
        })
        .await;
        assert_ne!(
            records.profile(second_identity).unwrap().uid.as_deref(),
            Some(uid.as_str())
        );
        assert_eq!(
            records.profile(first_identity).unwrap().uid.as_deref(),
            Some(uid.as_str())
        );
    }
}
