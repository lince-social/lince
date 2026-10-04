use super::*;
use bevy::ui::InteractionDisabled;
use nucleus::social::{PublicRequest, ServiceSettings};
use std::{collections::BTreeMap, sync::Arc};

struct Hosts(BTreeMap<String, Arc<engine::Engine>>);

#[derive(Resource, Default)]
struct InputTrace {
    focused: Vec<Entity>,
    activated: Vec<Entity>,
}

#[async_trait::async_trait]
impl engine::social::Network for Hosts {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        let host = self.0[destination].clone();
        let destination = destination.to_owned();
        tokio::spawn(async move {
            Box::pin(host.social_public_request(
                &destination,
                &destination,
                request,
                nucleus::execution::now().timestamp(),
            ))
            .await
        })
        .await
        .expect("Simulated public host stopped")
    }
}

fn social_form(world: &mut World, command: &str, matches: impl Fn(&Value) -> bool) -> Entity {
    world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .filter(|(_, form)| {
            form.payload["request"]["command"] == command && matches(&form.payload["request"])
        })
        .map(|(entity, _)| entity)
        .last()
        .unwrap_or_else(|| panic!("Missing native {command} form"))
}

async fn submit(app: &mut App, entity: Entity) {
    let confirmation = app
        .world()
        .get::<forms::Form>(entity)
        .unwrap()
        .confirmation
        .is_some();
    let button = app
        .world_mut()
        .query::<(
            Entity,
            &crate::actions::ActionButton,
            &crate::icons::Tooltip,
        )>()
        .iter(app.world())
        .find(|(_, button, label)| button.target == entity && label.0 != "Confirm")
        .unwrap()
        .0;
    assert!(app.world().get::<InteractionDisabled>(button).is_none());
    app.world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: button });
    app.update();
    if confirmation {
        let confirm = app
            .world_mut()
            .query::<(
                Entity,
                &crate::actions::ActionButton,
                &crate::icons::Tooltip,
            )>()
            .iter(app.world())
            .find(|(_, button, label)| button.target == entity && label.0 == "Confirm")
            .unwrap_or_else(|| {
                let form = app.world().get::<forms::Form>(entity);
                panic!(
                    "Missing confirmation; form is {}",
                    form.map_or("closed", |form| {
                        form.payload["action"].as_str().unwrap_or("unknown")
                    })
                )
            })
            .0;
        app.world_mut()
            .trigger(bevy::ui_widgets::Activate { entity: confirm });
        app.update();
    }
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            app.update();
            if app
                .world()
                .get::<forms::Form>(entity)
                .is_none_or(|form| form.pending.is_none())
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Native command did not finish");
}

async fn ui_person(secret: u8) -> (Arc<engine::Engine>, tempfile::TempDir, App, Entity) {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("root.key"), [secret; 32]).unwrap();
    engine.set_root_key_path(directory.path().join("root.key"));
    engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = engine.operational_key_for(&organ).await.unwrap();
    engine.set_signer(signer.clone()).await.unwrap();
    engine.set_organ_signer(signer).await.unwrap();
    let mut app = crate::sand_panel::tests::app();
    app.add_plugins((
        bevy::input::InputPlugin,
        crate::actions::ActionsPlugin,
        crate::accessibility::AccessibilityPlugin,
    ))
    .add_plugins(bevy::ui_widgets::ButtonPlugin)
    .add_plugins(OrganCastlePlugin);
    let root = app.world_mut().spawn_empty().id();
    let owner = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::Organ,
        "",
        bevy::math::DVec2::ZERO,
    );
    connect(&mut app, engine.clone());
    app.update();
    (engine, directory, app, owner)
}

async fn overview(app: &mut App) {
    let form = social_form(app.world_mut(), "overview", |_| true);
    let owner = app.world().get::<forms::Form>(form).unwrap().owner;
    Command::Page(6).apply(app.world_mut(), owner);
    submit(app, form).await;
}

async fn requests(app: &mut App) {
    let form = social_form(app.world_mut(), "requests", |_| true);
    let owner = app.world().get::<forms::Form>(form).unwrap().owner;
    Command::Page(6).apply(app.world_mut(), owner);
    submit(app, form).await;
}

async fn exchange(sender: &engine::Engine, receiver: &engine::Engine) {
    for table in [
        "social_private_destination",
        "social_message_work",
        "social_publication_job",
        "social_pickup_work",
    ] {
        store::sqlx::query(&format!("UPDATE {table} SET next_attempt=0"))
            .execute(&sender.store.pool)
            .await
            .unwrap();
        store::sqlx::query(&format!("UPDATE {table} SET next_attempt=0"))
            .execute(&receiver.store.pool)
            .await
            .unwrap();
    }
    sender.social_reconcile_private_admissions().await.unwrap();
    sender.social_prepare_messages_once().await.unwrap();
    for _ in 0..4 {
        sender.social_publish_once().await.unwrap();
    }
    for _ in 0..4 {
        sender.social_send_private_once().await.unwrap();
    }
    receiver.social_collect_private_once().await.unwrap();
}

async fn owner_wire(
    owner: &Arc<engine::Engine>,
    owner_ui: &mut App,
    owner_entity: Entity,
) -> Arc<engine::wire::Wire> {
    use engine::wire::{Reach, Wire};
    let wire = Arc::new(
        Wire::bind_with_discovery(
            owner.clone(),
            iroh::SecretKey::from_bytes(&[192; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    wire.serve_enrolment();
    for _ in 0..4 {
        owner_ui.update();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Command::Page(4).apply(owner_ui.world_mut(), owner_entity);
    let create = find(owner_ui.world_mut(), "roster-create-organ");
    submit(owner_ui, create).await;
    settle(owner_ui, |world| {
        world
            .iter_entities()
            .filter_map(|entity| entity.get::<forms::Form>())
            .any(|form| form.payload["action"] == "roster-enrol-token")
    })
    .await;
    wire
}

async fn enroll_history_and_revoke(
    owner: &Arc<engine::Engine>,
    owner_ui: &mut App,
    owner_entity: Entity,
    wire: &Arc<engine::wire::Wire>,
) {
    use engine::wire::{Reach, Wire};
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let server = wire.clone();
    let serving = tokio::spawn(async move { server.serve().await });
    Command::Page(4).apply(owner_ui.world_mut(), owner_entity);
    let issue = find(owner_ui.world_mut(), "roster-enrol-token");
    submit(owner_ui, issue).await;
    let code = owner_ui
        .world_mut()
        .query::<&Text>()
        .iter(owner_ui.world())
        .find(|text| text.0.starts_with("lincecell1|"))
        .unwrap()
        .0
        .clone();
    let (device, directory, mut device_ui, device_owner) = ui_person(191).await;
    std::fs::remove_file(directory.path().join("root.key")).unwrap();
    let mobile = Arc::new(
        Wire::bind_with_discovery(
            device.clone(),
            iroh::SecretKey::from_bytes(&[190; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    mobile.serve_enrolment();
    Command::Page(4).apply(device_ui.world_mut(), device_owner);
    let join = find(device_ui.world_mut(), "roster-join-organ");
    set(device_ui.world_mut(), join, "/code", &code);
    submit(&mut device_ui, join).await;
    assert_eq!(
        store::organs::local(&device.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        organ
    );
    device
        .set_signer(device.operational_key_for(&organ).await.unwrap())
        .await
        .unwrap();
    let syncing = mobile.clone();
    tokio::spawn(async move {
        syncing.sync_once().await.unwrap();
        syncing.sync_once().await.unwrap();
    })
    .await
    .unwrap();
    let original: String = store::sqlx::query_scalar(
        "SELECT uid FROM record WHERE body='Native offline introduction' AND kind='message'",
    )
    .fetch_one(&owner.store.pool)
    .await
    .unwrap();
    assert_eq!(
        store::records::get(&device.store.pool, &original)
            .await
            .unwrap()
            .unwrap()
            .body,
        "Native offline introduction"
    );
    requests(&mut device_ui).await;
    assert!(
        device_ui
            .world_mut()
            .query::<&forms::Form>()
            .iter(device_ui.world())
            .any(|form| form.payload["request"]["command"] == "send-private")
    );
    let device_cell = store::cells::local(&device.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    settle(owner_ui, |world| {
        world
            .get::<OrganCastle>(owner_entity)
            .unwrap()
            .rows
            .get("roster")
            .is_some_and(|rows| {
                rows.iter()
                    .find(|row| row["slug"] == "local-organ")
                    .is_some_and(|row| {
                        row["extension"]["cells"].as_array().is_some_and(|cells| {
                            cells.iter().any(|cell| cell["cell_uid"] == device_cell)
                        })
                    })
            })
    })
    .await;
    let revoke = owner_ui
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(owner_ui.world())
        .find(|(_, form)| {
            form.payload["action"] == "roster-revoke-cell"
                && form.payload["cell_uid"] == device_cell
        })
        .unwrap()
        .0;
    submit(owner_ui, revoke).await;
    let syncing = mobile.clone();
    tokio::spawn(async move { syncing.sync_once().await.unwrap() })
        .await
        .unwrap();
    assert!(
        owner
            .roster_of(&organ)
            .await
            .unwrap()
            .unwrap()
            .roster
            .cells
            .iter()
            .all(|cell| cell.cell_uid != device_cell)
    );
    let delivery = device.cell_delivery_status().await.unwrap();
    assert!(
        delivery["recovery"].is_string()
            || delivery["cells"]
                .as_array()
                .unwrap()
                .iter()
                .any(|cell| !cell["delivery"]["error"].is_null()),
        "{delivery}"
    );
    assert!(
        store::records::get(&device.store.pool, &original)
            .await
            .unwrap()
            .is_some()
    );
    mobile.shutdown().await;
    wire.shutdown().await;
    serving.abort();
    let _ = serving.await;
}

#[tokio::test]
async fn native_social_journey_covers_offline_delivery_enrollment_revocation_and_retained_block() {
    let (alice, _alice_dir, mut alice_ui, alice_owner) = ui_person(197).await;
    let (bob, _bob_dir, mut bob_ui, _) = ui_person(198).await;
    let wire = owner_wire(&alice, &mut alice_ui, alice_owner).await;
    let node = iroh::SecretKey::from_bytes(&[199; 32]).public().to_string();
    let host = tokio::spawn(async {
        let host = Arc::new(Box::pin(engine::Engine::open_memory()).await.unwrap());
        host.act(
            engine::actions::Action::Social {
                request: nucleus::social::Command::ConfigureServices {
                    settings: ServiceSettings {
                        directory: true,
                        mailbox: true,
                        ..Default::default()
                    },
                },
            },
            None,
        )
        .await
        .unwrap();
        host
    })
    .await
    .unwrap();
    let hosts = Arc::new(Hosts(BTreeMap::from([(node.clone(), host.clone())])));
    alice.attach_social_network(hosts.clone());
    bob.attach_social_network(hosts.clone());
    for (ui, name) in [(&mut alice_ui, "Alice"), (&mut bob_ui, "Bob")] {
        overview(ui).await;
        let profile = social_form(ui.world_mut(), "save-profile", |_| true);
        set(ui.world_mut(), profile, "/request/fields/name", name);
        submit(ui, profile).await;
    }
    let draft = social_form(alice_ui.world_mut(), "save-draft", |request| {
        request["record"].is_null()
    });
    set(
        alice_ui.world_mut(),
        draft,
        "/request/draft/title",
        "Native bicycle contribution",
    );
    set(
        alice_ui.world_mut(),
        draft,
        "/request/draft/destinations",
        &node,
    );
    submit(&mut alice_ui, draft).await;
    overview(&mut alice_ui).await;
    let keys = social_form(alice_ui.world_mut(), "prepare-reply-keys", |_| true);
    let record = alice_ui.world().get::<forms::Form>(keys).unwrap().payload["request"]["record"]
        .as_str()
        .unwrap()
        .to_owned();
    set(alice_ui.world_mut(), keys, "/request/services", &node);
    submit(&mut alice_ui, keys).await;
    let preview = social_form(alice_ui.world_mut(), "preview", |request| {
        request["record"] == record && request["state"] == "active"
    });
    submit(&mut alice_ui, preview).await;
    let publish = social_form(alice_ui.world_mut(), "publish", |request| {
        request["record"] == record
    });
    submit(&mut alice_ui, publish).await;
    for _ in 0..4 {
        alice.social_publish_once().await.unwrap();
    }
    let search = social_form(bob_ui.world_mut(), "search", |_| true);
    set(
        bob_ui.world_mut(),
        search,
        "/request/query/text",
        "Native bicycle",
    );
    set(bob_ui.world_mut(), search, "/request/services", &node);
    submit(&mut bob_ui, search).await;
    let introduction = social_form(bob_ui.world_mut(), "open-request", |_| true);
    set(
        bob_ui.world_mut(),
        introduction,
        "/request/text",
        "Native offline introduction",
    );
    set(
        bob_ui.world_mut(),
        introduction,
        "/request/alias",
        "Neighbor",
    );
    set(bob_ui.world_mut(), introduction, "/request/services", &node);
    submit(&mut bob_ui, introduction).await;
    for _ in 0..2 {
        bob.social_prepare_messages_once().await.unwrap();
        for _ in 0..4 {
            bob.social_publish_once().await.unwrap();
        }
    }
    bob.social_send_private_once().await.unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record WHERE body='Native offline introduction'"
        )
        .fetch_one(&alice.store.pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_service_envelope")
            .fetch_one(&host.store.pool)
            .await
            .unwrap()
            > 0
    );
    exchange(&bob, &alice).await;
    requests(&mut alice_ui).await;
    let accept = social_form(alice_ui.world_mut(), "decide-request", |request| {
        request["decision"] == "accept"
    });
    submit(&mut alice_ui, accept).await;
    for _ in 0..2 {
        exchange(&alice, &bob).await;
        exchange(&bob, &alice).await;
    }
    for ui in [&mut alice_ui, &mut bob_ui] {
        requests(ui).await;
        let reveal = social_form(ui.world_mut(), "reveal-profile", |_| true);
        submit(ui, reveal).await;
    }
    for _ in 0..2 {
        exchange(&alice, &bob).await;
        exchange(&bob, &alice).await;
    }
    for ui in [&mut alice_ui, &mut bob_ui] {
        requests(ui).await;
        let connect = social_form(ui.world_mut(), "connect-participant", |_| true);
        submit(ui, connect).await;
    }
    for _ in 0..2 {
        exchange(&alice, &bob).await;
        exchange(&bob, &alice).await;
    }
    for person in [&alice, &bob] {
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM record WHERE body='Native offline introduction' AND kind='message'").fetch_one(&person.store.pool).await.unwrap(), 1);
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
                .fetch_one(&person.store.pool)
                .await
                .unwrap(),
            0
        );
        let contacts = store::organs::contacts(&person.store.pool).await.unwrap();
        assert_eq!(contacts.len(), 1);
    }
    enroll_history_and_revoke(&alice, &mut alice_ui, alice_owner, &wire).await;
    alice.attach_social_network(hosts.clone());
    overview(&mut alice_ui).await;
    let withdrawal = social_form(alice_ui.world_mut(), "preview", |request| {
        request["record"] == record && request["state"] == "withdrawn"
    });
    submit(&mut alice_ui, withdrawal).await;
    let publish = social_form(alice_ui.world_mut(), "publish", |request| {
        request["document"]["state"] == "withdrawn"
    });
    submit(&mut alice_ui, publish).await;
    for _ in 0..4 {
        alice.social_publish_once().await.unwrap();
    }
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_search")
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        0
    );
    requests(&mut alice_ui).await;
    let block = social_form(alice_ui.world_mut(), "decide-request", |request| {
        request["decision"] == "block"
    });
    submit(&mut alice_ui, block).await;
    exchange(&alice, &bob).await;
    requests(&mut alice_ui).await;
    let unblock = social_form(alice_ui.world_mut(), "unblock-participant", |_| true);
    assert!(
        !alice_ui
            .world()
            .get::<forms::Form>(unblock)
            .unwrap()
            .payload["request"]["peer"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(store::sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM record WHERE body='Native offline introduction' AND kind='message'"
    ).fetch_one(&alice.store.pool).await.unwrap(), 1);
    let status_parent = alice_ui.world_mut().spawn_empty().id();
    report(
        alice_ui.world_mut(),
        status_parent,
        "Waiting for current owner authorization",
    );
    let statuses: Vec<_> = alice_ui
        .world_mut()
        .query::<&bevy::a11y::AccessibilityNode>()
        .iter(alice_ui.world())
        .filter(|node| node.role() == accesskit::Role::Status)
        .collect();
    assert!(
        statuses
            .iter()
            .any(|node| node.label() == Some("Waiting for current owner authorization"))
    );
    assert!(alice_ui.world().get::<OrganCastle>(alice_owner).is_some());
}

#[tokio::test]
async fn native_social_keyboard_submission_accessibility_and_disabled_buttons() {
    use bevy::{
        input::{
            ButtonState, InputSystems,
            keyboard::{Key, KeyboardInput, NativeKey},
        },
        input_focus::{InputFocus, InputFocusSystems, dispatch_focused_input},
        window::PrimaryWindow,
    };
    let (engine, _directory, mut app, owner) = ui_person(196).await;
    Command::Page(6).apply(app.world_mut(), owner);
    app.add_systems(
        PreUpdate,
        dispatch_focused_input::<KeyboardInput>
            .in_set(InputFocusSystems::Dispatch)
            .after(InputSystems),
    );
    app.init_resource::<InputTrace>()
        .add_observer(
            |event: On<bevy::input_focus::FocusedInput<KeyboardInput>>,
             mut trace: ResMut<InputTrace>| {
                trace.focused.push(event.focused_entity);
            },
        )
        .add_observer(
            |event: On<bevy::ui_widgets::Activate>, mut trace: ResMut<InputTrace>| {
                trace.activated.push(event.entity);
            },
        );
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update();
    let form = social_form(app.world_mut(), "save-draft", |_| true);
    set(
        app.world_mut(),
        form,
        "/request/draft/title",
        "Keyboard social publication draft",
    );
    let input = forms::input_text(app.world(), form, "/request/draft/title").unwrap();
    let accessible = app
        .world()
        .get::<bevy::a11y::AccessibilityNode>(input)
        .unwrap();
    assert_eq!(accessible.role(), accesskit::Role::TextInput);
    assert_eq!(accessible.label(), Some("Title"));
    let button = app
        .world_mut()
        .query::<(Entity, &crate::actions::ActionButton)>()
        .iter(app.world())
        .find(|(_, button)| button.target == form)
        .unwrap()
        .0;
    let accessible = app
        .world()
        .get::<bevy::a11y::AccessibilityNode>(button)
        .unwrap();
    assert_eq!(accessible.role(), accesskit::Role::Button);
    assert_eq!(accessible.label(), Some("Save announcement draft"));
    assert!(
        app.world()
            .get::<bevy::input_focus::tab_navigation::TabIndex>(button)
            .is_some()
    );
    app.insert_resource(InputFocus::from_entity(button));
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Enter,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
    assert!(
        app.world()
            .resource::<InputTrace>()
            .focused
            .contains(&button)
    );
    assert!(
        app.world()
            .resource::<InputTrace>()
            .activated
            .contains(&button)
    );
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            app.update();
            let saved: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM record_extension WHERE namespace='lince.social.publication' AND json_extract(fds,'$.draft.title')='Keyboard social publication draft'",
            )
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if saved == 1
                && app
                    .world()
                    .get::<forms::Form>(form)
                    .is_none_or(|form| form.pending.is_none())
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|error| {
        let labels: Vec<_> = app
            .world_mut()
            .query::<&bevy::a11y::AccessibilityNode>()
            .iter(app.world())
            .filter(|node| node.role() == accesskit::Role::Status)
            .filter_map(|node| node.label().map(str::to_owned))
            .collect();
        panic!("Keyboard submission did not save the native draft: {error}; {labels:?}");
    });
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record_extension WHERE namespace='lince.social.publication' AND json_extract(fds,'$.draft.title')='Keyboard social publication draft'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        1
    );
    app.world_mut()
        .entity_mut(button)
        .insert(InteractionDisabled);
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Space,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    app.update();
    assert!(
        app.world()
            .get::<forms::Form>(form)
            .unwrap()
            .pending
            .is_none()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM record_extension WHERE namespace='lince.social.publication' AND json_extract(fds,'$.draft.title')='Keyboard social publication draft'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap(),
        1
    );
    let inspect = social_form(app.world_mut(), "inspect-service", |_| true);
    set(
        app.world_mut(),
        inspect,
        "/request/endpoint",
        "invalid endpoint",
    );
    submit(&mut app, inspect).await;
    let output = app.world().get::<forms::Form>(inspect).unwrap().output;
    assert!(
        app.world()
            .get::<Children>(output)
            .unwrap()
            .iter()
            .any(|child| {
                app.world()
                    .get::<bevy::a11y::AccessibilityNode>(child)
                    .is_some_and(|node| {
                        node.role() == accesskit::Role::Status
                            && node
                                .label()
                                .is_some_and(|label| !label.is_empty() && label != "Working…")
                    })
            })
    );
    assert_eq!(
        forms::payload(app.world(), inspect).unwrap()["request"]["endpoint"],
        "invalid endpoint"
    );
}

#[tokio::test]
async fn native_offline_profile_edits_keep_conflicting_branches_until_explicit_review() {
    let (engine, _directory, mut app, _) = ui_person(195).await;
    let node = iroh::SecretKey::from_bytes(&[194; 32]).public().to_string();
    overview(&mut app).await;
    let initial = social_form(app.world_mut(), "save-profile", |_| true);
    set(
        app.world_mut(),
        initial,
        "/request/fields/name",
        "Original public name",
    );
    set(app.world_mut(), initial, "/request/destinations", &node);
    submit(&mut app, initial).await;
    overview(&mut app).await;
    let edit = social_form(app.world_mut(), "save-profile", |_| true);
    assert_eq!(
        app.world().get::<forms::Form>(edit).unwrap().payload["request"]["parents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    set(
        app.world_mut(),
        edit,
        "/request/fields/name",
        "First offline edit",
    );
    submit(&mut app, edit).await;
    set(
        app.world_mut(),
        edit,
        "/request/fields/name",
        "Concurrent offline edit",
    );
    submit(&mut app, edit).await;
    overview(&mut app).await;
    let review = social_form(app.world_mut(), "save-profile", |_| true);
    assert_eq!(
        app.world().get::<forms::Form>(review).unwrap().payload["request"]["parents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    set(
        app.world_mut(),
        review,
        "/request/fields/name",
        "Reviewed merged name",
    );
    submit(&mut app, review).await;
    assert!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_publication_job WHERE state='pending'"
        )
        .fetch_one(&engine.store.pool)
        .await
        .unwrap()
            > 0
    );
    let host = Arc::new(engine::Engine::open_memory().await.unwrap());
    host.act(
        engine::actions::Action::Social {
            request: nucleus::social::Command::ConfigureServices {
                settings: ServiceSettings {
                    directory: true,
                    ..Default::default()
                },
            },
        },
        None,
    )
    .await
    .unwrap();
    let network = Arc::new(Hosts(BTreeMap::from([(node.clone(), host.clone())])));
    engine.attach_social_network(network.clone());
    for _ in 0..8 {
        engine.social_publish_once().await.unwrap();
    }
    let profile: String =
        store::sqlx::query_scalar("SELECT body FROM social_document WHERE kind='profile'")
            .fetch_one(&host.store.pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&profile).unwrap()["fields"]["name"],
        "Reviewed merged name"
    );
}
