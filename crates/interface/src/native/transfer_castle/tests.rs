use super::*;

#[cfg_attr(test, test)]
fn transfer_party_opens_the_actual_person_experience_without_discovery() {
    let mut app = app();
    let root = app.world_mut().spawn(crate::native::workspace::Workspaces::default()).id();
    let person = nucleus::new_uid("r");
    let owner = spawn(app.world_mut(), root, 1, DVec2::ZERO, TransferCastle {person:person.clone(), ..Default::default()});
    app.world_mut().get_mut::<View>(owner).unwrap().context = json!({"viewer":{"local":true}});
    let parent = app.world().get::<View>(owner).unwrap().detail;
    let actor = nucleus::new_uid("r");
    detail::render(app.world_mut(), owner, parent, &json!({"uid":nucleus::new_uid("r"),"revision":1,"head":"One journey","parties":[{"actor":actor,"actor_head":"Driver"}],"promises":[],"occurrences":[]}));
    crate::native::trust_castle::tests::assert_subject_entry(app.world_mut(), parent, nucleus::trust_analysis::Identity::Person(actor));
}

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
        if matches!(preset, "Ride" | "Delivery") {
            form.data["promises"][0]["item"]["transport"]["origin"] = json!(nucleus::new_uid("r"));
            form.data["promises"][0]["item"]["transport"]["destination"] = json!(nucleus::new_uid("r"));
            let now = nucleus::execution::now();
            form.data["promises"][0]["window_start"] = json!((now - chrono::Duration::minutes(5)).to_rfc3339());
            form.data["promises"][0]["window_end"] = json!((now + chrono::Duration::minutes(30)).to_rfc3339());
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

pub(super) fn app() -> App {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .add_plugins(TransferCastlePlugin);
    app
}

#[cfg_attr(test, test)]
fn native_inbox_forms_live_snapshots_and_workspace_roundtrip() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
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

#[cfg_attr(test, test)]
fn outcome_forms_keep_exact_amounts_separate_from_completion_and_settlement() {
    use nucleus::transfer::outcome::{Measure, Request, Role};
    let concept = nucleus::new_uid("c");
    let definition = nucleus::vocabulary::DefinitionVersion {
        concept_uid: concept,
        revision: 1,
        definition: "Measured portions".into(),
        created_at: String::new(),
    };
    let measure = Measure {
        meaning: nucleus::vocabulary::reference::MeaningReference::from_definition(
            &nucleus::new_uid("g"),
            &definition,
        )
        .unwrap(),
        unit: None,
        object: None,
    };
    let transfer = json!({"uid":nucleus::new_uid("r"),"revision":2});
    let occurrence = json!({"uid":nucleus::new_uid("to")});
    let mut form = outcomes::statement_form(
        &transfer,
        &occurrence,
        &nucleus::new_uid("r"),
        "Report received amount",
        Request::State {
            role: Role::Receiver,
            measure,
            amount: "0".into(),
            withdrawn: false,
            need_binding: None,
            supersedes: None,
        },
    );
    form.fields[0].value = "9007199254740993.000000000000000001".into();
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(
        payload["request"]["amount"],
        "9007199254740993.000000000000000001"
    );
    assert_eq!(payload["expected_revision"], 2);
    assert!(payload.get("claimed").is_none());
    assert!(payload.get("canonical_quantity").is_none());
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
}

#[cfg_attr(test, test)]
fn receipt_review_forms_preserve_the_exact_scope_and_require_explicit_approval() {
    let transfer = json!({"uid":nucleus::new_uid("r"),"revision":2});
    let outcome = nucleus::new_uid("r");
    let occurrence = json!({"uid":nucleus::new_uid("toc"),"outcomes":{"entries":[{"evidence":{"record":outcome}}]}});
    let mut form = receipts::prepare_form(&transfer, &occurrence, "ana");
    assert!(
        form.review["Attribution"]
            .as_str()
            .unwrap()
            .contains("Sharing approval alone does not establish historical Organ representation")
    );
    form.fields[0].value = "2027-04-01T00:00:00Z".into();
    form.fields[1].value = "false".into();
    form.fields[3].value = "need, receiver".into();
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["scope"]["expected_revision"], 2);
    assert_eq!(
        payload["request"]["scope"]["outcome_records"],
        json!([outcome])
    );
    assert_eq!(
        payload["request"]["scope"]["publish_agreed_quantity"],
        false
    );
    assert_eq!(
        payload["request"]["scope"]["stages"],
        json!(["need", "receiver"])
    );
    assert_eq!(
        payload["request"]["scope"]["expires_at"],
        chrono::DateTime::parse_from_rfc3339("2027-04-01T00:00:00Z")
            .unwrap()
            .timestamp()
    );
    assert!(payload.get("claimed").is_none());
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let form = receipts::action_form(
        "ana",
        "Approve receiver sharing",
        json!({"kind":"approve","record":"review","expected_version":3,"role":"receiver","authority_record":"authority"}),
    );
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["expected_version"], 3);
    assert_eq!(payload["request"]["role"], "receiver");
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let transfer = json!({"revision_evidence":{"current":{"terms":{"parties":[
        {"person_uid":"ana","organ_uid":"original-organ","kind":"creator"}
    ]}}}});
    let form = detail::party_form(
        "Agree",
        json!({"action":"set-transfer-agreement-level"}),
        &transfer,
    );
    assert_eq!(
        form.review["Recorded parties"][0]["organ_uid"],
        "original-organ"
    );
    assert!(
        form.review["Association"]
            .as_str()
            .unwrap()
            .contains("new terms")
    );
}

#[cfg_attr(test, test)]
fn receipt_source_forms_keep_frozen_terms_and_exact_reports_separate_from_publication() {
    let receipt = json!({"record":nucleus::new_uid("r"),"version":1});
    let measure = json!({"meaning":{"vocabulary_uid":nucleus::new_uid("g"),"concept_uid":nucleus::new_uid("c"),"revision":1,"definition_hash":"a".repeat(64)},"unit":null,"object":null});
    let selection = json!({"need":nucleus::new_uid("r"),"expected_need_revision":2,"assertion":nucleus::new_uid("a"),"vocabulary":measure["meaning"]["vocabulary_uid"],"unit_vocabulary":null});
    let source = json!({"kind":"bind_need","selection":selection,"expected_measure":measure,"supersedes":null});
    let mut form = receipts::source_form(
        "ana",
        &receipt,
        source.clone(),
        json!(nucleus::new_uid("r")),
        Value::Null,
    );
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["request"]["outcome"], source);
    assert!(
        form.review["Privacy"]
            .as_str()
            .unwrap()
            .contains("does not share it")
    );
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let report = json!({"kind":"state","role":"receiver","measure":measure,"amount":"12.000000000000000001","withdrawn":false,"need_binding":nucleus::new_uid("r"),"supersedes":null});
    let mut form = receipts::source_form(
        "ana",
        &receipt,
        report,
        json!(nucleus::new_uid("r")),
        Value::Null,
    );
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(
        payload["request"]["request"]["outcome"]["amount"],
        "12.000000000000000001"
    );
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let public = nucleus::transfer::receipt::Measurement {
        meaning: serde_json::from_value(measure["meaning"].clone()).unwrap(),
        unit: None,
        object_commitment: None,
    };
    let mut sharing = receipts::prepare_form(
        &json!({"uid":nucleus::new_uid("r"),"revision":1}),
        &json!({"uid":nucleus::new_uid("toc")}),
        "ana",
    );
    sharing
        .fields
        .iter_mut()
        .find(|field| field.path == "/request/scope/public_measurements")
        .unwrap()
        .value = serde_json::to_string(&vec![public.clone()]).unwrap();
    sharing.commit_fields().unwrap();
    let payload = sharing.payload().unwrap();
    assert_eq!(
        payload["request"]["scope"]["public_measurements"],
        json!([public])
    );
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
}

#[cfg_attr(test, test)]
fn receipt_delivery_forms_review_all_recipients_and_keep_send_separate_from_import() {
    let inbox = receipts::action_form(
        "ana",
        "Receipt inbox",
        json!({"kind":"delivery","request":{"kind":"inbox"}}),
    );
    serde_json::from_value::<engine::actions::Action>(inbox.payload().unwrap()).unwrap();
    let older = receipt_delivery::inbox_form("ana", Some("opaque-continuation"));
    let older = older.payload().unwrap();
    assert_eq!(older["request"]["request"]["before"], "opaque-continuation");
    serde_json::from_value::<engine::actions::Action>(older).unwrap();
    let newest = receipt_delivery::inbox_form("ana", None).payload().unwrap();
    assert!(newest["request"]["request"]["before"].is_null());
    let source = json!({"kind":"document","record":nucleus::new_uid("r"),"expected_version":3});
    let mut recipients = vec![nucleus::new_uid("r"), nucleus::new_uid("r")];
    recipients.sort();
    let thread = json!({"thread":nucleus::new_uid("r"),"recipients":recipients});
    let mut form = receipt_delivery::preview_form("ana", &source, &thread, &Value::Null);
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(
        payload["request"]["request"]["request"]["recipients"],
        thread["recipients"]
    );
    assert_eq!(payload["request"]["request"]["request"]["source"], source);
    serde_json::from_value::<engine::actions::Action>(payload.clone()).unwrap();
    form.fields[0].value = "invalid date".into();
    form.commit_fields().unwrap();
    assert!(form.payload().is_err());
    let hash = nucleus::karma::canonical_hash("lince.delivery-test.v1", &"reviewed").unwrap();
    let public =
        json!({"recipients":recipients,"artifact":{"kind":"document","value":{"kind":"review"}}});
    let preview = json!({"request":payload["request"]["request"]["request"],"artifact":public,"reviewed_at":nucleus::execution::now().timestamp(),"expected_projection":hash});
    let send = receipt_delivery::send_form("ana", &preview);
    assert!(send.fields.is_empty());
    assert_eq!(send.review["Exact outgoing artifact"], public);
    let action = send.payload().unwrap();
    assert_eq!(action["request"]["request"]["kind"], "send");
    assert_eq!(
        action["request"]["request"]["expected_projection"],
        json!(hash)
    );
    serde_json::from_value::<engine::actions::Action>(action).unwrap();
    let mut inspect = receipt_delivery::inspect_form("ana");
    inspect.fields[0].value = nucleus::new_uid("r");
    inspect.commit_fields().unwrap();
    let action = inspect.payload().unwrap();
    assert_eq!(action["request"]["request"]["kind"], "inspect");
    serde_json::from_value::<engine::actions::Action>(action).unwrap();
}

#[cfg_attr(test, test)]
fn public_event_forms_review_only_the_projection_and_keep_source_references_private() {
    let receipt = json!({"record":nucleus::new_uid("r"),"version":3});
    let source = json!({"kind":"outcome","record":nucleus::new_uid("r")});
    let mut form = receipts::event_form("ana", &receipt, source.clone());
    form.fields[0].value = nucleus::new_uid("r");
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["request"]["source"], source);
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let public = json!({"content":{"kind":"statement","role":"receiver","quantity":{"value":"12","scale":0}},"source_commitment":"opaque original Fact commitment"});
    let preview = json!({"event":public,"artifact_kind":"proof_fragment","request":form.data["request"]["request"],"reviewed_at":1_900_000_000,"expected_projection":nucleus::karma::canonical_hash("lince.transfer.test.v1", &"projection").unwrap()});
    let confirmation = receipts::publish_event_form("ana", &preview);
    assert!(confirmation.fields.is_empty());
    assert_eq!(confirmation.review["Public event"], public);
    assert!(
        confirmation.review["Proof packaging"]
            .as_str()
            .unwrap()
            .contains("separate artifacts")
    );
    assert!(
        !confirmation.review["Public event"]
            .to_string()
            .contains(source["record"].as_str().unwrap())
    );
    assert_eq!(confirmation.data["request"]["request"]["source"], source);
    serde_json::from_value::<engine::actions::Action>(confirmation.payload().unwrap()).unwrap();
    let mut settlement = receipts::event_form(
        "ana",
        &receipt,
        json!({"kind":"settlement","fact":"fact","supersedes":null}),
    );
    settlement.fields[0].value = nucleus::new_uid("r");
    settlement.commit_fields().unwrap();
    serde_json::from_value::<engine::actions::Action>(settlement.payload().unwrap()).unwrap();
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: "ana".into(),
            ..Default::default()
        },
    );
    let origin = nucleus::new_uid("r");
    let occurrence = json!({"uid":nucleus::new_uid("toc")});
    let commitment = nucleus::transfer::receipt::Source::occurrence_id(
        &origin,
        occurrence["uid"].as_str().unwrap(),
    )
    .unwrap();
    let mut eligible = receipt.clone();
    let now = nucleus::execution::now().timestamp();
    let scope = nucleus::transfer::receipt::Scope {
        audience:nucleus::trust_analysis::contacts::Audience::Public, upfront_terms:None,
        public_persons:Vec::new(), original_authorities:Vec::new(), party_attribution:nucleus::transfer::receipt::PartyAttribution::LaterOrganEndorsement,
        source:nucleus::transfer::receipt::Source { origin:origin.clone(), occurrence:commitment.clone(), activated_at:now-20, agreement_revision:1, agreement_commitment:commitment.clone(), activation_commitment:commitment },
        giver:origin, receiver:nucleus::new_uid("r"), measurements:Vec::new(), stages:vec![nucleus::transfer::receipt::Stage::Settled], agreed_quantity:None, canonical_unit:None, issued_at:now-10, expires_at:now+86400,
    };
    let approvals = [nucleus::transfer::receipt::ApprovalRole::Origin,nucleus::transfer::receipt::ApprovalRole::Giver,nucleus::transfer::receipt::ApprovalRole::Receiver].into_iter().map(|role| nucleus::transfer::receipt::Approval {role,issued_at:now,key_id:"test".into(),signature:"a".repeat(88)}).collect::<Vec<_>>();
    eligible["document"] = json!(nucleus::transfer::receipt::Document::Review(nucleus::transfer::receipt::Review { scope, approvals, person_approvals:Vec::new(), authorities:Vec::new() }));
    app.world_mut().get_mut::<View>(owner).unwrap().receipt = Some(eligible.clone());
    assert!(receipts::current_receipt(app.world(), owner, &occurrence).is_some());
    assert!(
        receipts::current_receipt(app.world(), owner, &json!({"uid":nucleus::new_uid("toc")}))
            .is_none()
    );
    eligible["document"]["value"]["approvals"].as_array_mut().unwrap().pop();
    app.world_mut().get_mut::<View>(owner).unwrap().receipt = Some(eligible);
    assert!(receipts::current_receipt(app.world(), owner, &occurrence).is_none());
}

#[cfg_attr(test, test)]
fn original_participation_forms_keep_private_requests_separate_from_publication_consent() {
    let transfer = json!({"uid":nucleus::new_uid("r"),"revision":2});
    let party = nucleus::new_uid("r");
    let origin = nucleus::new_uid("r");
    let agreement = nucleus::karma::canonical_hash("lince.transfer.test.v1", &"agreement").unwrap();
    let commitment = nucleus::karma::canonical_hash(
        "lince.transfer.participation.party.v1",
        &(&origin, transfer["uid"].as_str().unwrap(), 2u64, &party),
    )
    .unwrap();
    let request = json!({"transfer":transfer["uid"],"party":party,"terms":{
        "organ":nucleus::new_uid("r"),"origin":origin,"agreement_revision":2,
        "agreement_commitment":agreement,"party_commitment":commitment,
    }});
    let mut form = receipts::participation_form("ana", &request);
    assert!(form.payload().is_err());
    receipts::review_participation_form(&mut form).unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["request"], request.to_string());
    assert_eq!(payload["request"]["kind"], "authorize_participation");
    assert!(
        form.review["Privacy"]
            .as_str()
            .unwrap()
            .contains("private Transfer and Person references")
    );
    assert!(
        form.review["Meaning"]
            .as_str()
            .unwrap()
            .contains("displayed optional manifest")
    );
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    form.fields[0].value = format!(" {} ", request);
    form.commit_fields().unwrap();
    form.fields[0].value = format!("{} ", serde_json::to_string_pretty(&request).unwrap());
    form.commit_fields().unwrap();
    assert!(form.payload().is_err());
    receipts::review_participation_form(&mut form).unwrap();
    assert!(form.payload().is_ok());
    let certificate = nucleus::new_uid("r");
    let list = json!({"entries":[{"record":certificate,"participation":request["terms"]}]});
    let action = detail::action_base(&transfer, "activate-transfer-occurrence", "ana");
    let mut form = receipts::activation_form(&transfer, action, &list);
    assert_eq!(form.data["original_authorities"], json!([]));
    form.data["promise"] = json!("delivery");
    form.fields[0].value = certificate.clone();
    form.commit_fields().unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["original_authorities"], json!([certificate]));
    assert!(payload.get("party_attribution").is_none());
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let receipt = json!({"record":nucleus::new_uid("r"),"version":1});
    let original = json!({"party_attribution":"original_organ_authority"});
    for role in ["giver", "receiver"] {
        let mut form = receipts::approval_form("ana", &receipt, &original, role);
        assert!(form.data["request"]["private_transfer"].is_null());
        form.fields[0].value = transfer["uid"].as_str().unwrap().into();
        form.fields[1].value = certificate.clone();
        form.commit_fields().unwrap();
        let payload = form.payload().unwrap();
        assert_eq!(payload["request"]["private_transfer"], transfer["uid"]);
        assert_eq!(payload["request"]["role"], role);
        serde_json::from_value::<engine::actions::Action>(payload).unwrap();
        assert!(
            form.review["Private reference"]
                .as_str()
                .unwrap()
                .contains("omitted from the exported receipt")
        );
    }
    for (scope, role) in [
        (original, "origin"),
        (
            json!({"party_attribution":"later_organ_endorsement"}),
            "receiver",
        ),
    ] {
        let form = receipts::approval_form("ana", &receipt, &scope, role);
        assert!(form.data["request"].get("private_transfer").is_none());
        assert_eq!(form.fields.len(), 1);
    }
}

#[cfg_attr(test, test)]
fn receipt_results_survive_refresh_and_clear_when_the_acting_person_changes() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: "ana".into(),
            ..Default::default()
        },
    );
    let form = receipts::action_form(
        "ana",
        "Export receipt",
        json!({"kind":"export","record":"review"}),
    );
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.ready = true;
        view.context = json!({"organ":"organ","viewer":{"local":true}});
        view.pending = Some(Pending {
            id: "receipt-result".into(),
            form,
        });
    }
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk {
        id:"receipt-result".into(),created:Some("review".into()),facts:0,warnings:Vec::new(),
        data:Some(json!({"receipt":{"record":"review","version":2,"state":"private_review","document":{"kind":"review","value":{
            "scope":{"source":{"origin":"organ"},"giver":"organ","receiver":"other","expires_at":1_900_000_000},"approvals":[],"authorities":[]
        }}},"receipt_export":"bounded document"})),
    }));
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert_eq!(view.receipt.as_ref().unwrap()["version"], 2);
    assert_eq!(view.receipt_export.as_deref(), Some("bounded document"));
    let cached = view.receipt.clone();
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(Pending {
        id: "receipt-withheld".into(),
        form: receipts::action_form(
            "ana",
            "Inspect receipt",
            json!({"kind":"inspect","record":"review"}),
        ),
    });
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::Error {
            id: "receipt-withheld".into(),
            message: "Receipt signing authority has been revoked".into(),
            code: Some("transfer_receipt_authority_withheld".into()),
        }));
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().receipt.is_none());
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .receipt_export
            .is_none()
    );
    app.world_mut().get_mut::<View>(owner).unwrap().receipt = cached;
    app.world_mut()
        .get_mut::<TransferCastle>(owner)
        .unwrap()
        .form = Some(receipts::action_form(
        "ana",
        "Approve sharing",
        json!({"kind":"approve","record":"review","expected_version":2,"role":"giver","authority_record":"certificate"}),
    ));
    forms::render(app.world_mut(), owner);
    ui::Command::Person("beto".into()).apply(app.world_mut(), owner);
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.receipt.is_none());
    assert!(view.receipt_export.is_none());
    assert!(view.receipt_list.is_null());
    assert!(
        app.world()
            .get::<TransferCastle>(owner)
            .unwrap()
            .form
            .is_none()
    );
    assert_eq!(
        app.world().get::<Node>(view.form).unwrap().display,
        Display::None
    );
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(Pending {
        id: "stale-receipt-reply".into(),
        form: receipts::action_form(
            "ana",
            "Inspect receipt",
            json!({"kind":"inspect","record":"private-review"}),
        ),
    });
    app.world_mut()
        .resource_mut::<Requests>()
        .0
        .insert("receipt-viewer-change".into(), (owner, "transfers".into()));
    app.world_mut().write_message(CellMessage(cell::ServerMessage::Snapshot {
        id:"receipt-viewer-change".into(),rows:vec![json!({"kind":"transfer_context","viewer":{"local":false,"person":"beto"},"organ":"organ"})],
    }));
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "stale-receipt-reply".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(
                json!({"receipt":{"record":"private-review"},"receipt_export":"private-document"}),
            ),
        }));
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().receipt.is_none());
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .receipt_export
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn source_history_pages_outcomes_and_explains_conflicts_without_computing_credit() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: "ana".into(),
            ..Default::default()
        },
    );
    let occurrence =
        nucleus::karma::canonical_hash("lince.transfer.history.ui.v1", &"source").unwrap();
    let scope = json!({"source":{"origin":nucleus::new_uid("r"),"occurrence":occurrence}});
    let form = receipt_history::history_form("ana", &scope);
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["kind"], "history");
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    assert!(!form.review.to_string().contains("Private Record"));
    let outcomes=(0..65).map(|index|json!({"id":format!("outcome-{index}"),"conflicted":index==0,"slots":[],"copies":[{"record":format!("receipt-{index}"),"content":{"kind":"statement","quantity":"12"}}]})).collect::<Vec<_>>();
    let document = json!({"kind":"source_history","value":{"origin":scope["source"]["origin"],"occurrence":occurrence,"partial":true,"coverage":"Permitted local evidence","history":{"source_conflict":true,"outcomes":outcomes,"slots":[]},"failures":[{"state":"unknown_identity","message":"Recognize the original Organ first"}]}});
    let panel = app.world_mut().spawn(Node::default()).id();
    assert!(receipt_history::render(
        app.world_mut(),
        owner,
        panel,
        "ana",
        &document
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|text| text == "More outcomes"));
    assert!(
        labels
            .iter()
            .any(|text| text.contains("Conflicting source terms"))
    );
    assert!(
        labels
            .iter()
            .any(|text| text.contains("Recognize the original Organ"))
    );
    assert_eq!(
        labels
            .iter()
            .filter(|text| text.contains("signed copies"))
            .count(),
        32
    );
    app.world_mut().entity_mut(panel).despawn_children();
    app.world_mut().get_mut::<View>(owner).unwrap().receipt_page = 2;
    assert!(receipt_history::render(
        app.world_mut(),
        owner,
        panel,
        "ana",
        &document
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|text| text == "Previous outcomes"));
    assert!(!labels.iter().any(|text| text == "More outcomes"));
    assert_eq!(
        labels
            .iter()
            .filter(|text| text.contains("signed copies"))
            .count(),
        1
    );
    app.world_mut().get_mut::<View>(owner).unwrap().receipt = Some(json!({"document":document}));
    ui::Command::Person("beto".into()).apply(app.world_mut(), owner);
    assert!(app.world().get::<View>(owner).unwrap().receipt.is_none());
    assert_eq!(app.world().get::<View>(owner).unwrap().receipt_page, 0);
}

#[cfg_attr(test, test)]
fn original_version_navigation_is_read_only_and_proof_pages_are_not_importable_receipts() {
    let record = json!(nucleus::new_uid("r"));
    for form in [
        receipt_browsing::versions_form("ana", &record, json!("opaque-version-position")),
        receipt_browsing::proof_form("ana", &record, &json!(2), json!("opaque-proof-position")),
        receipts::action_form(
            "ana",
            "Read original version",
            json!({"kind":"inspect_version","record":record,"version":1}),
        ),
        receipts::action_form(
            "ana",
            "Export original version",
            json!({"kind":"export_version","record":record,"version":1}),
        ),
    ] {
        serde_json::from_value::<engine::actions::Action>(form.payload().unwrap()).unwrap();
    }
    let source = json!({"origin":nucleus::new_uid("r"),"occurrence":nucleus::karma::canonical_hash("lince.receipt.browsing-ui.v1",&"source").unwrap(),"next_before":"opaque-source-position"});
    let continuation = receipt_history::continuation_form("ana", &source)
        .payload()
        .unwrap();
    assert_eq!(continuation["request"]["before"], "opaque-source-position");
    serde_json::from_value::<engine::actions::Action>(continuation).unwrap();
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: "ana".into(),
            ..Default::default()
        },
    );
    let receipt = json!({"record":record,"version":1,"historical":true,"state":"private_review","document":{"kind":"review","value":{"scope":{"source":source,"giver":"organ","receiver":"organ"},"approvals":[]}}});
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.context = json!({"viewer":{"local":true},"organ":"organ"});
        view.receipt = Some(receipt);
    }
    let panel = app.world_mut().spawn(Node::default()).id();
    receipts::render(app.world_mut(), owner, panel);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|text| text == "Browse original signed proof")
    );
    assert!(
        labels
            .iter()
            .any(|text| text == "Prepare exact-version export")
    );
    assert!(
        !labels
            .iter()
            .any(|text| text.starts_with("Approve as ") || text == "Review outcome event…")
    );
    app.world_mut().entity_mut(panel).despawn_children();
    let proof = json!({"kind":"receipt_proof","value":{"record":record,"version":2,"document_state":"verified_receipt","coverage":"Read-only original proof. This page is not a portable receipt.","header":{},"events":[{"event":{"source_commitment":"evidence"}}],"next_before":"opaque-proof-position"}});
    assert!(receipt_browsing::render(
        app.world_mut(),
        owner,
        panel,
        "ana",
        &proof
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|text| text == "Next proof page"));
    assert!(
        !labels
            .iter()
            .any(|text| text.contains("Import reviewed document"))
    );
    for (code, label) in [
        ("transfer_receipt_unknown_identity", "Unknown identity"),
        ("transfer_receipt_incomplete", "Incomplete proof"),
        ("transfer_receipt_authority_withheld", "Authority withheld"),
        ("transfer_receipt_invalid", "Invalid proof"),
    ] {
        let explanation = receipts::verification_message(Some(code), "Original reason");
        assert!(explanation.contains(label));
        assert!(explanation.contains("Original reason"));
    }
}

#[cfg_attr(test, test)]
fn unresolved_signed_claims_are_separate_from_accepted_outcomes_and_keep_original_links() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: "ana".into(),
            ..Default::default()
        },
    );
    let claims = (0..33).map(|index| json!({"event":format!("event-{index}"),"record":nucleus::new_uid("r"),"version":2,"state":if index == 0 {"invalid"} else {"incomplete"},"reason":"Original dependency unavailable","content":{"kind":"statement","quantity":"12"}})).collect::<Vec<_>>();
    let document = json!({"kind":"source_history","value":{"origin":nucleus::new_uid("r"),"occurrence":nucleus::karma::canonical_hash("lince.receipt.pending-ui.v1",&"source").unwrap(),"history":{"outcomes":[],"slots":[]},"unresolved":claims,"partial":true,"next_before":"opaque-position"}});
    let panel = app.world_mut().spawn(Node::default()).id();
    assert!(receipt_history::render(
        app.world_mut(),
        owner,
        panel,
        "ana",
        &document
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|text| text.contains("excluded from accepted outcomes"))
    );
    assert!(labels.iter().any(|text| text.contains("Invalid proof")));
    assert!(labels.iter().any(|text| text.contains("Incomplete proof")));
    assert!(labels.iter().any(|text| text == "Older evidence Records"));
    assert_eq!(
        labels
            .iter()
            .filter(|text| text.as_str() == "Open unresolved original version")
            .count(),
        32
    );
    assert!(!labels.iter().any(|text| text.contains("signed copies")));
    app.world_mut().entity_mut(panel).despawn_children();
    app.world_mut().get_mut::<View>(owner).unwrap().receipt_page = 1;
    assert!(receipt_history::render(
        app.world_mut(),
        owner,
        panel,
        "ana",
        &document
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        labels
            .iter()
            .filter(|text| text.as_str() == "Open unresolved original version")
            .count(),
        1
    );
}

crate::native::laboratory_cases! {
    transfer_party_opens_the_actual_person_experience_without_discovery,
    public_history_form_keeps_nonce_stable_and_requires_explicit_audience_and_renewal_review,
    lifecycle_scope_and_review_keep_cancellation_and_compensation_separate_from_delivery,
    person_key_comparison_is_reviewed_and_late_results_do_not_cross_person_views,
    personal_attribution_is_optional_and_publication_waits_for_personal_consent,
    unresolved_signed_claims_are_separate_from_accepted_outcomes_and_keep_original_links,
    original_version_navigation_is_read_only_and_proof_pages_are_not_importable_receipts,
    source_history_pages_outcomes_and_explains_conflicts_without_computing_credit,
    receipt_delivery_forms_review_all_recipients_and_keep_send_separate_from_import,
    receipt_source_forms_keep_frozen_terms_and_exact_reports_separate_from_publication,
    public_event_forms_review_only_the_projection_and_keep_source_references_private,
    original_participation_forms_keep_private_requests_separate_from_publication_consent,
    receipt_results_survive_refresh_and_clear_when_the_acting_person_changes,
    receipt_review_forms_preserve_the_exact_scope_and_require_explicit_approval,
    outcome_forms_keep_exact_amounts_separate_from_completion_and_settlement,
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

#[cfg_attr(test, test)]
fn public_history_form_keeps_nonce_stable_and_requires_explicit_audience_and_renewal_review() {
    let person = nucleus::new_uid("r");
    let mut form = model::composer(&person, "Donation");
    assert!(form.data["public_history"].is_null());
    let terms = nucleus::transfer::public_history::Terms::default();
    form.data["public_history"] = json!(terms);
    form.step = Some(3);
    forms::composer_fields(&mut form);
    assert_eq!(form.fields.iter().find(|field| field.path == "/public_history/retention_seconds").unwrap().value, "365d");
    form.fields.iter_mut().find(|field| field.path == "/public_history/retention_seconds").unwrap().value = "24h".into();
    form.fields.iter_mut().find(|field| field.path == "/public_history/audience/kind").unwrap().value = "selected".into();
    form.commit_fields().unwrap();
    forms::composer_fields(&mut form);
    form.fields.iter_mut().find(|field| field.path == "/public_history/audience/viewers").unwrap().value = format!("person:{person}");
    form.commit_fields().unwrap();
    assert_eq!(form.data["public_history"]["nonce"], terms.nonce);
    assert_eq!(form.data["public_history"]["retention_seconds"], 86400);
    assert_eq!(form.data["public_history"]["audience"]["viewers"], json!([{"kind":"person","uid":person}]));
    let selected: nucleus::transfer::public_history::Terms = serde_json::from_value(form.data["public_history"].clone()).unwrap();
    selected.validate().unwrap();
    let scope = receipts::prepare_form(&json!({"uid":nucleus::new_uid("r"),"revision":1,"revision_evidence":{"current":{"terms":{"transfer":{"public_history":selected}}}}}), &json!({"uid":nucleus::new_uid("toc")}), &person);
    assert_eq!(scope.data["request"]["scope"]["audience"]["kind"], "selected");
    assert_eq!(scope.data["request"]["scope"]["use_upfront_terms"], true);
    assert!(scope.fields.iter().any(|field| field.path == "/request/scope/use_upfront_terms"));
    let mut wrong = form.data["public_history"].clone();
    wrong.as_object_mut().unwrap().remove("nonce");
    assert!(serde_json::from_value::<nucleus::transfer::public_history::Terms>(wrong).is_err());
}

#[cfg_attr(test, test)]
fn lifecycle_scope_and_review_keep_cancellation_and_compensation_separate_from_delivery() {
    use nucleus::transfer::receipt::{Content, RecordedFloat};
    let receipt = json!({"record":nucleus::new_uid("r"),"version":3});
    for kind in ["settlement_status", "cancellation", "compensation"] {
        let mut form = receipts::event_form(
            "ana",
            &receipt,
            json!({"kind":kind,"fact":"","supersedes":null}),
        );
        form.fields
            .iter_mut()
            .find(|field| field.path == "/request/request/authority_record")
            .unwrap()
            .value = nucleus::new_uid("r");
        form.fields
            .iter_mut()
            .find(|field| field.path == "/request/request/source/fact")
            .unwrap()
            .value = nucleus::new_uid("f");
        form.commit_fields().unwrap();
        let payload = form.payload().unwrap();
        assert_eq!(payload["request"]["request"]["source"]["kind"], kind);
        serde_json::from_value::<engine::actions::Action>(payload).unwrap();
        assert!(
            form.review["Lifecycle disclosure"]
                .as_str()
                .unwrap()
                .contains("Cancellation is not delivery")
        );
    }
    for (remaining, expected) in [(6.0, "partially settled"), (0.0, "fully settled")] {
        let content = Content::SettlementStatus {
            quantity: RecordedFloat::new(4.0).unwrap(),
            remaining_after: RecordedFloat::new(remaining).unwrap(),
        };
        let label = receipt_history::description(&serde_json::to_value(content).unwrap());
        assert!(label.contains(expected));
        assert!(label.contains("not Need fulfillment"));
    }
    let label = receipt_history::description(
        &json!({"kind":"cancelled","quantity":nucleus::DecimalValue::parse_inferred("6").unwrap(),"revision":2}),
    );
    assert!(label.contains("not delivered quantity"));
    let commitment = nucleus::karma::canonical_hash("lince.lifecycle.ui.v1", &"source").unwrap();
    let label = receipt_history::description(
        &json!({"kind":"compensated","role":"giver","settlement":commitment,"application":commitment}),
    );
    assert!(label.contains("canonical settlement unchanged"));
    let form = receipts::prepare_form(
        &json!({"uid":"transfer","revision":1}),
        &json!({"uid":"occurrence"}),
        "ana",
    );
    let stages = form
        .fields
        .iter()
        .find(|field| field.path == "/request/scope/stages")
        .unwrap();
    let model::FieldKind::Selections(choices) = &stages.kind else {
        panic!("Stage choices expected");
    };
    for stage in ["settlement_status", "cancelled", "compensated"] {
        assert!(choices.iter().any(|(value, _)| value == stage));
        assert!(
            !form.data["request"]["scope"]["stages"]
                .as_array()
                .unwrap()
                .contains(&json!(stage))
        );
    }
}

#[cfg_attr(test, test)]
fn person_key_comparison_is_reviewed_and_late_results_do_not_cross_person_views() {
    let person = nucleus::new_uid("r");
    let signer = engine::trust::Signer::generate(&person, "independent-v1");
    let key = nucleus::identity::PersonKey {
        subject: person.clone(),
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
    };
    let mut preview = receipt_identity::preview_form(&person);
    preview
        .fields
        .iter_mut()
        .find(|field| field.path == "/request/key/key_id")
        .unwrap()
        .value = key.key_id.clone();
    preview
        .fields
        .iter_mut()
        .find(|field| field.path == "/request/key/public_key")
        .unwrap()
        .value = key.public_key.clone();
    preview.commit_fields().unwrap();
    let payload = preview.payload().unwrap();
    assert_eq!(payload["action"], "identity");
    assert!(payload.get("request_id").is_none());
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    serde_json::from_value::<engine::actions::Action>(
        receipt_identity::keys_form(&person).payload().unwrap(),
    )
    .unwrap();
    let value = json!({"kind":"person_key_preview","key":key,"profile_revision":4,"fingerprint":key.fingerprint().unwrap(),"review_hash":key.review_hash(4).unwrap(),"meaning":"Recognition is separate from reliability"});
    let accept = receipt_identity::accept_form(&value);
    assert!(accept.fields.is_empty());
    let payload = accept.payload().unwrap();
    assert_eq!(payload["request"]["expected_review"], value["review_hash"]);
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.clone(),
            form: Some(accept.clone()),
            ..Default::default()
        },
    );
    let panel = app.world_mut().spawn(Node::default()).id();
    assert!(receipt_identity::render(
        app.world_mut(),
        owner,
        panel,
        &json!({"kind":"identity","value":value})
    ));
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(labels.iter().any(|text| text == "Accept compared key…"));
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.pending = Some(Pending {
            id: "identity-preview".into(),
            form: preview,
        });
        view.ready = true;
        view.context = json!({"viewer":{"local":true},"organ":"organ"});
        view.receipt_export = Some("earlier receipt export".into());
    }
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "identity-preview".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"identity":value})),
        }));
    app.update();
    assert_eq!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()["document"]["kind"],
        "identity"
    );
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .receipt_export
            .is_none()
    );
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(Pending {
        id: "old-identity".into(),
        form: accept,
    });
    ui::Command::Person(nucleus::new_uid("r")).apply(app.world_mut(), owner);
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    assert!(
        app.world()
            .get::<TransferCastle>(owner)
            .unwrap()
            .form
            .is_none()
    );
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "old-identity".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"identity":value})),
        }));
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().receipt.is_none());
}

#[cfg_attr(test, test)]
fn personal_attribution_is_optional_and_publication_waits_for_personal_consent() {
    use nucleus::transfer::receipt::{
        Approval, ApprovalRole, PartyAttribution, PublicPerson, Review, Scope, Source, Stage,
    };
    let person = nucleus::new_uid("r");
    let record = nucleus::new_uid("r");
    let occurrence = nucleus::new_uid("toc");
    let now = nucleus::execution::now().timestamp();
    let digest = nucleus::karma::canonical_hash("lince.receipt.person-ui.v1", &"proof").unwrap();
    let origin = nucleus::new_uid("r");
    let scope = Scope {
        audience: nucleus::trust_analysis::contacts::Audience::Public,
        upfront_terms: None,
        public_persons: vec![PublicPerson {
            role: ApprovalRole::Receiver,
            person: person.clone(),
        }],
        source: Source {
            origin: origin.clone(),
            occurrence: Source::occurrence_id(&origin, &occurrence).unwrap(),
            activated_at: now - 20,
            agreement_revision: 1,
            agreement_commitment: digest.clone(),
            activation_commitment: digest,
        },
        party_attribution: PartyAttribution::LaterOrganEndorsement,
        original_authorities: Vec::new(),
        giver: origin.clone(),
        receiver: nucleus::new_uid("r"),
        measurements: Vec::new(),
        stages: vec![Stage::Settled],
        agreed_quantity: None,
        canonical_unit: None,
        issued_at: now - 10,
        expires_at: now + 86400,
    };
    let receipt = json!({"record":record,"version":1});
    let form =
        receipts::person_approval_form(&person, &receipt, &json!(scope), "receiver").unwrap();
    let payload = form.payload().unwrap();
    assert_eq!(payload["request"]["approval"]["person"], person);
    assert_eq!(
        payload["request"]["approval"]["scope"],
        json!(scope.id().unwrap())
    );
    assert!(
        !payload["request"]["approval"]
            .to_string()
            .contains(&occurrence)
    );
    assert!(
        form.review["Consent"]
            .as_str()
            .unwrap()
            .contains("does not confirm delivery")
    );
    serde_json::from_value::<engine::actions::Action>(payload).unwrap();
    let sharing = receipts::prepare_form(
        &json!({"uid":record,"revision":1}),
        &json!({"uid":occurrence}),
        &person,
    );
    assert_eq!(
        sharing.data["request"]["scope"]["public_person_roles"],
        json!([])
    );
    let mut app = app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.clone(),
            ..Default::default()
        },
    );
    let review = Review {
        scope,
        person_approvals: Vec::new(),
        approvals: [
            ApprovalRole::Origin,
            ApprovalRole::Giver,
            ApprovalRole::Receiver,
        ]
        .into_iter()
        .map(|role| Approval {
            role,
            issued_at: now,
            key_id: "organ-key".into(),
            signature: "a".repeat(88),
        })
        .collect(),
        authorities: Vec::new(),
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.context = json!({"viewer":{"local":true},"organ":origin});
        view.receipt = Some(
            json!({"record":record,"version":1,"state":"private_review","document":{"kind":"review","value":review}}),
        );
    }
    let panel = app.world_mut().spawn(Node::default()).id();
    receipts::render(app.world_mut(), owner, panel);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|text| text == "Consent to identifying me…")
    );
    assert!(
        labels
            .iter()
            .any(|text| text.contains("0 of 1 personal identity consents"))
    );
    assert!(!labels.iter().any(|text| text == "Review outcome event…"));
}
