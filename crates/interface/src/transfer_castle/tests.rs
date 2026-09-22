use super::*;

fn draft() -> Form {
    let mut form = model::composer("person-ana", "Sale");
    form.data["invitees"] = json!(["person-beto"]);
    form.data["promises"][0]["record"] = json!("record-bike");
    form.data["promises"][1]["record"] = json!("record-money");
    form
}

#[test]
fn presets_produce_typed_actions_and_validation_keeps_invalid_input() {
    for preset in model::PRESETS {
        let mut form = model::composer("person-ana", preset);
        form.data["invitees"] = json!(["person-beto"]);
        model::bind_preset(&mut form);
        for promise in form.data["promises"].as_array_mut().unwrap() {
            promise["record"] = json!("record-resource");
        }
        if preset == "Dependency plan" {
            form.data["dependencies"][0]["upstream"] = json!("transfer-parent");
        }
        let action = form.payload().unwrap();
        let parsed: engine::actions::Action = serde_json::from_value(action.clone()).unwrap();
        assert!(matches!(
            parsed,
            engine::actions::Action::CreateTransferDraft { .. }
        ));
        if matches!(preset, "Donation" | "Service" | "Information") {
            assert_eq!(action["promises"][0]["open"], true);
            assert!(action["promises"][0]["party"].is_null());
        }
        assert!(action["default_place"].is_null());
    }
    let mut form = draft();
    form.field("/promises/0/delta", "Quantity", model::FieldKind::Number);
    form.fields[0].value = "NaN".into();
    assert!(form.commit_fields().is_err());
    assert_eq!(form.fields[0].value, "NaN");
    assert_eq!(form.data["promises"][0]["delta"], -1);
    form.data["default_place"] = json!({"lat":95,"lon":0,"address":null});
    assert!(form.payload().is_err());
    form.data["default_place"] = Value::Null;
    form.data["invitees"] = json!(["person-ana"]);
    assert!(form.payload().is_err());
}

#[test]
fn signed_edit_preserves_creator_locations_dependencies_and_counteroffer_actor() {
    let row = json!({"uid":"transfer-1","revision":4,"head":"Projected name","agreement_type":"full",
        "revision_evidence":{"current":{"terms":{
            "transfer":{"head":"Signed name","slug":"sale","agreement_type":"full","visibility":"hidden","require_confirmation":true,"default_place":{"lat":1,"lon":2,"address":"Door"}},
            "parties":[{"kind":"participant","person_uid":"beto"},{"kind":"creator","person_uid":"ana"}],
            "promises":[{"uid":"p-1","record_uid":"resource","person_uid":"ana","delta":-2,"state":"proposed","reserve_from":"agreed","open_reuse_policy":"consume","location":{"address":"Home","lat":null,"lon":null}}],
            "dependencies":[{"uid":"d-1","scope":"transfer","promise_uid":null,"upstream_kind":"transfer","upstream_uid":"upstream","required_state":"kept"}]
        }}}, "invitations":[]});
    let form = model::edit(&row, true, "beto");
    let action = form.payload().unwrap();
    assert_eq!(action["person"], "beto");
    assert_eq!(action["expected_revision"], 4);
    assert_eq!(action["draft"]["creator"], "ana");
    assert_eq!(action["draft"]["head"], "Signed name");
    assert_eq!(action["draft"]["promises"][0]["place"]["address"], "Home");
    assert_eq!(action["draft"]["dependencies"][0]["upstream"], "upstream");
    assert!(serde_json::from_value::<engine::actions::Action>(action).is_ok());
}

#[test]
fn settlement_review_rejects_changed_quantity_identity_and_incomplete_evidence() {
    let form = Form::action(
        "Settle",
        json!({"action":"settle-transfer-occurrence","occurrence":"o-1","person":"ana","canonical_quantity":2}),
    );
    let preview = json!({"occurrence":"o-1","person":"ana","canonical_quantity":2,
        "capabilities":{"settle":true},"remaining_quantity":3,"local_delta":2,
        "application_formula_hash":"hash","application_formula_version":1,"remainder_policy":"visible",
        "expected_remaining_quantity":3,"expected_local_delta":2,"expected_application_formula_hash":"hash",
        "expected_application_formula_version":1,"expected_remainder_policy":"visible"});
    assert!(forms::reviewed_payload(&form, &preview, "ana").is_ok());
    let mut floating = preview.clone();
    floating["canonical_quantity"] = json!(2.0);
    assert!(forms::reviewed_payload(&form, &floating, "ana").is_ok());
    assert!(forms::reviewed_payload(&form, &preview, "beto").is_err());
    let mut changed = form.clone();
    changed.data["canonical_quantity"] = json!(1);
    assert!(forms::reviewed_payload(&changed, &preview, "ana").is_err());
    let mut missing = preview.clone();
    missing["expected_local_delta"] = Value::Null;
    assert!(forms::reviewed_payload(&form, &missing, "ana").is_err());
    let mut changed = preview.clone();
    changed["occurrence"] = json!("o-2");
    assert!(forms::reviewed_payload(&form, &changed, "ana").is_err());
}

fn app() -> App {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(TransferCastlePlugin);
    app
}

#[test]
fn native_inbox_forms_live_snapshots_and_workspace_roundtrip() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle::default(),
    );
    app.world_mut()
        .resource_mut::<Requests>()
        .0
        .insert("test-transfers".into(), (owner, "transfers".into()));
    let mut rows = vec![
        json!({"kind":"transfer_context","viewer":{"local":true},"capabilities":{"create":true}}),
    ];
    rows.extend((0..1000).map(|index| json!({"kind":"transfer","uid":format!("t-{index}"),"head":format!("Transfer {index}"),"revision":1,
        "inbox_facets":{"mine":index % 2 == 0,"active":true},"parties":[],"promises":[],"occurrences":[]})));
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::Snapshot {
            id: "test-transfers".into(),
            rows,
        }));
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().ready);
    let list = app.world().get::<View>(owner).unwrap().list;
    assert_eq!(app.world().get::<Children>(list).unwrap().len(), 31);
    ui::Command::Select("t-0".into()).apply(app.world_mut(), owner);
    ui::Command::New("Sale".into()).apply(app.world_mut(), owner);
    app.world_mut()
        .get_mut::<TransferCastle>(owner)
        .unwrap()
        .form = Some(draft());
    forms::render(app.world_mut(), owner);
    let before = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .clone();
    ui::render(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<TransferCastle>(owner).unwrap().form,
        before
    );
    for step in 0..5 {
        ui::Command::Step(step).apply(app.world_mut(), owner);
    }
    ui::Command::Step(2).apply(app.world_mut(), owner);
    ui::Command::Remove("promises".into(), 0).apply(app.world_mut(), owner);
    let form = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .as_ref()
        .unwrap();
    assert_eq!(array(&form.data, "promises").len(), 1);
    assert!(
        form.fields
            .iter()
            .all(|field| !field.path.starts_with("/promises/1/"))
    );
    assert!(form.payload().is_ok());
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
    let encoded = serde_json::to_string(&saved).unwrap();
    let restored: Vec<SavedTransferCastle> = serde_json::from_str(&encoded).unwrap();
    assert!(restored[0].valid());
    restored
        .into_iter()
        .next()
        .unwrap()
        .restore(app.world_mut(), root);
    assert_eq!(
        app.world_mut()
            .query::<&TransferCastle>()
            .iter(app.world())
            .count(),
        2
    );
    ui::Command::Cancel.apply(app.world_mut(), owner);
    ui::Command::Mine(true).apply(app.world_mut(), owner);
    ui::Command::Tree(true).apply(app.world_mut(), owner);
    assert_eq!(
        model::filtered(
            &app.world().get::<View>(owner).unwrap().rows,
            true,
            "all",
            "Transfer 998",
            "name"
        )
        .len(),
        1
    );
    let cycle = vec![
        json!({"uid":"a","parent":"b"}),
        json!({"uid":"b","parent":"a"}),
    ];
    assert_eq!(model::depth(&cycle[0], &cycle), 1);
}
