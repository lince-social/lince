use super::*;

#[cfg_attr(test, test)]
fn ordinary_decision_and_claim_forms_match_the_engine_actions() {
    let transfer = json!({"uid":"transfer","revision":2});
    for (action, fields) in [
        ("accept-transfer-invitation", json!({"invitation":"invite"})),
        ("reject-transfer-invitation", json!({"invitation":"invite"})),
        (
            "withdraw-transfer-invitation",
            json!({"invitation":"invite"}),
        ),
        (
            "reopen-transfer-invitation",
            json!({"invitation":"invite","expires_at":null}),
        ),
        ("set-transfer-agreement-level", json!({"level":2})),
        (
            "set-transfer-occurrence-claim",
            json!({"occurrence":"occurrence","role":"delivery","claimed":true}),
        ),
        (
            "set-transfer-occurrence-dispute",
            json!({"occurrence":"occurrence","disputed":true}),
        ),
        (
            "set-transfer-occurrence-application-formula",
            json!({"occurrence":"occurrence","formula":"incoming()"}),
        ),
        (
            "compensate-transfer-occurrence-settlement",
            json!({"settlement":"slice"}),
        ),
        ("create-transfer-thread", json!({"head":"Discussion"})),
        (
            "create-transfer-message",
            json!({"thread":"thread","body":"Selected results","references":[]}),
        ),
    ] {
        let mut payload = detail::action_base(&transfer, action, "ana");
        payload
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let form = Form::action(action, payload);
        serde_json::from_value::<engine::actions::Action>(form.payload().unwrap()).unwrap();
    }
}

#[cfg_attr(test, test)]
fn donation_and_trade_presets_describe_addressed_routes() {
    for (preset, count) in [("Donation", 1), ("Trade", 2), ("Sale", 2)] {
        let mut form = model::composer("ana", preset);
        form.data["invitees"] = json!(["beto"]);
        model::bind_preset(&mut form);
        let payload = form.payload().unwrap();
        let promises = payload["promises"].as_array().unwrap();
        assert_eq!(promises.len(), count);
        assert_eq!(promises[0]["item"]["exchange"]["giver"], "ana");
        assert_eq!(promises[0]["item"]["exchange"]["receiver"], "beto");
        for promise in promises {
            assert_eq!(promise["open"], false);
        }
        if count == 2 {
            assert_eq!(promises[1]["item"]["exchange"]["giver"], "beto");
            assert_eq!(promises[1]["item"]["exchange"]["receiver"], "ana");
            assert_eq!(promises[1]["record"], "");
        }
    }
}

fn draft() -> Form {
    let mut form = model::composer("person-ana", "Sale");
    form.data["invitees"] = json!(["person-beto"]);
    model::bind_preset(&mut form);
    form.data["promises"][0]["record"] = json!("record-bike");
    form.data["promises"][1]["record"] = json!("record-money");
    form
}

#[cfg_attr(test, test)]
fn item_text_and_disclosure_survive_composing_without_a_source() {
    let mut form = model::composer("person-ana", "Donation");
    form.data["promises"][0]["item"]["title"] = json!("City bike");
    form.data["promises"][0]["item"]["description"] = json!("Blue frame");
    let action = form.payload().unwrap();
    let typed: engine::actions::Action = serde_json::from_value(action).unwrap();
    let engine::actions::Action::CreateTransferDraft { promises, .. } = typed else {
        panic!("draft")
    };
    assert!(promises[0].record.is_empty());
    assert_eq!(promises[0].item.as_ref().unwrap().title, "City bike");
    form.data["promises"][0]["record"] = json!("secret-source");
    let preview = model::disclosure_preview(&form.data, None, true);
    assert!(!preview.to_string().contains("secret-source"));
    assert_eq!(preview["promises"][0]["title"], "City bike");
    assert_eq!(
        model::title(&json!({"uid":"p","disclosed":{"title":false}})),
        "Title unavailable"
    );
}

#[cfg_attr(test, test)]
fn counteroffer_keeps_unavailable_terms_out_of_editing_and_review() {
    let row = json!({"uid":"transfer","head":"Apples","revision":1,"agreement_type":"full",
        "promises":[{"uid":"p","title":"Apples","description":"Fresh",
            "disclosed":{"source":false,"parties":false,"quantity":false,"title":true,"description":true}}]});
    let mut form = model::edit(&row, true, "beto");
    form.field("/promises/0/delta", "Quantity", model::FieldKind::Number);
    form.field(
        "/promises/0/record",
        "Source",
        model::FieldKind::Reference("record".into()),
    );
    assert!(form.fields.is_empty());
    assert_eq!(form.data["promises"][0]["item"]["title"], "Apples");
    assert_eq!(
        model::review_draft(&form)["promises"][0]["delta"],
        "Unavailable; unchanged"
    );
    let payload = form.payload().unwrap();
    let action: engine::actions::Action = serde_json::from_value(payload).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::CounterofferTransfer { .. }
    ));
}

#[cfg_attr(test, test)]
fn explicit_routes_keep_their_identity_and_observers_stay_private() {
    let mut form = draft();
    form.data["promises"][0]["item"]["exchange"] =
        json!({"uid":"bike-route","giver":"person-ana","receiver":"person-beto"});
    form.data["promises"][0]["amount"] = json!(3);
    let payload = form.payload().unwrap();
    assert_eq!(payload["promises"][0]["delta"], -3.0);
    assert_eq!(payload["promises"][0]["party"], "person-ana");
    assert_eq!(
        payload["promises"][0]["item"]["exchange"]["uid"],
        "bike-route"
    );
    assert!(payload["promises"][0]["amount"].is_null());
    let observer = model::observer("dora", &json!({"uid":"road-transfer","head":"Road ready"}));
    let payload = observer.payload().unwrap();
    assert_eq!(payload["visibility"], "hidden");
    assert_eq!(payload["invitees"], json!([]));
    assert_eq!(payload["dependencies"][0]["upstream"], "road-transfer");
}

#[cfg_attr(test, test)]
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
        if matches!(preset, "Service" | "Information") {
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
    assert_eq!(form.data["promises"][0]["delta"].as_f64(), Some(-1.0));
    form.data["default_place"] = json!({"lat":95,"lon":0,"address":null});
    assert!(form.payload().is_err());
    form.data["default_place"] = Value::Null;
    form.data["invitees"] = json!(["person-ana"]);
    assert!(form.payload().is_err());
}

#[cfg_attr(test, test)]
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

#[cfg_attr(test, test)]
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
    let mut group = preview.clone();
    group["effects"] = json!([{"record":"bike"},{"record":"transport"}]);
    assert!(forms::reviewed_payload(&form, &group, "ana").is_err());
    group["expected_effects_hash"] = json!("reviewed-group");
    assert_eq!(
        forms::reviewed_payload(&form, &group, "ana").unwrap()["expected_effects_hash"],
        "reviewed-group"
    );
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

#[cfg_attr(test, test)]
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

#[cfg_attr(test, test)]
fn private_ratio_builds_only_the_private_policy_action() {
    let mut form = model::Form::action(
        "My accounting",
        serde_json::json!({
            "action":"set-transfer-private-application-policy","transfer":"transfer","exchange":"payment",
            "person":"owner","effects":[{"record":"money","formula":"-incoming()","mode":"quantity","private_ratio":"-0.5"}],
            "expected_version":0,"request_id":"policy"
        }),
    );
    let payload = form.payload().unwrap();
    assert_eq!(payload["effects"][0]["formula"], "incoming() * -0.5");
    assert!(payload["effects"][0].get("private_ratio").is_none());
    assert!(serde_json::from_value::<engine::actions::Action>(payload).is_ok());
    form.data["effects"][0]["private_ratio"] = serde_json::json!("");
    assert_eq!(
        form.payload().unwrap()["effects"][0]["formula"],
        "-incoming()"
    );
    form.data["effects"][0]["private_ratio"] = serde_json::json!("not a ratio");
    assert!(form.payload().is_err());
}

#[cfg_attr(test, test)]
fn grouped_need_editor_keeps_private_units_and_the_whole_review() {
    let mut form = model::Form::action(
        "This outcome meets",
        serde_json::json!({
            "action":"set-transfer-private-application-policy","transfer":"transfer","exchange":"bike","person":"beto",
            "effects":[{"record":"bike","formula":"incoming()","mode":"quantity"},
                {"record":"transport","formula":"incoming()","mode":"fulfilment"}],
            "expected_version":0,"request_id":"effects",
        }),
    );
    forms::effect_fields(&mut form);
    assert_eq!(form.fields.len(), 8);
    form.fields
        .iter_mut()
        .find(|field| field.path == "/effects/1/private_ratio")
        .unwrap()
        .value = "0.5".into();
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["effects"][0]["mode"], "quantity");
    assert_eq!(payload["effects"][1]["mode"], "fulfilment");
    assert_eq!(payload["effects"][1]["formula"], "incoming() * 0.5");
    assert!(!payload.to_string().contains("private_ratio"));
    assert!(serde_json::from_value::<engine::actions::Action>(payload).is_ok());
    form.data["effects"] = serde_json::json!([]);
    assert!(form.payload().is_err());
}

#[cfg_attr(test, test)]
fn cancellation_forms_keep_the_reviewed_exact_amount_and_revision() {
    for payload in [
        json!({"action":"propose-transfer-cancellation","transfer":"transfer","occurrence":"occurrence","expected_revision":4,"expected_remaining_quantity":{"scale":3,"value":"0.125"},"person":"owner","request_id":"propose"}),
        json!({"action":"apply-transfer-cancellation","transfer":"transfer","cancellation":"cancellation","expected_revision":5,"person":"owner","request_id":"complete"}),
    ] {
        let form = Form::action("Cancel remaining work", payload.clone());
        let reviewed = form.payload().unwrap();
        assert_eq!(reviewed["expected_revision"], payload["expected_revision"]);
        assert_eq!(
            reviewed["expected_remaining_quantity"],
            payload["expected_remaining_quantity"]
        );
        assert!(serde_json::from_value::<engine::actions::Action>(reviewed).is_ok());
    }
    let cancelled = json!({"uid":"transfer","primary_status":"cancelled","inbox_facets":{"cancelled_or_broken":true,"active":false,"completed":false}});
    assert_eq!(model::status(&cancelled), "cancelled");
}

#[cfg_attr(test, test)]
fn required_child_review_keeps_the_parent_revision_and_explicit_choice() {
    let action = json!({"action":"set-transfer-child-requirement", "transfer":"parent", "child":"child",
        "required":false, "expected_revision":9, "person":"ana", "request_id":null});
    let form = Form::action("Make optional and reset parent agreement", action);
    let reviewed = form.payload().unwrap();
    let parsed: engine::actions::Action = serde_json::from_value(reviewed).unwrap();
    assert!(
        matches!(parsed, engine::actions::Action::SetTransferChildRequirement {
        expected_revision:9, required:false, transfer, child, ..
    } if transfer == "parent" && child == "child")
    );
}

#[cfg_attr(test, test)]
fn observer_composer_keeps_private_commitments_and_offers_transfer_outcomes() {
    let mut form = model::composer("dora", "Dependency plan");
    form.data["agreement"] = json!("dependency");
    form.data["promises"] = json!([{"uid":"private", "party":"dora", "delta":1,
        "record":"", "item":{"title":"Road ready"}, "open":false}]);
    form.data["dependencies"] = json!([{"scope":"transfer", "upstream_kind":"transfer",
        "upstream":"road", "required_state":"kept"}]);
    form.step = Some(2);
    forms::composer_fields(&mut form);
    assert!(form.data["promises"][0]["item"]["exchange"].is_null());
    form.step = Some(3);
    forms::composer_fields(&mut form);
    assert_eq!(form.data["dependencies"][0]["required_state"], "settled");
    assert_eq!(
        form.fields
            .iter()
            .find(|field| field.path == "/dependencies/0/required_state")
            .unwrap()
            .kind,
        model::FieldKind::Choice(vec!["agreed".into(), "settled".into()])
    );
    let action: engine::actions::Action = serde_json::from_value(form.payload().unwrap()).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::CreateTransferDraft {
            agreement: nucleus::transfer::AgreementType::Dependency,
            ..
        }
    ));
}

#[cfg_attr(test, test)]
fn loan_proposals_are_reviewed_ordinary_actions() {
    let source = json!({"uid":"loan","revision":2});
    let item = json!({"title":"Bike","delta":1.0,"unit":null,"exchange":"bike-loan","giver":"ana","receiver":"beto","accepted_loan":{"origin":"origin","until_ms":1_800_000_000_000i64}});
    for future in [false, true] {
        let mut form = model::loan_proposal(&source, &item, "beto", future);
        forms::composer_fields(&mut form);
        let action: engine::actions::Action =
            serde_json::from_value(form.payload().unwrap()).unwrap();
        let engine::actions::Action::CreateTransferDraft { promises, .. } = action else {
            panic!("proposal")
        };
        let promise = &promises[0];
        let item = promise.item.as_ref().unwrap();
        assert_eq!(promise.open, future);
        assert_eq!(item.return_of.is_some(), !future);
        assert_eq!(item.future_need_for.is_some(), future);
        if !future {
            assert_eq!(item.exchange.as_ref().unwrap().giver, "beto");
            assert_eq!(item.exchange.as_ref().unwrap().receiver, "ana");
        }
    }
}

crate::laboratory_cases! {
    ordinary_decision_and_claim_forms_match_the_engine_actions,
    donation_and_trade_presets_describe_addressed_routes,
    item_text_and_disclosure_survive_composing_without_a_source,
    counteroffer_keeps_unavailable_terms_out_of_editing_and_review,
    explicit_routes_keep_their_identity_and_observers_stay_private,
    presets_produce_typed_actions_and_validation_keeps_invalid_input,
    signed_edit_preserves_creator_locations_dependencies_and_counteroffer_actor,
    settlement_review_rejects_changed_quantity_identity_and_incomplete_evidence,
    native_inbox_forms_live_snapshots_and_workspace_roundtrip,
    private_ratio_builds_only_the_private_policy_action,
    grouped_need_editor_keeps_private_units_and_the_whole_review,
    cancellation_forms_keep_the_reviewed_exact_amount_and_revision,
    required_child_review_keeps_the_parent_revision_and_explicit_choice,
    observer_composer_keeps_private_commitments_and_offers_transfer_outcomes,
    loan_proposals_are_reviewed_ordinary_actions,
}
