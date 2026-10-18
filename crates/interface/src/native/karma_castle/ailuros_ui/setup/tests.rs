use super::*;

fn fixture() -> (App, Entity) {
    let mut app = crate::native::sand_panel::tests::app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = app
        .world_mut()
        .spawn((
            crate::native::sand::InBox(root),
            crate::native::workspace::WorkspaceMember(1),
        ))
        .id();
    let panel = panel::column(app.world_mut(), root);
    app.world_mut().entity_mut(owner).insert(Devices {
        panel,
        rows: Vec::new(),
        audience: "00".repeat(32),
        pending: None,
        pending_save: false,
        editing: false,
        target: None,
        revision: None,
        enabled: false,
        fields: super::super::defaults(),
        issued_token: None,
    });
    spawn(app.world_mut(), owner, root);
    (app, owner)
}

fn set(world: &mut World, owner: Entity, index: usize, value: &str) {
    let entity = world.get::<Form>(owner).unwrap().fields[index];
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
    capture(world, owner);
}

fn enroll(world: &mut World, owner: Entity) -> Endpoint {
    let row = Endpoint {
        uid: "r_00000000000000000000000001".into(),
        name: "Camera".into(),
        public_key: "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296".into(),
        signal_uid: "r_00000000000000000000000002".into(),
        enabled: false,
        revision: 1,
        credential_epoch: 1,
        cooldown_seconds: 60,
        minimum: "0".into(),
        maximum: "100".into(),
        expires_at: 1900000000,
        last_sequence: 0,
        last_fingerprint: None,
        last_fact_uid: None,
        last_received_at: None,
        created_at: 1800000000,
    };
    set(world, owner, PORT, "/dev/selected-device");
    world
        .get_mut::<Devices>(owner)
        .unwrap()
        .rows
        .push(row.clone());
    let mut form = world.get_mut::<Form>(owner).unwrap();
    form.selected = Some(row.uid.clone());
    form.identity = Some(Identity {
        protocol: 1,
        chip: "esp32s3".into(),
        public_key: row.public_key.clone(),
        firmware: "lince-ailuros-v1".into(),
        provisioning: true,
        flash_bytes: 4 * 1024 * 1024,
        psram_bytes: 0,
    });
    row
}

fn displayed(world: &World, owner: Entity) -> String {
    world
        .get::<Text>(world.get::<Form>(owner).unwrap().status)
        .unwrap()
        .0
        .clone()
}

#[test]
fn loading_a_base_profile_clears_previous_wiring_and_confirmation() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let camera = lince_ailuros::profile::builtin("goouuu-esp32s3-cam-v1.5").unwrap();
    load_profile(world, owner, &camera);
    assert!(world.get::<Form>(owner).unwrap().values[PINS].contains("xclk"));
    world.get_mut::<Form>(owner).unwrap().confirmed_pins = true;
    world.get_mut::<Form>(owner).unwrap().artifact = Some(Artifact {
        version: 1,
        mode: lince_ailuros::project::FirmwareMode::Workflow,
        chip: "esp32s3".into(),
        idf: "5.5.2".into(),
        source_sha384: "00".repeat(48),
        images: Vec::new(),
    });
    load_profile(
        world,
        owner,
        &lince_ailuros::profile::builtin("esp32s3").unwrap(),
    );
    let form = world.get::<Form>(owner).unwrap();
    assert!(form.values[PINS].is_empty());
    assert!(!form.confirmed_pins);
    assert!(form.artifact.is_none());
}

#[test]
fn configuration_binds_confirmed_device_and_never_persists_the_password_editor_as_sand_text() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let row = enroll(world, owner);
    set(world, owner, SERVER, "https://cell.example");
    set(world, owner, SSID, "camera-network");
    set(world, owner, PASSWORD, "test-password");
    set(world, owner, KIND, "scalar");
    let config = configuration(world, owner).unwrap();
    assert_eq!(config.setup.device, row.uid);
    assert_eq!(config.setup.signal, row.signal_uid);
    assert_eq!(config.password, "test-password");
    let password = world.get::<Form>(owner).unwrap().fields[PASSWORD];
    assert!(crate::native::protected_text_input(world, password));
    assert_eq!(world.get::<TextColor>(password).unwrap().0, Color::NONE);
    assert!(
        world
            .get::<crate::native::sand_text::SandText>(password)
            .is_none()
    );
    set(world, owner, PORT, "/dev/different-device");
    assert!(configuration(world, owner).is_err());
    assert!(world.get::<Form>(owner).unwrap().identity.is_none());
}

#[test]
fn changing_camera_wiring_requires_confirmation_again() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    enroll(world, owner);
    set(world, owner, SERVER, "https://cell.example");
    set(world, owner, SSID, "camera-network");
    set(
        world,
        owner,
        PINS,
        r#"{"data":[11,9,8,10,12,18,17,16],"xclk":15,"pclk":13,"vsync":6,"href":7,"sda":4,"scl":5,"pwdn":-1,"reset":-1}"#,
    );
    assert!(configuration(world, owner).is_err());
    apply(world, owner, &Control::ConfirmPins).unwrap();
    assert!(configuration(world, owner).is_ok());
    set(
        world,
        owner,
        PINS,
        r#"{"data":[11,9,8,10,12,18,17,16],"xclk":14,"pclk":13,"vsync":6,"href":7,"sda":4,"scl":5,"pwdn":-1,"reset":-1}"#,
    );
    assert!(!world.get::<Form>(owner).unwrap().confirmed_pins);
    assert!(configuration(world, owner).is_err());
    apply(world, owner, &Control::ConfirmPins).unwrap();
    set(world, owner, PROJECT, "/tmp/another-firmware-project");
    assert!(!world.get::<Form>(owner).unwrap().confirmed_pins);
    apply(world, owner, &Control::ConfirmPins).unwrap();
    set(world, owner, PORT, "/dev/another-camera");
    assert!(!world.get::<Form>(owner).unwrap().confirmed_pins);
}

#[test]
fn signal_preparation_waits_for_matching_confirmation_and_rejects_unrelated_responses() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let row = enroll(world, owner);
    world.get_mut::<Form>(owner).unwrap().pending = Some(("prepare".into(), Backend::Configure));
    assert!(!receive(
        world,
        &ServerMessage::ActionOk {
            id: "other".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: None
        }
    ));
    assert!(world.get::<Form>(owner).unwrap().streams.is_empty());
    let stream = store::signal_streams::Configuration {
        signal_uid: row.signal_uid,
        value_type: "jpeg".into(),
        source_url: String::new(),
        revision: 1,
    };
    let response = ServerMessage::ActionOk {
        id: "prepare".into(),
        created: None,
        facts: 0,
        warnings: Vec::new(),
        data: Some(serde_json::json!({"stream":stream})),
    };
    assert!(receive(world, &response));
    let form = world.get::<Form>(owner).unwrap();
    assert!(form.pending.is_none());
    assert_eq!(form.streams.len(), 1);
    assert_eq!(form.streams[0].value_type, "jpeg");
}

#[test]
fn stale_worker_results_and_isolated_hardware_actions_do_not_touch_the_selected_device() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let row = enroll(world, owner);
    let identity = world.get::<Form>(owner).unwrap().identity.clone().unwrap();
    world.get_mut::<Form>(owner).unwrap().job = Some(Job {
        cancellation: Cancellation::default(),
        reply: Arc::new(Mutex::new(Some(Ok(Reply::Identity(identity))))),
        port: "/dev/old-device".into(),
        project: String::new(),
    });
    maintain(world);
    assert!(displayed(world, owner).contains("selection changed"));
    assert_eq!(selected(world, owner).unwrap().uid, row.uid);
    world
        .entity_mut(owner)
        .insert(crate::native::laboratory::LaboratoryOwner);
    assert!(crate::native::laboratory::blocked(world, owner));
    assert!(
        apply(world, owner, &Control::Flash)
            .unwrap_err()
            .contains("unavailable")
    );
    assert!(world.get::<Form>(owner).unwrap().job.is_none());
}

#[test]
fn worker_failure_preserves_drafts_and_cancellation_reaches_the_running_job() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    set(world, owner, PROJECT, "/tmp/edited-firmware");
    set(world, owner, PASSWORD, "draft-password");
    let cancellation = Cancellation::default();
    world.get_mut::<Form>(owner).unwrap().job = Some(Job {
        cancellation: cancellation.clone(),
        reply: Arc::new(Mutex::new(None)),
        port: String::new(),
        project: "/tmp/edited-firmware".into(),
    });
    Control::Cancel.apply(world, owner);
    assert!(cancellation.check().is_err());
    let reply = world
        .get::<Form>(owner)
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .reply
        .clone();
    *reply.lock().unwrap() = Some(Err("Compilation failed".into()));
    maintain(world);
    assert_eq!(
        world.get::<Form>(owner).unwrap().values[PROJECT],
        "/tmp/edited-firmware"
    );
    assert_eq!(displayed(world, owner), "Compilation failed");
    assert_eq!(
        world.get::<Form>(owner).unwrap().values[PASSWORD],
        "draft-password"
    );
    clear_confirmed_password(world, owner, "an-earlier-password");
    assert_eq!(
        world.get::<Form>(owner).unwrap().values[PASSWORD],
        "draft-password"
    );
    clear_confirmed_password(world, owner, "draft-password");
    assert!(world.get::<Form>(owner).unwrap().values[PASSWORD].is_empty());
}

fn provisioning_reply(world: &mut World, owner: Entity, row: &Endpoint) {
    let form = world.get::<Form>(owner).unwrap();
    let job = Job {
        cancellation: Cancellation::default(),
        reply: Arc::new(Mutex::new(Some(Ok(Reply::Provisioned {
            password: "submitted-password".into(),
            endpoint: row.uid.clone(),
            revision: row.revision,
        })))),
        port: form.values[PORT].clone(),
        project: form.values[PROJECT].clone(),
    };
    world.get_mut::<Form>(owner).unwrap().job = Some(job);
}

#[test]
fn provisioning_completion_cannot_enable_access_changed_while_usb_was_busy() {
    for change in 0..4 {
        let (mut app, owner) = fixture();
        let world = app.world_mut();
        let row = enroll(world, owner);
        set(world, owner, PASSWORD, "submitted-password");
        provisioning_reply(world, owner, &row);
        {
            let mut state = world.get_mut::<Devices>(owner).unwrap();
            let changed = &mut state.rows[0];
            changed.revision += 1;
            match change {
                0 => changed.enabled = false,
                1 => changed.public_key = format!("02{}", "11".repeat(32)),
                2 => changed.signal_uid = "r_00000000000000000000000003".into(),
                _ => state.rows.clear(),
            }
        }
        maintain(world);
        assert!(world.get::<Form>(owner).unwrap().pending.is_none());
        assert!(!displayed(world, owner).contains("Waiting for"));
        assert!(!displayed(world, owner).contains("access confirmed"));
        if change == 0 || change == 2 {
            assert!(displayed(world, owner).contains("access changed"));
        }
        assert!(world.get::<Form>(owner).unwrap().values[PASSWORD].is_empty());
    }
}

#[test]
fn asynchronous_provisioning_completion_preserves_a_newer_password_draft() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let row = enroll(world, owner);
    provisioning_reply(world, owner, &row);
    set(world, owner, PASSWORD, "newer-password");
    world.get_mut::<Devices>(owner).unwrap().rows[0].revision += 1;
    maintain(world);
    assert_eq!(
        world.get::<Form>(owner).unwrap().values[PASSWORD],
        "newer-password"
    );
    assert!(displayed(world, owner).contains("access changed"));
}

#[test]
fn cancellation_before_completion_is_consumed_never_enables_access() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let row = enroll(world, owner);
    set(world, owner, PASSWORD, "submitted-password");
    provisioning_reply(world, owner, &row);
    Control::Cancel.apply(world, owner);
    maintain(world);
    assert!(world.get::<Form>(owner).unwrap().pending.is_none());
    assert!(!selected(world, owner).unwrap().enabled);
    assert!(displayed(world, owner).contains("cancelled"));
    assert_eq!(
        world.get::<Form>(owner).unwrap().values[PASSWORD],
        "submitted-password"
    );
}

#[test]
fn enable_success_requires_a_confirmed_enabled_endpoint_and_backend_errors_remain_visible() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    let mut row = enroll(world, owner);
    for enabled in [false, true] {
        row.enabled = enabled;
        world.get_mut::<Form>(owner).unwrap().pending = Some(("enable".into(), Backend::Enable));
        assert!(receive(
            world,
            &ServerMessage::ActionOk {
                id: "enable".into(),
                created: None,
                facts: 0,
                warnings: Vec::new(),
                data: Some(serde_json::json!({"endpoints":[row], "audience":"00".repeat(32)})),
            }
        ));
        assert_eq!(
            displayed(world, owner).contains("access confirmed"),
            enabled
        );
    }
    world.get_mut::<Form>(owner).unwrap().pending = Some(("denied".into(), Backend::Configure));
    assert!(receive(
        world,
        &ServerMessage::Error {
            id: "denied".into(),
            message: "Permission refused".into(),
            code: None,
        }
    ));
    assert_eq!(displayed(world, owner), "Permission refused");
    assert!(world.get::<Form>(owner).unwrap().pending.is_none());
}

#[test]
fn incomplete_or_wrong_signal_confirmations_do_not_report_configuration_success() {
    let (mut app, owner) = fixture();
    let world = app.world_mut();
    enroll(world, owner);
    for data in [
        serde_json::json!({}),
        serde_json::json!({"stream":null}),
        serde_json::json!({"stream":{
            "signal_uid":"r_00000000000000000000000003", "value_type":"jpeg", "source_url":"", "revision":1,
        }}),
    ] {
        world.get_mut::<Form>(owner).unwrap().pending =
            Some(("prepare".into(), Backend::Configure));
        assert!(receive(
            world,
            &ServerMessage::ActionOk {
                id: "prepare".into(),
                created: None,
                facts: 0,
                warnings: Vec::new(),
                data: Some(data),
            }
        ));
        assert!(!displayed(world, owner).contains("configuration confirmed"));
    }
    assert!(world.get::<Form>(owner).unwrap().streams.is_empty());
}

fn setup_responses(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::native::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        receive(world, &message);
    }
}

#[tokio::test]
async fn confirmed_enrollment_provisioning_and_enable_open_the_same_signal_in_video_castle() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    store::organs::ensure_local(&engine.store.pool, "https://camera.test")
        .await
        .unwrap();
    let (mut app, owner) = fixture();
    app.init_resource::<Assets<Image>>();
    crate::native::sand_panel::tests::connect(&mut app, engine.clone());
    app.add_systems(
        Update,
        (setup_responses, maintain)
            .chain()
            .after(crate::native::cell_bridge::ReceiveCell),
    );
    app.update();
    {
        let world = app.world_mut();
        enroll(world, owner);
        world.get_mut::<Devices>(owner).unwrap().rows.clear();
        world.get_mut::<Form>(owner).unwrap().selected = None;
        apply(world, owner, &Control::Enroll).unwrap();
    }
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        let form = world.get::<Form>(owner).unwrap();
        form.pending.is_none()
            && form
                .streams
                .iter()
                .any(|stream| stream.value_type == "jpeg")
    })
    .await;
    let row = selected(app.world(), owner).unwrap();
    assert!(!row.enabled);
    let stream = engine
        .stream_channel(&row.signal_uid, None)
        .await
        .unwrap()
        .config
        .clone();
    assert_eq!(stream.value_type, "jpeg");
    assert!(stream.source_url.is_empty());
    provisioning_reply(app.world_mut(), owner, &row);
    maintain(app.world_mut());
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world.get::<Form>(owner).unwrap().pending.is_none()
            && selected(world, owner).is_ok_and(|row| row.enabled)
    })
    .await;
    assert!(displayed(app.world(), owner).contains("access confirmed"));
    apply(app.world_mut(), owner, &Control::OpenVideo).unwrap();
    let sources: Vec<_> = app
        .world_mut()
        .query::<&crate::native::video_castle::Settings>()
        .iter(app.world())
        .map(|settings| settings.source.clone())
        .collect();
    assert_eq!(sources, vec![row.signal_uid]);
    assert!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .any(|text| text.value().to_string() == sources[0])
    );
}
