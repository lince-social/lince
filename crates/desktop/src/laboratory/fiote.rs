use crate::{
    actions::Action,
    protein_area::{RecordBinding, Source},
};
use bevy::{input_focus::InputFocus, prelude::*};
use cell::fiote::laboratory::{HELLO, Options, Preparation, Prepared, Report};
use std::time::{Duration, Instant};
use tokio::{sync::watch, task::JoinHandle};

#[derive(Resource)]
pub struct Launch(pub Options);

#[derive(Component)]
struct PreparedConnection(String);

pub(crate) fn connection_ready(world: &World, binding: &RecordBinding) -> bool {
    world
        .get::<PreparedConnection>(binding.area)
        .is_some_and(|connection| connection.0 == binding.uid)
}

#[derive(Resource)]
struct Live {
    root: Entity,
    content: Entity,
    status: Entity,
    connection: Entity,
    model: Entity,
    reasoning: Entity,
    options: Options,
    choices: Vec<(String, String)>,
    preparation: Option<JoinHandle<Result<Preparation, String>>>,
    verification: Option<JoinHandle<Report>>,
    prepared: Option<Prepared>,
    report: Option<Report>,
    pending_report: Option<Report>,
    view_started: Option<Instant>,
    setup: Option<String>,
    authorized: bool,
    browser_url: Option<String>,
    setup_task: Option<JoinHandle<Result<Option<String>, String>>>,
    cancel: watch::Sender<bool>,
    started: Instant,
    retry: Instant,
    ready_at: Option<Instant>,
    running: bool,
    headless: bool,
}

pub(super) fn install(app: &mut App) {
    app.add_systems(Update, tick.after(crate::cell_bridge::ReceiveCell));
}

#[derive(Clone)]
pub struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, _: Entity) {
        super::workspace::open(world);
        if world.contains_resource::<Live>() {
            return;
        }
        let Some(root) = world.resource::<super::Laboratory>().root else {
            return;
        };
        let mut options = world
            .remove_resource::<Launch>()
            .map(|launch| launch.0)
            .unwrap_or_default();
        if options.record.is_none() {
            options.record = world.resource::<super::Laboratory>().fiote_target.clone();
        }
        mount(world, root, options, false);
    }
}

fn mount(world: &mut World, root: Entity, options: Options, headless: bool) {
    let panel = world
        .spawn((
            ChildOf(root),
            GlobalZIndex(50),
            Node {
                position_type: PositionType::Absolute,
                top: percent(46),
                bottom: px(12),
                left: px(12),
                right: px(12),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(px(12)),
                ..default()
            },
            ScrollPosition::default(),
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    crate::scroll_sand::attach(world, panel);
    crate::edit_mode::label(world, panel, "Test real Fiote conversation", 20.0);
    crate::edit_mode::label(
        world,
        panel,
        "Uses your chosen AI connection for one greeting. Model tools are disabled for this test thread. The conversation is retained.",
        14.0,
    );
    let status = crate::edit_mode::label(world, panel, "Choose a target, then Run", 14.0);
    let connection = crate::edit_mode::label(
        world,
        panel,
        "Uses the selected Fiote's native connection",
        14.0,
    );
    let model = crate::fiote::session::field(world, panel, "Test model override (optional)", false);
    let reasoning =
        crate::fiote::session::field(world, panel, "Test reasoning override (optional)", false);
    world
        .get_mut::<bevy::text::EditableText>(model)
        .unwrap()
        .editor
        .set_text(options.model.as_deref().unwrap_or_default());
    world
        .get_mut::<bevy::text::EditableText>(reasoning)
        .unwrap()
        .editor
        .set_text(options.reasoning.as_deref().unwrap_or_default());
    let row = world
        .spawn((
            ChildOf(panel),
            Node {
                column_gap: px(6),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        ))
        .id();
    for (title, action) in [
        ("Run", Control::Run),
        ("Cancel", Control::Cancel),
        ("Next Fiote", Control::Next),
        ("Existing / separate test Fiote", Control::Separate),
        ("Open conversation", Control::Conversation),
    ] {
        crate::description::button(world, row, root, title, action);
    }
    if !headless {
        crate::description::button(
            world,
            row,
            root,
            "Export report",
            super::LaboratoryAction::Export,
        );
    }
    let content = world
        .spawn((
            ChildOf(panel),
            Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                ..default()
            },
        ))
        .id();
    let (cancel, _) = watch::channel(false);
    world.insert_resource(Live {
        root,
        content,
        status,
        connection,
        model,
        reasoning,
        options,
        choices: Vec::new(),
        preparation: None,
        verification: None,
        prepared: None,
        report: None,
        pending_report: None,
        view_started: None,
        setup: None,
        authorized: false,
        browser_url: None,
        setup_task: None,
        cancel,
        started: Instant::now(),
        retry: Instant::now(),
        ready_at: None,
        running: false,
        headless,
    });
}

#[derive(Clone)]
enum Control {
    Run,
    Cancel,
    Next,
    Separate,
    Conversation,
}
impl Action for Control {
    fn apply(&self, world: &mut World, _: Entity) {
        let Some(mut live) = world.remove_resource::<Live>() else {
            return;
        };
        match self {
            Self::Run if !live.running => {
                live.options.model = world
                    .get::<bevy::text::EditableText>(live.model)
                    .map(|text| text.value().to_string().trim().to_string())
                    .filter(|value| !value.is_empty());
                live.options.reasoning = world
                    .get::<bevy::text::EditableText>(live.reasoning)
                    .map(|text| text.value().to_string().trim().to_string())
                    .filter(|value| !value.is_empty());
                live.report = None;
                live.pending_report = None;
                live.view_started = None;
                live.prepared = None;
                live.setup = None;
                live.authorized = false;
                live.browser_url = None;
                live.cancel.send_replace(false);
                live.started = Instant::now();
                live.retry = Instant::now();
                live.running = true;
                live.ready_at = None;
                crate::protein_area::laboratory_detach(world, live.content);
                world
                    .entity_mut(live.content)
                    .remove::<(PreparedConnection, crate::thread_castle::ThreadCastle)>();
                world.entity_mut(live.content).despawn_related::<Children>();
            }
            Self::Cancel => {
                live.cancel.send_replace(true);
            }
            Self::Next if !live.running => {
                let index = live
                    .choices
                    .iter()
                    .position(|choice| Some(&choice.0) == live.options.record.as_ref())
                    .unwrap_or(0);
                if !live.choices.is_empty() {
                    let next = &live.choices[(index + 1) % live.choices.len()];
                    live.options.record = Some(next.0.clone());
                    world.get_mut::<Text>(live.status).unwrap().0 = format!("Target: {}", next.1);
                }
            }
            Self::Separate if !live.running => {
                live.options.separate = !live.options.separate;
                world.get_mut::<Text>(live.status).unwrap().0 = if live.options.separate {
                    "A separate Laboratory Fiote will use the chosen connection"
                } else {
                    "A new test thread will use the existing Fiote"
                }
                .into();
            }
            Self::Conversation => {
                if let Some(record) = live
                    .report
                    .as_ref()
                    .and_then(|report| report.record.as_deref())
                {
                    crate::full_record::open_fiote(world, live.root, record);
                }
            }
            _ => {}
        }
        world.insert_resource(live);
    }
}

pub(super) fn cancel(world: &mut World) {
    if let Some(live) = world.get_resource::<Live>() {
        live.cancel.send_replace(true);
    }
}

pub(super) fn busy(world: &World) -> bool {
    world
        .get_resource::<Live>()
        .is_some_and(|live| live.running)
}

pub(super) fn exported(world: &World) -> Option<&Report> {
    world
        .get_resource::<Live>()
        .and_then(|live| live.report.as_ref())
}

fn fail(live: &mut Live, stage: &str, detail: &str, outcome: &str) {
    live.pending_report = None;
    live.view_started = None;
    if let Some(task) = live.preparation.take() {
        task.abort();
    }
    if let Some(task) = live.setup_task.take() {
        task.abort();
    }
    let mut report = live
        .prepared
        .as_ref()
        .map(|prepared| prepared.report.clone())
        .unwrap_or_default();
    report.outcome = outcome.into();
    report.stage = stage.into();
    report.detail = detail.into();
    report.elapsed_ms = live.started.elapsed().as_millis() as u64;
    if report.record.is_none() {
        report.record = live.setup.clone().or_else(|| live.options.record.clone());
    }
    live.report = Some(report);
    live.running = false;
}

fn tick(world: &mut World) {
    if world.contains_resource::<Launch>()
        && !world.contains_resource::<Live>()
        && world.contains_resource::<crate::app::CellHandle>()
    {
        Open.apply(world, Entity::PLACEHOLDER);
        Control::Run.apply(world, Entity::PLACEHOLDER);
    }
    let Some(mut live) = world.remove_resource::<Live>() else {
        return;
    };
    if !world.entities().contains(live.root) {
        live.cancel.send_replace(true);
        return;
    }
    let Some(runtime) = world
        .get_resource::<crate::app::CellHandle>()
        .map(|handle| handle.0.clone())
    else {
        world.insert_resource(live);
        return;
    };
    let Some(host) = runtime.fiote.clone() else {
        fail(
            &mut live,
            "connection",
            "Fiote service is unavailable",
            "failed",
        );
        world.insert_resource(live);
        return;
    };
    if live.choices.is_empty() {
        let host = host.clone();
        if let Ok(choices) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(host.laboratory_choices())
        }) {
            live.choices = choices
                .into_iter()
                .map(|choice| (choice.record, choice.title))
                .collect();
            if live.options.record.is_none()
                && let Some((record, title)) = live.choices.first()
            {
                live.options.record = Some(record.clone());
                world.get_mut::<Text>(live.status).unwrap().0 = format!("Target: {title}");
            }
        }
    }
    if live.running && *live.cancel.borrow() && live.verification.is_none() {
        fail(&mut live, "cancelled", "Stopped by the user", "cancelled");
    }
    if live.running && live.started.elapsed() > Duration::from_secs(600) && live.prepared.is_none()
    {
        fail(
            &mut live,
            "authentication",
            "Authorization did not finish within ten minutes",
            "interaction-required",
        );
    }
    if live.running
        && live.preparation.is_none()
        && live.prepared.is_none()
        && live.retry.elapsed() >= Duration::from_millis(250)
    {
        let host = host.clone();
        let options = live.options.clone();
        live.preparation = Some(tokio::spawn(async move {
            host.laboratory_prepare(&options).await
        }));
        live.retry = Instant::now();
    }
    if live
        .preparation
        .as_ref()
        .is_some_and(|task| task.is_finished())
    {
        let task = live.preparation.take().unwrap();
        let result =
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(task));
        match result {
            Ok(Ok(Preparation::Setup { record, detail })) => {
                world.get_mut::<Text>(live.status).unwrap().0 = detail;
                if live.headless
                    && live.setup_task.is_none()
                    && live.authorized
                    && live.browser_url.is_none()
                {
                    fail(
                        &mut live,
                        "authentication",
                        "Unlock or connect in Lince, or provide --laboratory-vault-password-file",
                        "interaction-required",
                    );
                } else {
                    if live.setup.as_deref() != Some(&record) {
                        live.setup = Some(record.clone());
                        if !live.headless {
                            let binding = RecordBinding {
                                area: live.root,
                                uid: record.clone(),
                                source: Source::Local,
                            };
                            crate::fiote::session::populate(world, live.content, binding.clone());
                            crate::fiote::session::command(world, &binding, "/login");
                        }
                    }
                    if !live.authorized && live.setup_task.is_none() {
                        let host = host.clone();
                        let profile = live.options.profile.clone();
                        live.setup_task = Some(tokio::spawn(async move {
                            host.laboratory_authorize(&record, profile.as_deref()).await
                        }));
                    }
                }
            }
            Ok(Ok(Preparation::Ready(mut prepared))) => {
                world.get_mut::<Text>(live.connection).unwrap().0 = format!(
                    "{} · {} · reasoning {} · fast {} · model tools disabled",
                    prepared.report.provider,
                    prepared.report.model,
                    prepared.report.reasoning.as_deref().unwrap_or("default"),
                    prepared.report.fast
                );
                prepared.report.timings.push((
                    "connection".into(),
                    live.started.elapsed().as_millis() as u64,
                ));
                world.entity_mut(live.content).despawn_related::<Children>();
                world
                    .entity_mut(live.content)
                    .insert(PreparedConnection(prepared.report.record.clone().unwrap()));
                crate::thread_castle::populate(
                    world,
                    live.content,
                    RecordBinding {
                        area: live.content,
                        uid: prepared.report.record.clone().unwrap(),
                        source: Source::Local,
                    },
                    &prepared.data,
                );
                crate::protein_area::laboratory_attach(world, live.content, prepared.data.clone());
                live.prepared = Some(prepared);
                live.ready_at = Some(Instant::now());
            }
            _ => fail(
                &mut live,
                "connection",
                "Cannot prepare this native connection; inspect its settings",
                "failed",
            ),
        }
    }
    if live
        .setup_task
        .as_ref()
        .is_some_and(|task| task.is_finished())
    {
        let task = live.setup_task.take().unwrap();
        let result =
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(task));
        live.authorized = true;
        if let Ok(Ok(Some(url))) = result {
            if live.browser_url.as_ref() != Some(&url) {
                if live.headless {
                    eprintln!("Finish authorization in your browser: {url}");
                } else {
                    crate::fiote::session::laboratory_browser(world, live.root, &url);
                }
                let _ = cell::fiote::laboratory::open_browser(&url);
                live.browser_url = Some(url);
            }
        }
    }
    if live.running
        && live.verification.is_none()
        && live.pending_report.is_none()
        && let Some(prepared) = &live.prepared
    {
        let thread = prepared.report.thread.as_deref().unwrap();
        if crate::thread_castle::laboratory_submit(world, live.content, thread, HELLO) {
            let host = host.clone();
            let report = prepared.report.clone();
            let cancel = live.cancel.subscribe();
            live.verification = Some(tokio::spawn(async move {
                host.laboratory_verify(report, cancel).await
            }));
            world.get_mut::<Text>(live.status).unwrap().0 =
                "Waiting for a completed assistant reply…".into();
        } else if live
            .ready_at
            .is_some_and(|ready| ready.elapsed() > Duration::from_secs(30))
        {
            fail(
                &mut live,
                "sending",
                "The normal composer could not submit the greeting",
                "failed",
            );
        }
    }
    if live
        .verification
        .as_ref()
        .is_some_and(|task| task.is_finished())
    {
        let task = live.verification.take().unwrap();
        match tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(task)) {
            Ok(report) => {
                if report.persisted && report.outcome == "passed" {
                    live.pending_report = Some(report);
                    live.view_started = Some(Instant::now());
                    world.get_mut::<Text>(live.status).unwrap().0 =
                        "Verifying the reply reached the conversation view…".into();
                } else {
                    live.report = Some(report);
                    live.running = false;
                }
            }
            Err(_) => fail(&mut live, "inference", "Live test worker stopped", "failed"),
        }
    }
    if let Some(mut report) = live.pending_report.take() {
        let visible = crate::thread_castle::laboratory_visible(
            world,
            live.content,
            report.thread.as_deref().unwrap(),
            &report.reply,
        );
        if visible
            || live
                .view_started
                .is_some_and(|start| start.elapsed() > Duration::from_secs(10))
        {
            report.stage = "view".into();
            if visible {
                report.view = if live.headless {
                    "headless-component-state"
                } else {
                    "windowed-component-state"
                }
                .into();
                report.detail =
                    "Completed reply saved and delivered to the normal conversation view".into();
            } else {
                report.outcome = "failed".into();
                report.detail = "Saved reply did not reach the conversation view".into();
            }
            report.elapsed_ms = live.started.elapsed().as_millis() as u64;
            report.timings.push((
                "view".into(),
                live.view_started.unwrap().elapsed().as_millis() as u64,
            ));
            live.report = Some(report);
            live.running = false;
        } else {
            live.pending_report = Some(report);
        }
    }
    if let Some(report) = &live.report {
        world.get_mut::<Text>(live.status).unwrap().0 =
            format!("{} · {} · {}", report.outcome, report.stage, report.detail);
    }
    world.insert_resource(live);
}

impl Drop for Live {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
        if let Some(task) = self.preparation.take() {
            task.abort();
        }
        if let Some(task) = self.setup_task.take() {
            task.abort();
        }
    }
}

pub fn run_headless(runtime: cell::CellRuntime, options: Options) -> Report {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<InputFocus>()
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::app::CellHandle(runtime))
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::protein_area::ProteinAreaPlugin,
            crate::record_binding::RecordBindingPlugin,
            crate::fiote::session::Plugin,
            crate::thread_castle::ThreadCastlePlugin,
        ));
    let root = app.world_mut().spawn(Node::default()).id();
    mount(app.world_mut(), root, options, true);
    Control::Run.apply(app.world_mut(), root);
    let cancel = app.world().resource::<Live>().cancel.clone();
    let interrupt = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel.send_replace(true);
        }
    });
    app.finish();
    app.cleanup();
    loop {
        app.update();
        tick(app.world_mut());
        if let Some(report) = exported(app.world()) {
            interrupt.abort();
            return report.clone();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cell::fiote::laboratory::inference::{Message, Provider, Reply, ToolDefinition};

    struct Greeting;
    #[async_trait::async_trait]
    impl Provider for Greeting {
        async fn complete(
            &self,
            _: &str,
            messages: &[Message],
            tools: &[ToolDefinition],
        ) -> Result<Reply, String> {
            assert!(tools.is_empty());
            assert!(matches!(messages.last(),Some(Message::User(body)) if body==HELLO));
            Ok(Reply {
                text: "Hello from the test provider".into(),
                ..Default::default()
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn headless_workflow_uses_normal_send_and_displays_the_persisted_reply() {
        use std::sync::Arc;
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let directory = tempfile::tempdir().unwrap();
        let record = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Laboratory test".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let host = Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().join("fiote"))
                .await
                .unwrap()
                .with_provider(Arc::new(Greeting)),
        );
        let runtime = cell::CellRuntime {
            speech: None,
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            fiote: Some(host),
        };
        let mut session = runtime.local_session();
        let replies = session
            .handle(cell::ClientMessage::Fiote {
                id: "configure-test".into(),
                request: cell::FioteRequest::Configure {
                    record: record.clone(),
                    settings: cell::FioteSettings {
                        enabled: true,
                        provider: serde_json::from_value(serde_json::json!("openai")).unwrap(),
                        model: "fixture".into(),
                        ..Default::default()
                    },
                    api_key: Some(cell::FioteSecret("fixture-key".into())),
                    password: Some(cell::FioteSecret("fixture-password".into())),
                },
            })
            .await;
        assert!(
            !replies
                .iter()
                .any(|reply| matches!(reply, cell::ServerMessage::Error { .. })),
            "{replies:?}"
        );
        let report = tokio::task::block_in_place(|| {
            run_headless(
                runtime,
                Options {
                    record: Some(record),
                    ..Default::default()
                },
            )
        });
        assert_eq!(report.outcome, "passed", "{report:?}");
        assert_eq!(report.view, "headless-component-state");
        assert_eq!(report.reply, "Hello from the test provider");
        assert!(report.persisted);
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("fixture-key")
        );
    }

    #[test]
    fn live_test_is_explicit_and_controls_preserve_test_only_overrides() {
        let (mut app, root) = super::super::headless::stress_app();
        assert!(!app.world().contains_resource::<Live>());
        mount(app.world_mut(), root, Options::default(), false);
        assert!(!app.world().resource::<Live>().running);
        let model = app.world().resource::<Live>().model;
        app.world_mut()
            .get_mut::<bevy::text::EditableText>(model)
            .unwrap()
            .editor
            .set_text("chosen-model");
        let content = app.world().resource::<Live>().content;
        crate::thread_castle::populate(
            app.world_mut(),
            content,
            RecordBinding {
                area: content,
                uid: "record".into(),
                source: Source::Local,
            },
            &serde_json::json!({"uid":"record","threads":[]}),
        );
        assert!(
            app.world()
                .get::<crate::thread_castle::ThreadCastle>(content)
                .is_some()
        );
        Control::Separate.apply(app.world_mut(), root);
        Control::Run.apply(app.world_mut(), root);
        assert!(
            app.world()
                .get::<crate::thread_castle::ThreadCastle>(content)
                .is_none()
        );
        let live = app.world().resource::<Live>();
        assert!(live.running);
        assert!(live.options.separate);
        assert_eq!(live.options.model.as_deref(), Some("chosen-model"));
        Control::Cancel.apply(app.world_mut(), root);
        assert!(*app.world().resource::<Live>().cancel.borrow());
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Export report")
        );
    }
}
