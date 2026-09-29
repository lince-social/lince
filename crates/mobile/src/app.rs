use crate::{
    connection::{Connection, Event},
    navigation::{Navigation, Page},
};
use bevy::{prelude::*, text::EditableText, ui_widgets::Activate};
use cell::{ClientMessage, ServerMessage};
use lince_interface::{
    controls,
    theme::{INK, PAPER, PURPLE, Typography},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

#[cfg(test)]
mod tests;

#[derive(Resource)]
pub struct Mobile {
    pub navigation: Navigation,
    pub rows: BTreeMap<String, Vec<Value>>,
    pub drafts: BTreeMap<String, String>,
    pub documents: BTreeMap<String, String>,
    pub status: String,
    pub search: String,
    pub record_pages: Vec<String>,
    pub sort: usize,
    pub negative_only: bool,
    pub view: Option<lince_interface::queries::ProteinDraft>,
    pub view_settings: bool,
    pub picker: Option<crate::picker::Picker>,
    pub attachments: BTreeMap<String, Vec<nucleus::message::MessagePart>>,
    pub thread_pages: BTreeMap<String, Vec<protein::MessageCursor>>,
    pub setup_required: bool,
    pub identity: crate::session::Identity,
    pub karma: Option<lince_interface::karma::Draft>,
    pub frequency: Option<lince_interface::frequency::Draft>,
    pub moving: Option<String>,
    pub kanban_column: usize,
    pub column_settings: bool,
    pub deleting: Option<Intent>,
    pub ready: bool,
    pub dirty: bool,
    pending: BTreeMap<String, Pending>,
    active_record: Option<String>,
    active_topics: BTreeSet<&'static str>,
    directory: PathBuf,
    profile_root: PathBuf,
    pub profiles: crate::profiles::Profiles,
    next_profile: Option<crate::profiles::Profiles>,
    profile_error: Option<String>,
    organ: Option<String>,
    save_error: bool,
    last_saved: BTreeMap<String, String>,
    save_after: std::time::Instant,
    scrolls: BTreeMap<String, Vec2>,
    rendered_page: String,
    outbox: BTreeMap<String, crate::record::Prepared>,
    save_requested: bool,
    next_status: std::time::Instant,
}

struct Pending {
    field: Option<(String, String)>,
    text_snapshot: Option<String>,
    created_record: bool,
    transition: bool,
    scope_changed: bool,
    topic: &'static str,
    form: Option<String>,
    submitted: BTreeMap<String, String>,
    clear_fields: Vec<String>,
    deleted: Option<String>,
    task_concept: bool,
    sent_attachments: Option<(String, Vec<nucleus::message::MessagePart>)>,
    silent: bool,
}

#[derive(Resource, Default)]
struct DraftRecovery(BTreeMap<String, crate::storage::Saved>);

impl Mobile {
    pub fn new(directory: PathBuf) -> Self {
        let (profiles, profile_error) = match crate::profiles::Profiles::read(&directory) {
            Ok(profiles) => (profiles, None),
            Err(error) => (Default::default(), Some(error)),
        };
        let profile_root = directory;
        let directory = profiles.directory(&profile_root);
        Self {
            navigation: Default::default(),
            rows: Default::default(),
            drafts: Default::default(),
            documents: Default::default(),
            status: "Opening this device’s Organ…".into(),
            search: String::new(),
            record_pages: Vec::new(),
            sort: 0,
            negative_only: false,
            view: None,
            view_settings: false,
            picker: None,
            attachments: Default::default(),
            thread_pages: Default::default(),
            setup_required: false,
            identity: Default::default(),
            karma: None,
            frequency: None,
            moving: None,
            kanban_column: 1,
            column_settings: false,
            deleting: None,
            ready: false,
            dirty: true,
            pending: Default::default(),
            active_record: None,
            active_topics: Default::default(),
            directory,
            profile_root,
            profiles,
            next_profile: None,
            profile_error,
            organ: None,
            save_error: false,
            last_saved: Default::default(),
            save_after: std::time::Instant::now(),
            scrolls: Default::default(),
            rendered_page: String::new(),
            outbox: Default::default(),
            save_requested: false,
            next_status: std::time::Instant::now() + std::time::Duration::from_secs(5),
        }
    }

    pub fn draft(&self, scope: &str, field: &str, initial: &str) -> String {
        self.drafts
            .get(&format!("{scope}/{field}"))
            .cloned()
            .unwrap_or_else(|| initial.into())
    }

    pub fn scope_key(&self) -> String {
        format!(
            "{}:{:?}:{:?}",
            self.directory.display(),
            self.organ,
            self.identity
        )
    }

    pub fn request_save(&mut self) {
        self.save_requested = true;
    }
}

#[derive(Component, Clone)]
pub struct Input {
    pub key: String,
    pub initial: String,
    pub title: String,
}

#[derive(Component)]
pub struct Content;

#[derive(Component)]
pub(crate) struct Shell;

#[derive(Component)]
struct Status;

#[derive(Component)]
struct PasswordMask(Entity);

#[derive(Component, Clone)]
pub struct ButtonIntent(pub Intent);

#[derive(Clone)]
pub enum Intent {
    OpenLink(String),
    Nothing,
    KanbanColumn(usize),
    ColumnSettings,
    Menu,
    Back,
    Open(Page),
    Refresh,
    Search,
    Sort,
    ToggleNegative,
    CompleteRecord(String),
    RenameCell(String),
    Discovery(bool),
    SavePeerPort,
    ScanQr,
    FreshProfile,
    SwitchProfile(Option<String>),
    Login,
    Logout,
    EditLog(String, String),
    CancelLog(String),
    RetryDrafts,
    More,
    Previous,
    ViewSettings,
    ToggleViewField(String),
    SaveView,
    LoadView(Option<String>),
    Pick(crate::picker::Picker),
    SearchPicker,
    ClosePicker,
    SelectItem(String),
    AttachFile(String),
    RemoveAttachment(String, usize),
    ReplyTo(String, Option<String>),
    MoreMessages(String),
    LoadImage(String),
    NewerMessages(String),
    LatestMessages(String),
    CreateRecord,
    SaveField(String, String),
    DeleteRecord(String),
    Confirm,
    CancelDelete,
    Act(engine::actions::Action),
    EditKarma(Option<String>),
    SaveKarma,
    EditFrequency(Option<String>),
    SaveFrequency,
    Weekday(nucleus::karma::CivilWeekday),
    Pair,
    JoinOrgan,
    Move(String, usize),
    Ask(engine::actions::Action),
    MoveMenu(String),
    RecordControl(String, crate::pages::RecordControl),
}

#[derive(Resource, Default)]
struct Intents(VecDeque<Intent>);

pub(crate) fn queue_intent(world: &mut World, intent: Intent) {
    world.resource_mut::<Intents>().0.push_back(intent);
}

pub struct MobilePlugin;

impl Plugin for MobilePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(target_os = "android")]
        app.add_observer(crate::android::edit)
            .init_resource::<crate::accessibility::Snapshot>()
            .add_systems(Last, crate::accessibility::publish)
            .add_systems(Update, crate::android::receive);
        app.init_resource::<Intents>()
            .add_observer(activate)
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    receive,
                    keyboard,
                    password_masks,
                    poll_status,
                    crate::images::poll,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (capture, flush_on_suspend, apply, persist, render)
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(bevy::ui::UiSystems::Layout),
            );
    }
}

fn flush_on_suspend(
    mut lifecycle: MessageReader<bevy::window::AppLifecycle>,
    mut exit: MessageReader<AppExit>,
    mut state: ResMut<Mobile>,
) {
    if lifecycle
        .read()
        .any(|event| matches!(event, bevy::window::AppLifecycle::WillSuspend))
        || exit.read().next().is_some()
    {
        state.save_after = std::time::Instant::now();
    }
}

fn setup(world: &mut World) {
    connect(world);
    world.spawn(Camera2d);
    world.spawn((
        Shell,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            ..default()
        },
        BackgroundColor(PAPER),
    ));
}

fn connect(world: &mut World) {
    if let Some(error) = world.resource::<Mobile>().profile_error.clone() {
        world.resource_mut::<Mobile>().status = error;
        return;
    }
    let proxy = world.resource::<bevy::winit::EventLoopProxyWrapper>();
    let proxy = (**proxy).clone();
    let wake = lince_interface::wake::WakeSignal::new(move || {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    });
    #[cfg(target_os = "android")]
    crate::android::wake(wake.clone());
    let directory = world.resource::<Mobile>().directory.clone();
    match Connection::open(directory, wake) {
        Ok(connection) => world.insert_non_send(connection),
        Err(error) => world.resource_mut::<Mobile>().status = error.to_string(),
    }
}

fn activate(event: On<Activate>, buttons: Query<&ButtonIntent>, mut queue: ResMut<Intents>) {
    if let Ok(button) = buttons.get(event.entity) {
        queue.0.push_back(button.0.clone());
    }
}

fn keyboard(keys: Res<ButtonInput<KeyCode>>, mut queue: ResMut<Intents>) {
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::BrowserBack) {
        queue.0.push_back(Intent::Back);
    }
}

fn password_masks(inputs: Query<&EditableText>, mut masks: Query<(&PasswordMask, &mut Text)>) {
    for (mask, mut text) in &mut masks {
        if let Ok(input) = inputs.get(mask.0) {
            text.set_if_neq(Text::new("•".repeat(input.value().chars().count().min(32))));
        }
    }
}

fn poll_status(world: &mut World) {
    let state = world.resource::<Mobile>();
    if !state.ready
        || state.identity == crate::session::Identity::Locked
        || state.navigation.current != Page::Organ
        || !state.pending.is_empty()
        || std::time::Instant::now() < state.next_status
    {
        return;
    }
    if editing(world)
        || world
            .query::<&Window>()
            .iter(world)
            .any(|window| !window.focused)
    {
        return;
    }
    let previous = world.resource::<Mobile>().status.clone();
    world.resource_mut::<Mobile>().next_status =
        std::time::Instant::now() + std::time::Duration::from_secs(5);
    if act(world, engine::actions::Action::RosterStatus, None).is_ok() {
        for pending in world.resource_mut::<Mobile>().pending.values_mut() {
            pending.silent = true;
        }
        world.resource_mut::<Mobile>().status = previous;
    }
}

pub fn capture(world: &mut World) {
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(_, value)| !value.is_composing())
        .map(|(input, value)| {
            (
                input.key.clone(),
                input.initial.clone(),
                value.value().to_string(),
            )
        })
        .collect();
    let mut state = world.resource_mut::<Mobile>();
    for (key, initial, value) in values {
        if value == initial {
            if state.drafts.remove(&key).is_some() {
                state.save_after =
                    std::time::Instant::now() + std::time::Duration::from_millis(400);
            }
        } else {
            if state.drafts.get(&key) != Some(&value) {
                state.drafts.insert(key, value);
                state.save_after =
                    std::time::Instant::now() + std::time::Duration::from_millis(400);
            }
        }
    }
}

fn editing(world: &mut World) -> bool {
    world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .any(|(input, text)| text.is_composing() || text.value() != input.initial.as_str())
}

fn composing(world: &mut World) -> bool {
    world
        .query::<&EditableText>()
        .iter(world)
        .any(EditableText::is_composing)
}

pub fn send(world: &World, request: ClientMessage) -> Result<(), String> {
    if !world.resource::<Mobile>().ready {
        return Err("Wait for this device’s Organ to connect".into());
    }
    world
        .get_non_send::<Connection>()
        .ok_or("The connection is unavailable")?
        .send(request)
}

pub fn act(
    world: &mut World,
    action: engine::actions::Action,
    field: Option<(String, String)>,
) -> Result<(), String> {
    if !world.resource::<Mobile>().pending.is_empty() {
        return Err("Wait for the current change to finish".into());
    }
    let id = nucleus::new_uid("mobile");
    let created_record = matches!(action, engine::actions::Action::CreateRecordDraft { .. });
    let transition = matches!(
        action,
        engine::actions::Action::PreviewAreaTransition { .. }
    );
    let scope_changed = matches!(action, engine::actions::Action::RosterJoinOrgan { .. });
    let task_concept =
        matches!(&action, engine::actions::Action::CreateConcept { name, .. } if name == "task");
    let deleted = match &action {
        engine::actions::Action::DeleteRecord { target } => Some(target.clone()),
        _ => None,
    };
    let topic = match &action {
        engine::actions::Action::ReadMessageAttachment { .. } => "attachment-download",
        engine::actions::Action::SyncNow => "sync",
        engine::actions::Action::RosterStatus => "roster",
        engine::actions::Action::RosterEnrolToken => "enrolment",
        engine::actions::Action::RosterCreateOrgan => "identity",
        engine::actions::Action::SetCellConfig { namespace, .. }
            if namespace == "lince.discovery" || namespace == "lince.network" =>
        {
            "discovery"
        }
        engine::actions::Action::RosterRenameCell { .. }
        | engine::actions::Action::RosterRevokeCell { .. } => "device",
        _ => "result",
    };
    let form = match &action {
        engine::actions::Action::SaveKarmaRule { rule, .. } => {
            Some(format!("karma/{}/", rule.as_deref().unwrap_or("new")))
        }
        engine::actions::Action::SaveKarmaFrequency { frequency_uid, .. } => Some(format!(
            "frequency/{}/",
            frequency_uid.as_deref().unwrap_or("new")
        )),
        _ => None,
    };
    let clear_fields = match &action {
        engine::actions::Action::CreateMessage { thread, .. } => {
            vec![format!("{thread}/message"), format!("{thread}/parent")]
        }
        engine::actions::Action::AddKnownOrgan { .. } => {
            vec!["organ/invite".into(), "organ/name".into()]
        }
        engine::actions::Action::ChangeRecord { request } => {
            let names: &[&str] = match &request.mutation {
                engine::record_change::Mutation::Assertion { predicate, .. }
                    if predicate == "assigned-to" =>
                {
                    &["assignee"]
                }
                engine::record_change::Mutation::Assertion { .. } => {
                    &["predicate", "object", "amount", "unit"]
                }
                engine::record_change::Mutation::WorkLog { value: Some(_), .. } => {
                    &["log_start", "log_end", "log_id"]
                }
                _ => &[],
            };
            names
                .iter()
                .map(|name| format!("{}/{name}", request.record_uid))
                .collect()
        }
        _ => vec![],
    };
    let submitted = world
        .resource::<Mobile>()
        .drafts
        .iter()
        .filter(|(key, _)| {
            form.as_ref().is_some_and(|prefix| key.starts_with(prefix))
                || clear_fields.contains(key)
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let sent_attachments = match &action {
        engine::actions::Action::CreateMessage {
            thread, content, ..
        } => Some((thread.clone(), content.clone())),
        _ => None,
    };
    send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    let mut state = world.resource_mut::<Mobile>();
    state.pending.insert(
        id,
        Pending {
            field,
            text_snapshot: None,
            created_record,
            transition,
            scope_changed,
            topic,
            form,
            submitted,
            clear_fields,
            deleted,
            task_concept,
            sent_attachments,
            silent: false,
        },
    );
    state.status = "Saving…".into();
    Ok(())
}

pub(crate) fn subscribe(
    world: &mut World,
    topic: &'static str,
    protein: protein::Protein,
) -> Result<(), String> {
    send(
        world,
        ClientMessage::Subscribe {
            id: format!("mobile/{topic}"),
            protein,
        },
    )?;
    world.resource_mut::<Mobile>().active_topics.insert(topic);
    Ok(())
}

pub(crate) fn subscriptions(world: &mut World) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    let page = state.navigation.current.clone();
    let previous = state.active_record.clone();
    let desired: &[&str] = match &page {
        Page::Records => &["records", "saved_views"],
        Page::Kanban => &["records", "task_concept", "saved_views"],
        Page::Record(_) => &["record"],
        Page::Organ => &["organs", "roster_records", "pairing", "nearby"],
        Page::Karma => &["karma"],
        Page::Frequency => &["frequency"],
        Page::Credits => &[],
    };
    let obsolete: Vec<_> = state
        .active_topics
        .iter()
        .copied()
        .filter(|topic| !desired.contains(topic))
        .collect();
    for topic in obsolete {
        send(
            world,
            ClientMessage::Unsubscribe {
                id: format!("mobile/{topic}"),
            },
        )?;
        world.resource_mut::<Mobile>().active_topics.remove(topic);
    }
    let selected = match &page {
        Page::Record(uid) => Some(uid.clone()),
        _ => None,
    };
    if previous != selected {
        if let Some(uid) = previous {
            send(world, ClientMessage::CollabLeave { record_uid: uid })?;
            send(
                world,
                ClientMessage::Unsubscribe {
                    id: "mobile/record".into(),
                },
            )?;
        }
        world.resource_mut::<Mobile>().active_record = None;
        let mut state = world.resource_mut::<Mobile>();
        state.rows.remove("record");
        state.thread_pages.clear();
        let protected: Vec<_> = state
            .drafts
            .keys()
            .filter_map(|key| key.split_once('/').map(|(uid, _)| uid.to_owned()))
            .collect();
        state.documents.retain(|key, _| {
            let uid = key.split('/').next().unwrap_or(key);
            selected.as_deref() == Some(uid) || protected.iter().any(|value| value == uid)
        });
    }
    let state = world.resource::<Mobile>();
    match page {
        Page::Records | Page::Kanban => {
            let query = crate::views::query(state, page == Page::Kanban)?;
            if page == Page::Kanban {
                let mut concepts = source_query(protein::Source::Concept);
                concepts.filter = vec![protein::Predicate::SlugEq("task".into())];
                subscribe(world, "task_concept", concepts)?;
            }
            let saved = serde_json::from_value(serde_json::json!({"source":"record","where":[{"kind_eq":"protein"}],"fields":["uid","head","slug","extension"],"include":{"extension":{"namespace":"lince.protein"}},"limit":100,"order":[{"asc":"head"}]})).map_err(|error| format!("Saved views: {error}"))?;
            subscribe(world, "saved_views", saved)?;
            let draft = lince_interface::queries::ProteinDraft::from_protein(
                page.title().into(),
                String::new(),
                query,
            );
            subscribe(world, "records", draft.compile()?)?;
        }
        Page::Record(ref uid) => {
            let mut query = crate::record::query(Some(uid), 1, "");
            if let Some(threads) = &mut query.include.threads {
                threads.message_before = state
                    .thread_pages
                    .iter()
                    .filter_map(|(thread, pages)| {
                        pages.last().map(|cursor| (thread.clone(), cursor.clone()))
                    })
                    .collect();
            }
            subscribe(world, "record", query)?;
            send(
                world,
                ClientMessage::CollabJoin {
                    id: format!("mobile/doc/{uid}"),
                    record_uid: uid.clone(),
                },
            )?;
        }
        Page::Karma => subscribe(world, "karma", source_query(protein::Source::KarmaRule))?,
        Page::Frequency => subscribe(world, "frequency", source_query(protein::Source::Frequency))?,
        Page::Organ => {
            subscribe(world, "nearby", source_query(protein::Source::Nearby))?;
            let mut query = source_query(protein::Source::Record);
            query.filter = vec![protein::Predicate::KindEq("organ".into())];
            query.include.contact = true;
            subscribe(world, "organs", query)?;
            for (topic, namespace) in [
                ("roster_records", "lince.roster"),
                ("pairing", "lince.pairing"),
            ] {
                let mut query = source_query(protein::Source::Record);
                query.filter = vec![protein::Predicate::SlugEq("local-organ".into())];
                query.include.extension = Some(protein::ExtensionInclude {
                    namespace: namespace.into(),
                });
                subscribe(world, topic, query)?;
            }
        }
        Page::Credits => {}
    }
    world.resource_mut::<Mobile>().active_record = selected;
    Ok(())
}

fn source_query(source: protein::Source) -> protein::Protein {
    protein::Protein {
        source,
        filter: Vec::new(),
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(100),
    }
}

fn receive(world: &mut World) {
    let mut events = Vec::new();
    let mut disconnected = false;
    if let Some(connection) = world.get_non_send::<Connection>() {
        for _ in 0..32 {
            match connection.incoming.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
    }
    for event in events {
        match event {
            Event::Stopped => {
                let next = world.resource_mut::<Mobile>().next_profile.take();
                if let Some(next) = next {
                    let root = world.resource::<Mobile>().profile_root.clone();
                    match next.save(&root) {
                        Ok(()) => {
                            world.resource_mut::<Mobile>().status =
                                "Opening the selected profile…".into();
                            if let Err(error) = restart_profile(world, &root) {
                                world.resource_mut::<Mobile>().status =
                                    format!("Profile selected. Reopen Lince to continue: {error}");
                            }
                        }
                        Err(error) => {
                            world.remove_non_send::<Connection>();
                            world.insert_resource(Mobile::new(root));
                            connect(world);
                            world.resource_mut::<Mobile>().status = format!(
                                "Could not switch profile: {error}. Reopening the previous profile."
                            );
                        }
                    }
                }
            }
            Event::LoginFailed(error) => {
                world.resource_mut::<Mobile>().status = error;
            }
            Event::Ready(organ, setup, identity) => identity_ready(world, organ, setup, identity),
            Event::Failed(error) => {
                let mut state = world.resource_mut::<Mobile>();
                state.ready = false;
                state.status = error;
            }
            Event::Message(message) => receive_message(world, message),
        }
    }
    if disconnected && world.resource::<Mobile>().ready {
        let mut state = world.resource_mut::<Mobile>();
        state.ready = false;
        state.pending.clear();
        state.status = "The connection stopped. Your edits are kept; reopen Lince.".into();
    }
}

fn identity_ready(
    world: &mut World,
    organ: String,
    setup: bool,
    identity: crate::session::Identity,
) {
    let changed = {
        let state = world.resource::<Mobile>();
        state.identity != identity || state.organ.is_some()
    };
    if changed {
        capture(world);
        if let Some(old) = world.resource::<Mobile>().organ.clone() {
            if let Err(error) = save(world, old.clone()) {
                let state = world.resource::<Mobile>();
                let key = state.scope_key();
                let saved = draft_snapshot(state, old);
                world.init_resource::<DraftRecovery>();
                world.resource_mut::<DraftRecovery>().0.insert(key, saved);
                world.resource_mut::<Mobile>().status = error;
            }
        }
        let root = world.resource::<Mobile>().profile_root.clone();
        let old: Vec<_> = world
            .query_filtered::<Entity, With<Content>>()
            .iter(world)
            .collect();
        for entity in old {
            world.despawn(entity);
        }
        world.insert_resource(Mobile::new(root));
    }
    world.resource_mut::<Mobile>().organ = Some(organ.clone());
    world.resource_mut::<Mobile>().identity = identity.clone();
    let key = world.resource::<Mobile>().scope_key();
    let recovered = world
        .get_resource_mut::<DraftRecovery>()
        .and_then(|mut recovery| recovery.0.remove(&key));
    let was_recovered = recovered.is_some();
    let mut state = world.resource_mut::<Mobile>();
    state.ready = true;
    state.identity = identity;
    state.status = "Connected to this device’s Organ".into();
    if state.identity == crate::session::Identity::Locked {
        state.organ = Some(organ);
        state.navigation.open(Page::Organ);
        state.status = "Sign in to open this profile".into();
        state.dirty = true;
        return;
    }
    let saved = match recovered {
        Some(saved) => Ok(Some(saved)),
        None => crate::storage::read(&draft_directory(&state), &organ),
    };
    match saved {
        Ok(Some(saved)) => {
            state.navigation = saved.navigation;
            state.drafts = saved.drafts;
            state.documents = saved.documents;
            state.karma = saved.karma;
            state.frequency = saved.frequency;
            state.search = saved.search;
            state.sort = saved.sort.min(crate::record::SORTS.len() - 1);
            state.negative_only = saved.negative_only;
            state.view = saved.view;
            state.attachments = saved.attachments;
            state.outbox = saved.outbox;
            if !state.outbox.is_empty() {
                state.status =
                    "An edit was interrupted. Press its Save button again to finish it safely."
                        .into();
            }
            if was_recovered {
                state.last_saved.clear();
                state.save_requested = true;
                state.status = "Recovered unsaved drafts from this session. Keep Lince open until draft storage works again.".into();
            } else {
                state.last_saved = state.drafts.clone();
            }
        }
        Ok(None) => {}
        Err(error) => {
            state.status =
                format!("Could not restore drafts: {error}. The saved file has been kept.");
            state.save_error = true;
        }
    }
    state.organ = Some(organ);
    state.setup_required = setup;
    if setup {
        state.navigation.open(Page::Organ);
    }
    state.dirty = true;
    if let Err(error) = subscriptions(world) {
        world.resource_mut::<Mobile>().status = error;
    }
    if let Err(error) = act(world, engine::actions::Action::RosterStatus, None) {
        world.resource_mut::<Mobile>().status = error;
    }
}

fn restart_profile(_world: &mut World, _root: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        crate::android::restart()
    }
    #[cfg(not(target_os = "android"))]
    {
        std::process::Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
            .arg(_root)
            .spawn()
            .map_err(|error| error.to_string())?;
        _world.write_message(AppExit::Success);
        Ok(())
    }
}

fn persist(world: &mut World) {
    let state = world.resource::<Mobile>();
    let Some(organ) = state.organ.clone() else {
        return;
    };
    if state.save_error
        || (state.drafts == state.last_saved && !state.save_requested)
        || std::time::Instant::now() < state.save_after
    {
        return;
    }
    if let Err(error) = save(world, organ) {
        let mut state = world.resource_mut::<Mobile>();
        state.status = format!("Drafts are in memory but could not be saved: {error}");
        state.save_error = true;
    }
}

fn save(world: &mut World, organ: String) -> Result<(), String> {
    let state = world.resource::<Mobile>();
    if state.identity == crate::session::Identity::Locked {
        return Ok(());
    }
    if state.save_error {
        return Err("Restore draft storage before saving more changes".into());
    }
    let saved = draft_snapshot(state, organ);
    crate::storage::write(&draft_directory(state), &saved).map_err(|error| error.to_string())?;
    let mut state = world.resource_mut::<Mobile>();
    state.last_saved = state.drafts.clone();
    state.save_requested = false;
    Ok(())
}

fn draft_snapshot(state: &Mobile, organ: String) -> crate::storage::Saved {
    crate::storage::Saved {
        organ,
        navigation: state.navigation.clone(),
        drafts: state
            .drafts
            .iter()
            .filter(|(key, _)| {
                !key.starts_with("organ/")
                    && !key.starts_with("enrolment/")
                    && !key.starts_with("login/")
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        documents: state.documents.clone(),
        karma: state.karma.clone(),
        frequency: state.frequency.clone(),
        search: state.search.clone(),
        sort: state.sort,
        negative_only: state.negative_only,
        view: state.view.clone(),
        attachments: state.attachments.clone(),
        outbox: state.outbox.clone(),
    }
}

fn draft_directory(state: &Mobile) -> PathBuf {
    match &state.identity {
        crate::session::Identity::Person(person) => state.directory.join("person-drafts").join(
            person
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        ),
        _ => state.directory.clone(),
    }
}

fn receive_message(world: &mut World, message: ServerMessage) {
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            if id == "mobile/link" {
                let _ = send(world, ClientMessage::Unsubscribe { id });
                world.resource_mut::<Mobile>().active_topics.remove("link");
                if let Some(uid) = rows.first().and_then(|row| row["uid"].as_str()) {
                    queue_intent(world, Intent::Open(Page::Record(uid.into())));
                } else {
                    world.resource_mut::<Mobile>().status =
                        "That Record is unavailable to this Person".into();
                }
                return;
            }
            let Some(topic) = id.strip_prefix("mobile/") else {
                return;
            };
            let editing = editing(world);
            let mut state = world.resource_mut::<Mobile>();
            let changed = state.rows.get(topic) != Some(&rows);
            state.rows.insert(topic.into(), rows);
            if changed && !editing {
                state.dirty = true;
            }
        }
        ServerMessage::CollabState {
            record_uid,
            snapshot_base64,
            ..
        } => {
            capture(world);
            if let Err(error) = refresh_document(world, &record_uid, &snapshot_base64) {
                world.resource_mut::<Mobile>().status = error;
            }
        }
        ServerMessage::CollabChange { record_uid, .. } => {
            if world.resource::<Mobile>().active_record.as_ref() == Some(&record_uid) {
                let result = send(
                    world,
                    ClientMessage::CollabJoin {
                        id: format!("mobile/doc/{record_uid}"),
                        record_uid,
                    },
                );
                if let Err(error) = result {
                    world.resource_mut::<Mobile>().status = error;
                }
            }
        }
        ServerMessage::ActionOk {
            id,
            created,
            warnings,
            data,
            ..
        } => {
            let pending = world.resource_mut::<Mobile>().pending.remove(&id);
            let Some(pending) = pending else { return };
            if pending.task_concept {
                if let Some(uid) = created {
                    if let Err(error) = create_record(world, Some(uid)) {
                        world.resource_mut::<Mobile>().status = error;
                    }
                }
                return;
            }
            if pending.transition {
                let result = data
                    .ok_or_else(|| "The column change has no preview".to_string())
                    .and_then(|data| {
                        serde_json::from_value::<engine::area_transition::TransitionPreview>(data)
                            .map_err(|error| error.to_string())
                    })
                    .and_then(|preview| {
                        act(
                            world,
                            engine::actions::Action::ApplyAreaTransition {
                                request_id: nucleus::new_uid("mobile-move"),
                                preview,
                            },
                            None,
                        )
                    });
                if let Err(error) = result {
                    world.resource_mut::<Mobile>().status = error;
                }
                return;
            }
            if let Some((key, submitted)) = &pending.field {
                for mut input in world.query::<&mut Input>().iter_mut(world) {
                    if input.key == *key {
                        input.initial = submitted.clone();
                    }
                }
            }
            if let Some(prefix) = &pending.form {
                let fields: Vec<_> = world
                    .query::<(Entity, &Input)>()
                    .iter(world)
                    .filter(|(_, input)| input.key.starts_with(prefix))
                    .map(|(entity, _)| entity)
                    .collect();
                for field in fields {
                    world.entity_mut(field).remove::<Input>();
                }
            }
            let fields: Vec<_> = world
                .query::<(Entity, &Input)>()
                .iter(world)
                .filter(|(_, input)| pending.clear_fields.contains(&input.key))
                .map(|(entity, _)| entity)
                .collect();
            for field in fields {
                world.entity_mut(field).remove::<Input>();
            }
            let mut state = world.resource_mut::<Mobile>();
            for key in &pending.clear_fields {
                if state.drafts.get(key) == pending.submitted.get(key) {
                    state.drafts.remove(key);
                }
            }
            if let Some(prefix) = pending.form {
                let replacement = created
                    .as_ref()
                    .and_then(|uid| {
                        prefix
                            .ends_with("/new/")
                            .then(|| format!("{}/{uid}/", prefix.split('/').next().unwrap()))
                    })
                    .unwrap_or_else(|| prefix.clone());
                let remaining: Vec<_> = state
                    .drafts
                    .iter()
                    .filter(|(key, value)| {
                        key.starts_with(&prefix) && pending.submitted.get(*key) != Some(*value)
                    })
                    .map(|(key, value)| {
                        (
                            format!("{}{}", replacement, &key[prefix.len()..]),
                            value.clone(),
                        )
                    })
                    .collect();
                state.drafts.retain(|key, _| !key.starts_with(&prefix));
                if prefix.starts_with("karma/") {
                    state.karma = None;
                }
                if prefix.starts_with("frequency/") {
                    state.frequency = None;
                }
                state.drafts.extend(remaining);
            }
            if let Some((key, submitted)) = pending.field {
                state.outbox.remove(&key);
                state.save_requested = true;
                if let Some(snapshot) = pending.text_snapshot {
                    state.documents.insert(key.clone(), snapshot);
                }
                if state.drafts.get(&key) == Some(&submitted) {
                    state.drafts.remove(&key);
                }
                if let Some((uid, property)) = key.split_once('/') {
                    for records in state.rows.values_mut() {
                        if let Some(record) = records
                            .iter_mut()
                            .find(|row| row["uid"].as_str() == Some(uid))
                        {
                            record[property] = Value::String(submitted.clone());
                        }
                    }
                }
            }
            state.status = if pending.silent {
                state.status.clone()
            } else if warnings.is_empty() {
                if pending.topic == "roster" {
                    "Device status refreshed".into()
                } else {
                    "Saved on this device".into()
                }
            } else {
                warnings.join("\n")
            };
            #[cfg(target_os = "android")]
            if pending.topic == "roster" {
                let discovery = data.as_ref().map(|row| &row["discovery"]);
                let enabled = discovery.is_some_and(|fields| fields["local"] == true);
                let remaining = discovery
                    .and_then(|fields| fields["local_until"].as_str())
                    .and_then(|until| chrono::DateTime::parse_from_rfc3339(until).ok())
                    .map(|until| {
                        (until.with_timezone(&chrono::Utc) - chrono::Utc::now())
                            .num_milliseconds()
                            .max(0)
                    })
                    .unwrap_or(900_000);
                let _ = crate::android::discovery(enabled && remaining > 0, remaining);
            }
            let mut changed = !pending.silent;
            if let Some(data) = data {
                if pending.topic == "attachment-download" {
                    state.status = crate::attachments::save(&data).map_or_else(
                        |error| error,
                        |_| "Choose where to save the attachment".into(),
                    );
                }
                if pending.topic == "sync" {
                    if let Some(roster) = state
                        .rows
                        .get_mut("roster")
                        .and_then(|rows| rows.first_mut())
                    {
                        roster["sync"] = data.clone();
                    }
                }
                if pending.topic != "attachment-download" {
                    changed |=
                        state.rows.get(pending.topic).and_then(|rows| rows.first()) != Some(&data);
                    state.rows.insert(pending.topic.into(), vec![data]);
                }
            }
            if pending.topic == "identity" {
                state.setup_required = false;
            }
            state.dirty |= changed;
            if let Some((thread, sent)) = pending.sent_attachments {
                if let Some(parts) = state.attachments.get_mut(&thread) {
                    if parts.starts_with(&sent) {
                        parts.drain(..sent.len());
                    }
                }
                state.save_requested = true;
            }
            state.moving = None;
            if let Some(uid) = pending.deleted {
                state
                    .drafts
                    .retain(|key, _| !key.starts_with(&format!("{uid}/")));
                state
                    .documents
                    .retain(|key, _| !key.starts_with(&format!("{uid}/")));
                if state.navigation.current == Page::Record(uid) {
                    state.navigation.open(Page::Records);
                }
            }
            if pending.created_record
                && let Some(uid) = created
            {
                state.navigation.open(Page::Record(uid));
            }
            if pending.scope_changed {
                state.drafts.clear();
                state.documents.clear();
                state.rows.clear();
                state.karma = None;
                state.frequency = None;
                state.outbox.clear();
                state.navigation.reset();
                state.ready = false;
                state.active_topics.clear();
                state.active_record = None;
                state.status = "Device enrolled. Connecting to your Organ…".into();
                return;
            }
            if pending.silent {
                return;
            }
            if let Err(error) = subscriptions(world) {
                world.resource_mut::<Mobile>().status = error;
            }
            if matches!(pending.topic, "identity" | "discovery" | "device") {
                if let Err(error) = act(world, engine::actions::Action::RosterStatus, None) {
                    world.resource_mut::<Mobile>().status = error;
                }
            }
        }
        ServerMessage::Error { id, message, .. } => {
            let mut state = world.resource_mut::<Mobile>();
            if let Some(pending) = state.pending.remove(&id)
                && let Some((key, _)) = pending.field
            {
                state.outbox.remove(&key);
                state.save_requested = true;
            }
            state.status = message;
        }
        _ => {}
    }
}

fn refresh_document(world: &mut World, uid: &str, snapshot: &str) -> Result<(), String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    let document = loro::LoroDoc::new();
    document
        .import(&B64.decode(snapshot).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    for property in ["head", "body"] {
        let key = format!("{uid}/{property}");
        if world.resource::<Mobile>().drafts.contains_key(&key) {
            continue;
        }
        let composing = world
            .query::<(&Input, &EditableText)>()
            .iter(world)
            .any(|(input, text)| input.key == key && text.is_composing());
        if composing {
            continue;
        }
        let value = document.get_text(property).to_string();
        for (mut input, mut text) in world
            .query::<(&mut Input, &mut EditableText)>()
            .iter_mut(world)
        {
            if input.key == key {
                input.initial = value.clone();
                text.editor.set_text(&value);
            }
        }
        let mut state = world.resource_mut::<Mobile>();
        state.documents.insert(key, snapshot.into());
        for rows in state.rows.values_mut() {
            if let Some(row) = rows.iter_mut().find(|row| row["uid"].as_str() == Some(uid)) {
                row[property] = Value::String(value.clone());
            }
        }
    }
    Ok(())
}

fn apply(world: &mut World) {
    let intents: Vec<_> = world.resource_mut::<Intents>().0.drain(..).collect();
    for intent in intents {
        if let Err(error) = apply_intent(world, intent) {
            world.resource_mut::<Mobile>().status = error;
        }
    }
}

fn apply_intent(world: &mut World, intent: Intent) -> Result<(), String> {
    use engine::actions::Action;
    if composing(world) {
        return Err("Finish typing before changing pages or saving".into());
    }
    world.resource_mut::<Mobile>().dirty = true;
    world.resource_mut::<Mobile>().save_requested = true;
    match intent {
        Intent::LoadImage(source) => crate::images::request(world, source)?,
        Intent::OpenLink(reference) => {
            if reference.starts_with("https://") || reference.starts_with("http://") {
                #[cfg(target_os = "android")]
                return crate::android::open_link(&reference);
                #[cfg(not(target_os = "android"))]
                return Err(format!("Open this link on Android: {reference}"));
            }
            let target = reference.strip_prefix("record:").unwrap_or(&reference);
            if target.contains(':') || target.is_empty() {
                return Err("This link type is unsupported".into());
            }
            let mut query = crate::record::query(None, 1, "");
            query.filter = vec![protein::Predicate::Any(vec![
                protein::Predicate::UidEq(target.into()),
                protein::Predicate::SlugEq(target.into()),
            ])];
            subscribe(world, "link", query)?;
        }
        Intent::ReplyTo(thread, message) => {
            if let Some(message) = message {
                world
                    .resource_mut::<Mobile>()
                    .drafts
                    .insert(format!("{thread}/parent"), message);
            } else {
                world
                    .resource_mut::<Mobile>()
                    .drafts
                    .remove(&format!("{thread}/parent"));
            }
        }
        Intent::MoreMessages(thread) => {
            let cursor: protein::MessageCursor = world
                .resource::<Mobile>()
                .rows
                .get("record")
                .into_iter()
                .flatten()
                .flat_map(|row| row["threads"].as_array().into_iter().flatten())
                .find(|row| row["uid"] == thread && row["messages_has_more"] == true)
                .ok_or("This thread has no earlier messages")
                .and_then(|row| {
                    serde_json::from_value(row["messages_before"].clone())
                        .map_err(|_| "Refresh this thread before loading earlier messages")
                })?;
            let mut state = world.resource_mut::<Mobile>();
            let pages = state.thread_pages.entry(thread).or_default();
            if pages.last() != Some(&cursor) {
                pages.push(cursor);
            }
            subscriptions(world)?;
        }
        Intent::NewerMessages(thread) => {
            if let Some(pages) = world.resource_mut::<Mobile>().thread_pages.get_mut(&thread) {
                pages.pop();
            }
            subscriptions(world)?;
        }
        Intent::LatestMessages(thread) => {
            world.resource_mut::<Mobile>().thread_pages.remove(&thread);
            subscriptions(world)?;
        }
        Intent::Nothing => {}
        Intent::ColumnSettings => {
            let mut state = world.resource_mut::<Mobile>();
            state.column_settings = !state.column_settings;
        }
        Intent::KanbanColumn(column) => {
            let mut state = world.resource_mut::<Mobile>();
            state.kanban_column = column.min(lince_interface::records::KANBAN_COLUMNS.len());
            state.record_pages.clear();
            subscriptions(world)?;
        }
        Intent::AttachFile(thread) => crate::attachments::choose(world, &thread)?,
        Intent::RemoveAttachment(thread, index) => {
            if let Some(parts) = world.resource_mut::<Mobile>().attachments.get_mut(&thread) {
                if index < parts.len() {
                    parts.remove(index);
                }
            }
        }
        Intent::Pick(picker) => {
            world.resource_mut::<Mobile>().picker = Some(picker);
            crate::picker::search(world)?;
        }
        Intent::SearchPicker => crate::picker::search(world)?,
        Intent::ClosePicker => crate::picker::close(world)?,
        Intent::SelectItem(uid) => {
            let picker = world
                .resource::<Mobile>()
                .picker
                .clone()
                .ok_or("Open a selector first")?;
            let value = row(world, "picker", &uid).ok_or("That item is no longer available")?;
            let selected = if matches!(picker.kind, crate::picker::Kind::Concept) {
                value["name"]
                    .as_str()
                    .ok_or("Concept name unavailable")?
                    .to_string()
            } else {
                uid
            };
            world
                .resource_mut::<Mobile>()
                .drafts
                .insert(format!("{}/{}", picker.scope, picker.field), selected);
            crate::picker::close(world)?;
        }
        Intent::ViewSettings
        | Intent::ToggleViewField(_)
        | Intent::LoadView(_)
        | Intent::SaveView => crate::views::apply(world, intent)?,
        Intent::EditLog(record, id) => {
            let row = row(world, "record", &record).ok_or("Record is unavailable")?;
            let log = row["work_logs"]
                .as_array()
                .and_then(|logs| logs.iter().find(|log| log["id"] == id))
                .ok_or("Work log is unavailable")?;
            let mut state = world.resource_mut::<Mobile>();
            state.drafts.insert(format!("{record}/log_id"), id);
            state.drafts.insert(
                format!("{record}/log_start"),
                log["start"].as_str().unwrap_or_default().into(),
            );
            state.drafts.insert(
                format!("{record}/log_end"),
                log["end"].as_str().unwrap_or_default().into(),
            );
        }
        Intent::CancelLog(record) => {
            for key in ["log_id", "log_start", "log_end"] {
                world
                    .resource_mut::<Mobile>()
                    .drafts
                    .remove(&format!("{record}/{key}"));
            }
        }
        Intent::Login | Intent::Logout => {
            if !world.resource::<Mobile>().pending.is_empty() {
                return Err("Wait for the current change before switching identity".into());
            }
            if let Some(organ) = world.resource::<Mobile>().organ.clone() {
                save(world, organ)?;
            }
            if matches!(intent, Intent::Login) {
                let username = world.resource::<Mobile>().draft("login", "username", "");
                let password = world
                    .resource_mut::<Mobile>()
                    .drafts
                    .remove("login/password")
                    .unwrap_or_default();
                world.non_send::<Connection>().login(username, password)?;
                let mut inputs = world.query::<(&Input, &mut EditableText)>();
                for (input, mut text) in inputs.iter_mut(world) {
                    if input.key == "login/password" {
                        text.editor.set_text("");
                    }
                }
            } else {
                world.non_send::<Connection>().logout()?;
            }
            world.resource_mut::<Mobile>().status = "Changing identity…".into();
        }
        Intent::Menu => {
            let mut state = world.resource_mut::<Mobile>();
            state.navigation.menu_open = !state.navigation.menu_open;
        }
        Intent::FreshProfile | Intent::SwitchProfile(_) => {
            if !world.resource::<Mobile>().pending.is_empty() {
                return Err("Wait for current changes to finish before switching profiles".into());
            }
            if let Some(organ) = world.resource::<Mobile>().organ.clone() {
                save(world, organ)?;
            }
            let mut profiles = world.resource::<Mobile>().profiles.clone();
            match intent {
                Intent::FreshProfile => {
                    profiles.create(&world.resource::<Mobile>().draft("profile", "name", ""))?
                }
                Intent::SwitchProfile(selected) => {
                    if selected
                        .as_ref()
                        .is_some_and(|id| !profiles.names.contains_key(id))
                    {
                        return Err("That profile is unavailable".into());
                    }
                    profiles.selected = selected;
                }
                _ => unreachable!(),
            }
            world.non_send::<Connection>().stop()?;
            let mut state = world.resource_mut::<Mobile>();
            state.next_profile = Some(profiles);
            state.ready = false;
            state.status = "Saving and closing this profile…".into();
        }
        Intent::Back => {
            back(world);
            subscriptions(world)?;
        }
        Intent::Open(page) => {
            if world.resource::<Mobile>().identity == crate::session::Identity::Locked
                && page != Page::Organ
                && page != Page::Credits
            {
                return Err("Sign in to open this profile".into());
            }
            if world.resource::<Mobile>().setup_required
                && page != Page::Organ
                && page != Page::Credits
            {
                return Err("Create your Organ or join an existing Organ first".into());
            }
            let organ = page == Page::Organ;
            if page != world.resource::<Mobile>().navigation.current {
                world.resource_mut::<Mobile>().record_pages.clear();
            }
            world.resource_mut::<Mobile>().navigation.open(page);
            subscriptions(world)?;
            if organ {
                act(world, Action::RosterStatus, None)?;
            }
        }
        Intent::Refresh => {
            world.resource_mut::<Mobile>().record_pages.clear();
            world.resource_mut::<Mobile>().thread_pages.clear();
            subscriptions(world)?;
        }
        Intent::RetryDrafts => {
            let mut state = world.resource_mut::<Mobile>();
            state.save_error = false;
            state.last_saved.clear();
            state.save_after = std::time::Instant::now();
        }
        Intent::Search => {
            let search = world.resource::<Mobile>().draft("list", "search", "");
            let mut state = world.resource_mut::<Mobile>();
            state.search = search;
            state.record_pages.clear();
            subscriptions(world)?;
        }
        Intent::Sort => {
            let mut state = world.resource_mut::<Mobile>();
            state.sort = (state.sort + 1) % crate::record::SORTS.len();
            let field = crate::record::SORTS[state.sort].1;
            if let Some(view) = &mut state.view {
                view.query["order"] = serde_json::json!([{"asc":field}]);
            }
            state.record_pages.clear();
            subscriptions(world)?;
        }
        Intent::ToggleNegative => {
            let mut state = world.resource_mut::<Mobile>();
            state.negative_only = !state.negative_only;
            state.record_pages.clear();
            subscriptions(world)?;
        }
        Intent::CompleteRecord(uid) => {
            world
                .resource_mut::<Mobile>()
                .drafts
                .insert(format!("{uid}/quantity"), "0".into());
            apply_intent(world, Intent::SaveField(uid, "quantity".into()))?;
        }
        Intent::RenameCell(uid) => {
            let label = world.resource::<Mobile>().draft("device", &uid, "");
            act(
                world,
                Action::RosterRenameCell {
                    cell_uid: uid,
                    label,
                },
                None,
            )?;
        }
        Intent::SavePeerPort => {
            let port = world
                .resource::<Mobile>()
                .draft("organ", "peer_port", "6175")
                .trim()
                .parse::<u16>()
                .map_err(|_| "Enter a port between 0 and 65535")?;
            act(
                world,
                Action::SetCellConfig {
                    namespace: "lince.network".into(),
                    fds: serde_json::json!({"peer_port":port}),
                },
                None,
            )?;
        }
        Intent::Discovery(enabled) => {
            let until = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
            let fds = serde_json::json!({"local": enabled, "internet": false, "direct": false, "local_until":until});
            act(
                world,
                Action::SetCellConfig {
                    namespace: "lince.discovery".into(),
                    fds,
                },
                None,
            )?;
            #[cfg(target_os = "android")]
            crate::android::discovery(enabled, 900_000)?;
        }
        Intent::ScanQr => {
            #[cfg(target_os = "android")]
            crate::android::scan()?;
            #[cfg(not(target_os = "android"))]
            return Err("Use the desktop Organ Castle to scan a QR, or paste its code here".into());
        }
        Intent::More => {
            let mut state = world.resource_mut::<Mobile>();
            let rows = state
                .rows
                .get("records")
                .ok_or("Wait for the Records to load")?;
            if rows.len() <= crate::views::PAGE_SIZE {
                return Ok(());
            }
            let cursor = rows[crate::views::PAGE_SIZE - 1]["uid"]
                .as_str()
                .ok_or("Record cursor is unavailable")?
                .to_owned();
            if state.record_pages.last() != Some(&cursor) {
                state.record_pages.push(cursor);
            }
            subscriptions(world)?;
        }
        Intent::Previous => {
            let mut state = world.resource_mut::<Mobile>();
            state.record_pages.pop();
            subscriptions(world)?;
        }
        Intent::CreateRecord => {
            let state = world.resource::<Mobile>();
            if state.navigation.current == Page::Kanban {
                let concepts = state
                    .rows
                    .get("task_concept")
                    .ok_or("Wait for the task list to load")?;
                if let Some(uid) = concepts.iter().find_map(|row| row["uid"].as_str()) {
                    let uid = uid.to_string();
                    create_record(world, Some(uid))?;
                } else {
                    act(
                        world,
                        Action::CreateConcept {
                            lingua: "g_local".into(),
                            name: "task".into(),
                            parents: vec![],
                        },
                        None,
                    )?;
                }
            } else {
                create_record(world, None)?;
            }
        }
        Intent::SaveField(uid, field) => {
            let state = world.resource::<Mobile>();
            let key = format!("{uid}/{field}");
            if state
                .pending
                .values()
                .any(|pending| pending.field.as_ref().is_some_and(|(name, _)| name == &key))
            {
                return Err("This field is already saving".into());
            }
            let prepared = match state.outbox.get(&key) {
                Some(prepared) => prepared.clone(),
                None => {
                    let value = state
                        .drafts
                        .get(&key)
                        .ok_or("This field has no unsaved changes")?;
                    let (action, snapshot) = crate::record::prepare_change(
                        &uid,
                        &field,
                        value,
                        state.documents.get(&key).map(String::as_str),
                    )?;
                    crate::record::Prepared {
                        action,
                        snapshot,
                        submitted: value.clone(),
                    }
                }
            };
            let organ = state
                .organ
                .clone()
                .ok_or("Wait for this device’s Organ to connect")?;
            world
                .resource_mut::<Mobile>()
                .outbox
                .insert(key.clone(), prepared.clone());
            save(world, organ)?;
            act(world, prepared.action, Some((key, prepared.submitted)))?;
            if let Some(pending) = world.resource_mut::<Mobile>().pending.values_mut().next() {
                pending.text_snapshot = prepared.snapshot;
            }
        }
        Intent::DeleteRecord(uid) => {
            world.resource_mut::<Mobile>().deleting =
                Some(Intent::Act(Action::DeleteRecord { target: uid }))
        }
        Intent::Confirm => {
            if let Some(intent) = world.resource_mut::<Mobile>().deleting.take() {
                apply_intent(world, intent)?;
            }
        }
        Intent::CancelDelete => world.resource_mut::<Mobile>().deleting = None,
        Intent::Act(action) => act(world, action, None)?,
        Intent::Ask(action) => world.resource_mut::<Mobile>().deleting = Some(Intent::Act(action)),
        Intent::MoveMenu(uid) => {
            let mut state = world.resource_mut::<Mobile>();
            state.moving = if state.moving.as_ref() == Some(&uid) {
                None
            } else {
                Some(uid)
            };
        }
        Intent::RecordControl(uid, control) => crate::pages::record_control(world, &uid, control)?,
        Intent::EditKarma(uid) => crate::pages::edit_karma(world, uid)?,
        Intent::SaveKarma => crate::pages::save_karma(world)?,
        Intent::EditFrequency(uid) => crate::pages::edit_frequency(world, uid)?,
        Intent::SaveFrequency => crate::pages::save_frequency(world)?,
        Intent::Weekday(day) => {
            let mut state = world.resource_mut::<Mobile>();
            let draft = state.frequency.as_mut().ok_or("Open a Frequency first")?;
            if draft.weekdays.contains(&day) {
                draft.weekdays.retain(|value| *value != day);
            } else {
                draft.weekdays.push(day);
            }
        }
        Intent::Pair => {
            let state = world.resource::<Mobile>();
            let invite = state.draft("organ", "invite", "");
            let name = state.draft("organ", "name", "");
            engine::pairing::PairingInvite::decode(invite.trim())
                .map_err(|error| error.to_string())?;
            act(
                world,
                Action::AddKnownOrgan {
                    invite: invite.trim().into(),
                    name,
                },
                None,
            )?;
        }
        Intent::JoinOrgan => {
            let code = world.resource::<Mobile>().draft("organ", "enrol", "");
            engine::pairing::EnrolmentInvite::decode(code.trim())
                .map_err(|error| error.to_string())?;
            world.resource_mut::<Mobile>().deleting =
                Some(Intent::Act(Action::RosterJoinOrgan { code }));
        }
        Intent::Move(uid, column) => {
            let changes = crate::kanban::changes(world.resource::<Mobile>(), column)?;
            act(
                world,
                Action::PreviewAreaTransition {
                    target: uid,
                    changes,
                    constraints: default(),
                },
                None,
            )?;
        }
    }
    world.resource_mut::<Mobile>().dirty = true;
    Ok(())
}

fn create_record(world: &mut World, task: Option<String>) -> Result<(), String> {
    act(
        world,
        engine::actions::Action::CreateRecordDraft {
            draft: crate::record::new_draft(task),
        },
        None,
    )
}

#[cfg(target_os = "android")]
pub(crate) fn queue_back(world: &mut World) {
    world.resource_mut::<Intents>().0.push_back(Intent::Back);
}

pub fn back(world: &mut World) {
    if world.resource::<Mobile>().picker.is_some() {
        if let Err(error) = crate::picker::close(world) {
            world.resource_mut::<Mobile>().status = error;
        }
        world.resource_mut::<Mobile>().dirty = true;
        return;
    }
    let mut state = world.resource_mut::<Mobile>();
    if state.deleting.take().is_some() {
        state.dirty = true;
        return;
    }
    let handled = state.navigation.back();
    state.dirty = true;
    #[cfg(target_os = "android")]
    if !handled {
        crate::android::finish();
    }
    #[cfg(not(target_os = "android"))]
    let _ = handled;
}

fn render(world: &mut World) {
    let message = world.resource::<Mobile>().status.clone();
    for mut text in world
        .query_filtered::<&mut Text, With<Status>>()
        .iter_mut(world)
    {
        text.set_if_neq(Text::new(message.clone()));
    }
    if !world.resource::<Mobile>().dirty || composing(world) {
        return;
    }
    world.resource_mut::<Mobile>().dirty = false;
    if let Some(proxy) = world.get_resource::<bevy::winit::EventLoopProxyWrapper>() {
        let _ = proxy.send_event(bevy::winit::WinitUserEvent::WakeUp);
    }
    let Some(root) = world
        .query_filtered::<Entity, With<Shell>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let scroll = world
        .query_filtered::<&ScrollPosition, With<Content>>()
        .iter(world)
        .next()
        .map(|position| position.0);
    let page_key = {
        let state = world.resource::<Mobile>();
        if state.navigation.menu_open {
            "menu".into()
        } else if state.deleting.is_some() {
            "confirmation".into()
        } else {
            format!("{:?}", state.navigation.current)
        }
    };
    let scroll = {
        let mut state = world.resource_mut::<Mobile>();
        if let Some(scroll) = scroll {
            let key = state.rendered_page.clone();
            if state.scrolls.len() >= 64 && !state.scrolls.contains_key(&key) {
                state.scrolls.clear();
            }
            state.scrolls.insert(key, scroll);
        }
        state.rendered_page = page_key.clone();
        state.scrolls.get(&page_key).copied().unwrap_or_default()
    };
    controls::clear(world, root);
    let header = controls::row(world, root);
    button(world, header, "Pages", Intent::Menu);
    button(world, header, "Back", Intent::Back);
    let title = world.resource::<Mobile>().navigation.current.title();
    label(world, header, title, 24.0);
    let status = label(world, root, &message, 14.0);
    world.entity_mut(status).insert(Status);
    if world.resource::<Mobile>().save_error {
        button(world, root, "Retry saving drafts", Intent::RetryDrafts);
    }
    let content = controls::column(world, root);
    world.entity_mut(content).insert((
        Content,
        ScrollPosition(scroll),
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            min_height: px(0),
            row_gap: px(10),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    ));
    if world.resource::<Mobile>().identity == crate::session::Identity::Locked {
        crate::organ::login(world, content);
        crate::organ::profile_controls(world, content);
        return;
    } else if world.resource::<Mobile>().picker.is_some() {
        crate::picker::render(world, content);
        return;
    } else if world.resource::<Mobile>().navigation.menu_open {
        for page in Page::MENU {
            if world.resource::<Mobile>().setup_required
                && page != Page::Organ
                && page != Page::Credits
            {
                continue;
            }
            button(world, content, page.title(), Intent::Open(page));
        }
        return;
    }
    if world.resource::<Mobile>().deleting.is_some() {
        let message = match world.resource::<Mobile>().deleting.as_ref() {
            Some(Intent::Act(engine::actions::Action::DeleteRecord { .. })) => {
                "Delete this Record?"
            }
            Some(Intent::Act(engine::actions::Action::RosterJoinOrgan { .. })) => {
                "Enrol this fresh device in the Organ shown below? Your existing Organ and Records cannot be replaced here."
            }
            Some(Intent::Act(engine::actions::Action::RosterRevokeCell { .. })) => {
                "Remove this device from the Organ roster?"
            }
            Some(Intent::Act(engine::actions::Action::ForgetOrganContact { .. })) => {
                "Forget this Organ contact?"
            }
            Some(Intent::Act(engine::actions::Action::DeleteRecurrence { .. })) => {
                "Delete this Karma?"
            }
            Some(Intent::Act(engine::actions::Action::DeleteFrequency { .. })) => {
                "Delete this Frequency?"
            }
            _ => "Apply this change?",
        };
        label(world, content, message, 22.0);
        if let Some(Intent::Act(engine::actions::Action::RosterJoinOrgan { code })) =
            world.resource::<Mobile>().deleting.clone()
            && let Ok(invite) = engine::pairing::EnrolmentInvite::decode(&code)
        {
            label(
                world,
                content,
                &format!("Organ: {}", invite.organ_uid),
                18.0,
            );
            label(
                world,
                content,
                &format!("Identity key: {}", invite.root_key),
                14.0,
            );
            label(
                world,
                content,
                "Compare this identity with the device showing the code. Discovery and a contact code do not grant roster membership.",
                16.0,
            );
        }
        button(world, content, "Confirm", Intent::Confirm);
        button(world, content, "Cancel", Intent::CancelDelete);
        return;
    }
    let current = world.resource::<Mobile>().navigation.current.clone();
    let page = if world.resource::<Mobile>().setup_required && current != Page::Credits {
        Page::Organ
    } else {
        current
    };
    crate::pages::render(world, content, page);
}

pub fn label(world: &mut World, parent: Entity, value: &str, size: f32) -> Entity {
    let font = world.resource::<Typography>().text(size);
    world
        .spawn((
            ChildOf(parent),
            Text::new(value),
            font,
            TextColor(INK),
            TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
            Node {
                flex_shrink: 0.0,
                max_width: percent(100),
                ..default()
            },
        ))
        .id()
}

pub fn button(world: &mut World, parent: Entity, title: &str, intent: Intent) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            ButtonIntent(intent),
            Node {
                min_height: px(48),
                min_width: px(48),
                padding: UiRect::all(px(10)),
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(PURPLE),
        ))
        .id();
    label(world, entity, title, 18.0);
    entity
}

pub fn input(
    world: &mut World,
    parent: Entity,
    scope: &str,
    name: &str,
    title: &str,
    initial: &str,
    multiline: bool,
) -> Entity {
    label(world, parent, title, 16.0);
    let value = world.resource::<Mobile>().draft(scope, name, initial);
    let bundle = controls::text_editor(&value, world.resource::<Typography>(), 0);
    let entity = world
        .spawn((
            ChildOf(parent),
            bundle,
            Input {
                key: format!("{scope}/{name}"),
                initial: initial.into(),
                title: title.into(),
            },
        ))
        .id();
    let options = controls::EditorOptions {
        multiline,
        max_characters: if multiline { 262_144 } else { 65_536 },
        visible_lines: if multiline { 5.0 } else { 1.0 },
        minimum_height: 48.0,
    };
    if scope == "login" && name == "password" {
        world
            .entity_mut(entity)
            .remove::<lince_interface::style::TextToken>()
            .insert(TextColor(Color::NONE));
        let font = world.resource::<Typography>().text(22.0);
        world.spawn((
            ChildOf(entity),
            PasswordMask(entity),
            Text::new(""),
            font,
            TextColor(INK),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(6),
                top: px(6),
                ..default()
            },
        ));
    }
    let mut query = world.query::<(&mut EditableText, &mut Node)>();
    let (mut text, mut node) = query.get_mut(world, entity).unwrap();
    options.apply(&mut text, &mut node);
    entity
}

pub fn row(world: &World, topic: &str, uid: &str) -> Option<Value> {
    world
        .resource::<Mobile>()
        .rows
        .get(topic)?
        .iter()
        .find(|row| row["uid"].as_str() == Some(uid))
        .cloned()
}
