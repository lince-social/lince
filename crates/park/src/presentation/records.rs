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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub uid: Option<String>,
    pub layout: native::layout::LayoutBox,
    pub workspace: u64,
    pub position: [f64; 2],
    pub size: [f32; 2],
    pub tokens: native::tokens::TokenOverrides,
    pub launch: Option<crate::launch::Description>,
}

impl Saved {
    fn valid(&self) -> bool {
        self.uid
            .as_ref()
            .is_some_and(|uid| nucleus::valid_uid(uid, "r"))
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
    requests: HashMap<String, PlacementId>,
    errors: BTreeMap<PlacementId, String>,
    subscribed: BTreeSet<String>,
    rows: HashMap<String, serde_json::Value>,
    cursor: MessageCursor<native::cell_bridge::CellMessage>,
    dirty: bool,
    last_write: Instant,
    wake_pending: bool,
    inspect: Option<PlacementId>,
}

impl Records {
    pub fn load(path: PathBuf) -> io::Result<Self> {
        let records = read(&path)?;
        Ok(Self {
            path,
            saved: records
                .into_iter()
                .map(|record| (PlacementId(record.layout.id), record))
                .collect(),
            requests: HashMap::new(),
            errors: BTreeMap::new(),
            subscribed: BTreeSet::new(),
            rows: HashMap::new(),
            cursor: Default::default(),
            dirty: false,
            last_write: Instant::now(),
            wake_pending: false,
            inspect: None,
        })
    }

    pub fn profile(&self, identity: PlacementId) -> Option<&Saved> {
        self.saved.get(&identity)
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
        let saved = Saved {
            uid: previous.and_then(|record| record.uid.clone()),
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
        let messages: Vec<_> = self
            .cursor
            .read(world.resource::<Messages<native::cell_bridge::CellMessage>>())
            .filter(|message| match &message.0 {
                cell::ServerMessage::ActionOk { id, .. }
                | cell::ServerMessage::Error { id, .. } => {
                    self.requests.contains_key(id) || id == SUBSCRIPTION
                }
                cell::ServerMessage::Snapshot { id, .. }
                | cell::ServerMessage::Update { id, .. } => id == SUBSCRIPTION,
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
                        && nucleus::valid_uid(&uid, "r")
                        && let Some(saved) = self.saved.get_mut(&identity)
                    {
                        saved.uid = Some(uid);
                        self.dirty = true;
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
        let sender = world
            .get_non_send::<native::cell_bridge::CellBridge>()
            .map(|bridge| bridge.outgoing.clone());
        let active: BTreeSet<_> = identities.values().copied().collect();
        let pending: BTreeSet<_> = self.requests.values().copied().collect();
        self.saved.retain(|identity, saved| {
            saved.uid.is_some() || active.contains(identity) || pending.contains(identity)
        });
        self.errors
            .retain(|identity, _| self.saved.contains_key(identity));
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
            } else if !self.errors.contains_key(&identity) {
                let request = format!("park-new-window-{:032x}", u128::from_be_bytes(identity.0));
                if self.requests.contains_key(&request) {
                    continue;
                }
                if self.saved.len() > 65536 {
                    self.errors
                        .insert(identity, "Window customization history is full".into());
                    continue;
                }
                let message = cell::ClientMessage::Act {
                    id: request.clone(),
                    action: engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: "Application window".into(),
                        body: String::new(),
                        quantity: 0.0,
                    },
                };
                if sender
                    .as_ref()
                    .is_some_and(|sender| sender.try_send(message).is_ok())
                {
                    self.requests.insert(request, identity);
                } else {
                    waiting = true;
                }
            }
        }
        if live != self.subscribed {
            let message = if live.is_empty() {
                Some(cell::ClientMessage::Unsubscribe {
                    id: SUBSCRIPTION.into(),
                })
            } else {
                let mut draft = native::protein_castle::ProteinDraft::default();
                draft.query["where"] = serde_json::json!([{"any":live.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>() }]);
                draft.query["limit"] = serde_json::json!(512);
                draft
                    .compile()
                    .ok()
                    .map(|protein| cell::ClientMessage::Subscribe {
                        id: SUBSCRIPTION.into(),
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
                Duration::from_millis(50)
            } else {
                Duration::from_secs(1).saturating_sub(self.last_write.elapsed())
            });
            self.wake_pending = true;
        }
        if waiting {
            self.wake_pending = false;
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let records: Vec<_> = self
            .saved
            .values()
            .filter(|record| record.uid.is_some())
            .collect();
        if records.iter().any(|record| !record.valid()) {
            return Err(io::Error::other("invalid window customization data"));
        }
        super::outputs::save(&self.path, &records)?;
        self.last_write = Instant::now();
        self.dirty = false;
        Ok(())
    }

    pub fn pending(&self) -> bool { !self.requests.is_empty() }

    pub fn inspect(&mut self, identity: PlacementId) {
        self.errors.remove(&identity);
        self.inspect = Some(identity);
    }

    pub fn take_inspection(&mut self) -> Option<String> {
        let uid = self.saved.get(&self.inspect?)?.uid.clone()?;
        self.inspect = None;
        Some(uid)
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

    pub(super) fn inspect_window_record(&mut self) {
        let surface = self.application_focus.or_else(|| {
            self.last_geometry.as_ref()?.focused.and_then(|item| {
                if let lince_interface::host::ItemId::Application(surface) = item {
                    Some(surface)
                } else {
                    None
                }
            })
        });
        if let Some(identity) = surface
            .and_then(|surface| self.identities.get(&surface))
            .copied()
            && let Some(records) = &mut self.records
        {
            records.inspect(identity);
        } else {
            native::notifications::report(
                self.app.world(),
                "Park",
                "Focus an application connected to this Cell to inspect its Record.",
            );
        }
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

    fn window(world: &mut World, root: Entity) -> (SurfaceId, clients::Window, PlacementId) {
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

    async fn pump(
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
