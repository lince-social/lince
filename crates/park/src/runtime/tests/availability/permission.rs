use super::*;
use bevy::{
    input::{
        keyboard::{Key, KeyCode},
        mouse::MouseButton,
    },
    prelude::Children,
};
use engine::{
    actions::Action,
    wire::{PeerAddr, Wire},
};
use serde_json::Value;

const DRAFT: &str = "permission-held-draft";
const FINAL: &str = "permission-held-draft-recovered";

struct Servers(Vec<tokio::task::JoinHandle<()>>);

impl Drop for Servers {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

fn address(wire: &Wire) -> PeerAddr {
    PeerAddr::new(wire.node_id()).with_ip_addr(std::net::SocketAddr::from((
        [127, 0, 0, 1],
        wire.endpoint()
            .bound_sockets()
            .into_iter()
            .next()
            .unwrap()
            .port(),
    )))
}

fn frame(worker: &CanvasWorker, serial: u64) -> Completed {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        if let Some(failure) = worker.failure() {
            panic!("The permission fixture worker failed: {failure}");
        }
        assert!(worker.is_running());
        if let Some(frame) = worker.frame()
            && frame.input_serial >= serial
        {
            assert!(frame.frame.ready().unwrap());
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "The permission fixture did not publish input {serial}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn input(worker: &CanvasWorker, input: Input) -> Completed {
    frame(worker, worker.input(input).unwrap())
}

fn capture(worker: &CanvasWorker, phase: &str) {
    pixels(&input(worker, Input::Test(Box::new(|_| {}))), phase);
}

fn inspect<T: Send + 'static>(
    worker: &CanvasWorker,
    inspect: impl FnOnce(&mut crate::presentation::Presentation) -> T + Send + 'static,
) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    input(
        worker,
        Input::Test(Box::new(move |canvas| {
            assert!(sender.send(inspect(canvas)).is_ok());
        })),
    );
    receiver.recv_timeout(Duration::from_secs(1)).unwrap()
}

fn point(world: &World, entity: Entity) -> Option<[f32; 2]> {
    visible(world, entity).then_some(())?;
    Some(
        native::topology::presentation::bounds(world, entity)?
            .intersect(Rect::from_corners(Vec2::ZERO, Vec2::new(1000.0, 700.0)))
            .center()
            .to_array(),
    )
}

#[derive(Debug)]
struct State {
    row: Option<Entity>,
    source: Option<Source>,
    status: String,
    status_visible: bool,
    body: Option<(Entity, String, bool)>,
}

struct Ui<'a> {
    worker: &'a CanvasWorker,
    area: Entity,
    uid: String,
}

impl Ui<'_> {
    fn state(&self, body: Option<Entity>) -> State {
        let area = self.area;
        let uid = self.uid.clone();
        inspect(self.worker, move |canvas| {
            let world = canvas.world_mut();
            let row = world
                .query::<(Entity, &RecordBinding)>()
                .iter(world)
                .find(|(entity, binding)| {
                    binding.area == area
                        && binding.uid == uid
                        && world.get::<native::canvas::CanvasItem>(*entity).is_some()
                })
                .map(|(entity, binding)| (entity, binding.source.clone()));
            let status = world
                .query::<(Entity, &Name, &Text)>()
                .iter(world)
                .find(|(entity, name, _)| {
                    name.as_str() == "record.availability.status"
                        && belongs(world, *entity, area, &uid)
                })
                .map(|(entity, _, text)| (text.0.clone(), visible(world, entity)));
            let body = world
                .query::<(Entity, &EditableText)>()
                .iter(world)
                .find(|(entity, text)| {
                    body.map_or_else(
                        || text.value().to_string() == BODY,
                        |expected| expected == *entity,
                    ) && belongs(world, *entity, area, &uid)
                })
                .map(|(entity, text)| (entity, text.value().to_string(), visible(world, entity)));
            State {
                row: row.as_ref().map(|(entity, _)| *entity),
                source: row.map(|(_, source)| source),
                status: status
                    .as_ref()
                    .map_or_else(String::new, |(text, _)| text.clone()),
                status_visible: status.is_some_and(|(_, visible)| visible),
                body,
            }
        })
    }

    fn wait(&self, phase: &str, body: Option<Entity>, expected: impl Fn(&State) -> bool) -> State {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            let state = self.state(body);
            if expected(&state) {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "Permission recovery did not reach {phase}: {state:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn click(&self, point: [f32; 2]) -> Completed {
        input(self.worker, Input::Pointer(point));
        input(
            self.worker,
            Input::Button(MouseButton::Left, true, point, 0),
        );
        input(
            self.worker,
            Input::Button(MouseButton::Left, false, point, 0),
        )
    }

    fn button(&self, caption: &'static str) {
        let deadline = Instant::now() + Duration::from_secs(40);
        let activated = Arc::new(AtomicUsize::new(0));
        let point = loop {
            let area = self.area;
            let uid = self.uid.clone();
            let observed = activated.clone();
            if let Some(point) = inspect(self.worker, move |canvas| {
                let world = canvas.world_mut();
                let entity = world
                    .query::<(Entity, &native::actions::ActionButton, &Children)>()
                    .iter(world)
                    .find(|(entity, _, children)| {
                        belongs(world, *entity, area, &uid)
                            && children.iter().any(|child| {
                                world
                                    .get::<Text>(*child)
                                    .is_some_and(|text| text.0 == caption)
                            })
                    })
                    .map(|(entity, _, _)| entity)?;
                let point = point(world, entity)?;
                world.entity_mut(entity).observe(
                    move |_: bevy::prelude::On<bevy::ui_widgets::Activate>| {
                        observed.fetch_add(1, Ordering::SeqCst);
                    },
                );
                Some(point)
            }) {
                break point;
            }
            assert!(
                Instant::now() < deadline,
                "The ordinary permission recovery control is not visible: {caption}"
            );
        };
        self.click(point);
        assert_eq!(
            activated.load(Ordering::SeqCst),
            1,
            "The intended Record control did not activate once: {caption}"
        );
    }

    fn focus_body(&self, body: Entity, phase: &str) {
        let point = inspect(self.worker, move |canvas| point(canvas.world_mut(), body))
            .expect("The captured description editor is not visibly reachable");
        eprintln!("Permission editor phase={phase} entity={body:?} point={point:?}");
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut completed = self.click(point);
        while completed
            .frame
            .input
            .editor
            .as_ref()
            .map(|editor| editor.owner)
            != Some(body.to_bits())
        {
            if Instant::now() >= deadline {
                pixels(&completed, &format!("permission-focus-{phase}"));
                let state = self.state(Some(body));
                let details = inspect(self.worker, move |canvas| {
                    let world = canvas.world_mut();
                    format!(
                        "focus={:?}; hit={:?}; node={:?}; transform={:?}; bounds={:?}",
                        world
                            .get_resource::<bevy::input_focus::InputFocus>()
                            .and_then(bevy::input_focus::InputFocus::get),
                        world
                            .get_resource::<native::topology::input::PointerState>()
                            .and_then(|state| state.hit),
                        world.get::<ComputedNode>(body),
                        world.get::<bevy::prelude::UiGlobalTransform>(body),
                        native::topology::presentation::bounds(world, body),
                    )
                });
                panic!(
                    "The clicked description never acquired its input identity: {phase}; point={point:?}; published={:?}; state={state:?}; {details}",
                    completed
                        .frame
                        .input
                        .editor
                        .as_ref()
                        .map(|editor| editor.owner)
                );
            }
            completed = input(self.worker, Input::Test(Box::new(|_| {})));
        }
    }

    fn replace(&self, text: &str) {
        let mut serial = 0;
        for (physical, logical, pressed) in [
            (KeyCode::ControlLeft, Key::Control, true),
            (KeyCode::KeyA, Key::Character("a".into()), true),
            (KeyCode::KeyA, Key::Character("a".into()), false),
            (KeyCode::ControlLeft, Key::Control, false),
        ] {
            serial = self
                .worker
                .input(Input::Key {
                    physical,
                    logical,
                    text: None,
                    pressed,
                    repeat: false,
                })
                .unwrap();
        }
        frame(self.worker, serial);
        for character in text.chars() {
            let physical = match character {
                'a' => KeyCode::KeyA,
                'c' => KeyCode::KeyC,
                'd' => KeyCode::KeyD,
                'e' => KeyCode::KeyE,
                'f' => KeyCode::KeyF,
                'h' => KeyCode::KeyH,
                'i' => KeyCode::KeyI,
                'l' => KeyCode::KeyL,
                'm' => KeyCode::KeyM,
                'n' => KeyCode::KeyN,
                'o' => KeyCode::KeyO,
                'p' => KeyCode::KeyP,
                'r' => KeyCode::KeyR,
                's' => KeyCode::KeyS,
                't' => KeyCode::KeyT,
                'v' => KeyCode::KeyV,
                '-' => KeyCode::Minus,
                _ => panic!("Unsupported permission fixture character: {character}"),
            };
            for pressed in [true, false] {
                serial = self
                    .worker
                    .input(Input::Key {
                        physical,
                        logical: Key::Character(character.to_string().into()),
                        text: pressed.then(|| character.to_string()),
                        pressed,
                        repeat: false,
                    })
                    .unwrap();
            }
        }
        frame(self.worker, serial);
    }
}

fn record(runtime: &tokio::runtime::Runtime, cell: &native::app::CellHandle, uid: &str) -> Value {
    runtime.block_on(async {
        let messages = cell
            .0
            .local_session()
            .handle(cell::ClientMessage::Subscribe {
                id: "verify-permission-source".into(),
                protein: native::full_record::config(uid, Source::Local)
                    .query()
                    .unwrap(),
            })
            .await;
        let mut rows = messages
            .into_iter()
            .find_map(|message| match message {
                cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
                _ => None,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        let row = rows.pop().unwrap();
        assert_eq!(row["uid"], uid);
        row
    })
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and private loopback QUIC"]
fn real_permission_refusal_and_refresh_keep_the_captured_source_and_native_draft() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let authority = provider(Arc::new(
        runtime.block_on(engine::Engine::open_memory()).unwrap(),
    ));
    let local = provider(Arc::new(
        runtime.block_on(engine::Engine::open_memory()).unwrap(),
    ));
    let uid = nucleus::new_uid("r");
    let other = nucleus::new_uid("r");
    let mut servers = Servers(Vec::new());
    let (source, role, update, person) = runtime.block_on(async {
        for (cell, record, head, body) in [
            (&authority, &uid, "Permission inspector", BODY),
            (
                &authority,
                &other,
                "Other remote Record",
                "Other remote body",
            ),
            (
                &local,
                &uid,
                "Local collision control",
                "Private local body",
            ),
        ] {
            cell.0
                .engine
                .act(
                    Action::CreateRecordDraft {
                        draft: engine::record_creation::Draft {
                            uid: record.clone(),
                            head: head.into(),
                            body: body.into(),
                            ..Default::default()
                        },
                    },
                    None,
                )
                .await
                .unwrap();
        }
        let authority_organ = store::organs::local(&authority.0.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let local_organ = store::organs::local(&local.0.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        servers.0.push(
            authority
                .0
                .start_loopback_peer(&directory.path().join("authority.key"))
                .await
                .unwrap(),
        );
        servers.0.push(
            local
                .0
                .start_loopback_peer(&directory.path().join("local.key"))
                .await
                .unwrap(),
        );
        let authority_wire = authority.0.wire.read().await.clone().unwrap();
        let local_wire = local.0.wire.read().await.clone().unwrap();
        for (cell, organ, wire) in [
            (&authority, &local_organ, &local_wire),
            (&local, &authority_organ, &authority_wire),
        ] {
            store::organs::add_contact(&cell.0.store.pool, organ, None, "Private test peer", "", 0)
                .await
                .unwrap();
            store::organs::set_node_id(
                &cell.0.store.pool,
                organ,
                Some(&wire.node_id().to_string()),
            )
            .await
            .unwrap();
            store::organs::set_trust(&cell.0.store.pool, organ, "known")
                .await
                .unwrap();
        }
        authority_wire.remember_addr(address(&local_wire));
        local_wire.remember_addr(address(&authority_wire));
        let person = authority
            .0
            .engine
            .act(
                Action::GrantOrganLogin {
                    organ: local_organ,
                    person_name: "Private inspector user".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let role = store::auth::ensure_role(&authority.0.store.pool, "park-private-inspector")
            .await
            .unwrap();
        let mut update = None;
        for action in ["read", "update"] {
            let permission =
                store::auth::ensure_permission(&authority.0.store.pool, "record", action)
                    .await
                    .unwrap();
            store::auth::grant(&authority.0.store.pool, role, permission)
                .await
                .unwrap();
            if action == "update" {
                update = Some(permission);
            }
        }
        store::auth::set_user_role(&authority.0.store.pool, &person, role)
            .await
            .unwrap();
        store::visibility::grant(&authority.0.store.pool, "actor", Some(&person), &uid)
            .await
            .unwrap();
        assert!(
            !authority
                .0
                .engine
                .record_text_permissions(Some(&person), &uid)
                .await
                .unwrap()
                .is_empty()
        );
        (
            Source::Organ(authority_organ),
            role,
            update.unwrap(),
            person,
        )
    });
    let unrelated = record(&runtime, &authority, &other);
    let collision = record(&runtime, &local, &uid);
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [1000, 700],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: Some(local.clone()),
    })
    .unwrap();
    let expected = uid.clone();
    let captured = source.clone();
    let area = inspect(&worker, move |canvas| {
        canvas.outputs(vec![output()]).unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, bevy::prelude::With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        let workspace = world
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        assert!(native::workspace_config::set_physics(
            world, root, workspace, false
        ));
        native::full_record::open(world, root, &expected, captured).unwrap()
    });
    let ui = Ui {
        worker: &worker,
        area,
        uid: uid.clone(),
    };
    let loaded = ui.wait("actual remote body", None, |state| state.body.is_some());
    assert_eq!(loaded.source, Some(source.clone()));
    let body = loaded.body.as_ref().unwrap().0;
    ui.button("Raw");
    ui.wait("visible Raw editor", Some(body), |state| {
        state.body.as_ref().is_some_and(|body| body.2)
    });
    ui.focus_body(body, "before-revocation");
    runtime
        .block_on(store::auth::revoke(&authority.0.store.pool, role, update))
        .unwrap();
    assert!(matches!(
        runtime.block_on(
            authority
                .0
                .engine
                .record_text_permissions(Some(&person), &uid)
        ),
        Err(engine::EngineError::Forbidden(_))
    ));
    ui.replace(DRAFT);
    let refused = ui.wait("real permission refusal", Some(body), |state| {
        state.status_visible && state.status.to_ascii_lowercase().contains("permission")
    });
    assert_eq!(refused.row, loaded.row);
    assert_eq!(refused.source, Some(source.clone()));
    assert_eq!(refused.body.as_ref().unwrap().1, DRAFT);
    assert_eq!(record(&runtime, &authority, &uid)["body"], BODY);
    capture(&worker, "permission-refused");
    runtime
        .block_on(store::auth::grant(&authority.0.store.pool, role, update))
        .unwrap();
    ui.button("Refresh Record");
    let recovered = ui.wait("fresh authority", Some(body), |state| {
        !state.status_visible
            && state
                .body
                .as_ref()
                .is_some_and(|body| body.1 == DRAFT && body.2)
    });
    assert_eq!(recovered.row, loaded.row);
    assert_eq!(recovered.source, Some(source));
    ui.focus_body(body, "after-refresh");
    ui.replace(FINAL);
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let stored = record(&runtime, &authority, &uid);
        if stored["body"] == FINAL {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Recovered ordinary typing did not save to the remote source: {stored:?}"
        );
        ui.state(Some(body));
    }
    let saved = ui.state(Some(body));
    assert_eq!(saved.body.as_ref().unwrap().1, FINAL);
    for (before, after) in [
        (&unrelated, record(&runtime, &authority, &other)),
        (&collision, record(&runtime, &local, &uid)),
    ] {
        for field in ["head", "body", "assertions"] {
            assert_eq!(after[field], before[field]);
        }
    }
    let serial = worker.input(Input::Focus(false)).unwrap();
    pixels(&frame(&worker, serial), "permission-recovered");
    worker.stop().unwrap();
    runtime.block_on(async {
        for cell in [&authority, &local] {
            if let Some(wire) = cell.0.wire.write().await.take() {
                wire.endpoint().close().await;
            }
        }
    });
}
