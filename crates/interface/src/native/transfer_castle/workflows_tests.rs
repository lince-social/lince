use super::super::{self as castle, TransferCastle, View, detail};
use super::*;
use bevy::{math::DVec2, prelude::*};
use engine::{Engine, actions::Action as Backend, trust::Signer};

fn setup(person: &str) -> (App, Entity) {
    let mut app = castle::tests::app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let owner = castle::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.into(),
            ..Default::default()
        },
    );
    app.world_mut().get_mut::<View>(owner).unwrap().context =
        json!({"viewer":{"local":true},"capabilities":{"create":true}});
    (app, owner)
}

fn activate(world: &mut World, caption: &str) {
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == caption)
        .unwrap_or_else(|| panic!("Missing {caption}"))
        .0;
    let button = world.get::<ChildOf>(label).unwrap().parent();
    if let Some(action) = world.get::<crate::native::actions::ActionButton>(button) {
        let (target, actions) = (action.target, action.actions.clone());
        actions.run(world, target);
    } else {
        world.trigger(bevy::ui_widgets::Activate { entity: button });
        world.flush();
    }
}

fn preset(person: &str, kind: &str) -> Form {
    let (mut app, owner) = setup(person);
    activate(app.world_mut(), "Use preset…");
    activate(app.world_mut(), kind);
    let mut form = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .clone()
        .unwrap();
    assert_eq!(form.preset, kind);
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .preview_request
            .is_none()
    );
    form.fields.clear();
    form
}

#[cfg_attr(test, test)]
fn actual_activity_presets_keep_parties_units_and_reviewed_route_identities_separate() {
    let creator = nucleus::new_uid("r");
    let other = nucleus::new_uid("r");
    for kind in ACTIVITIES {
        let mut form = preset(&creator, kind);
        form.data["invitees"] = json!([other]);
        model::bind_preset(&mut form);
        let input = form.payload().unwrap();
        let promises = input["promises"].as_array().unwrap();
        assert_eq!(
            promises.len(),
            if kind == "Paid contribution" { 2 } else { 1 }
        );
        let requesting = matches!(kind, "Need to receive" | "Need to contribute");
        assert_eq!(
            promises[0]["item"]["exchange"]["giver"].as_str(),
            Some(if requesting {
                other.as_str()
            } else {
                creator.as_str()
            })
        );
        assert_eq!(
            promises[0]["item"]["exchange"]["receiver"].as_str(),
            Some(if requesting {
                creator.as_str()
            } else {
                other.as_str()
            })
        );
        assert_eq!(input["visibility"], "hidden");
        assert!(input["public_history"].is_null());
        let route = promises[0]["item"]["exchange"]["uid"].clone();
        model::bind_preset(&mut form);
        assert_eq!(form.data["promises"][0]["item"]["exchange"]["uid"], route);
        let _: Backend = serde_json::from_value(input).unwrap();
    }
}

fn proposals(person: &str, request: &Value) -> (Form, Form) {
    let (mut app, owner) = setup(person);
    let detail = app.world().get::<View>(owner).unwrap().detail;
    detail::render(app.world_mut(), owner, detail, request);
    activate(app.world_mut(), "Propose another solution");
    let mut alternative = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .clone()
        .unwrap();
    alternative.fields.clear();
    activate(app.world_mut(), "Cancel");
    detail::render(app.world_mut(), owner, detail, request);
    activate(app.world_mut(), "Create a smaller linked Need");
    let mut child = app
        .world()
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .clone()
        .unwrap();
    child.fields.clear();
    assert_ne!(alternative.request_id, child.request_id);
    (alternative, child)
}

#[cfg_attr(test, test)]
fn actual_solution_shortcuts_create_independent_drafts_and_preserve_the_parent_request() {
    let person = nucleus::new_uid("r");
    let request = nucleus::new_uid("r");
    let row = json!({"uid":request,"head":"Guitar event","revision":1,"capabilities":{"edit_terms":true},"promises":[{"item":{"exchange":{"giver":nucleus::new_uid("r"),"receiver":person}}}]});
    let (alternative, child) = proposals(&person, &row);
    assert_eq!(alternative.preset, "Need to receive");
    assert_eq!(alternative.data["source"], request);
    assert!(alternative.data["parent"].is_null());
    assert_eq!(child.data["source"], request);
    assert_eq!(child.data["parent"], request);
    assert_eq!(child.data["satiation"], "none");
    for form in [alternative, child] {
        assert!(matches!(
            serde_json::from_value::<Backend>(form.payload().unwrap()).unwrap(),
            Backend::CreateTransferDraft { .. }
        ));
    }
    assert_eq!(row["revision"], 1);
}

async fn record(engine: &Engine, head: &str, kind: nucleus::RecordKind, quantity: f64) -> String {
    engine
        .act(
            Backend::CreateRecord {
                slug: None,
                kind,
                head: head.into(),
                body: String::new(),
                quantity,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn actor(engine: &Engine, label: &str) -> (String, Signer) {
    let uid = record(engine, label, nucleus::RecordKind::Person, 1.0).await;
    let signer = Signer::generate(&uid, &format!("activity:{label}"));
    (uid, signer)
}

async fn snapshot(engine: &Engine, transfer: &str, person: &str) -> Value {
    protein::execute_for_with_signer(
        &engine.store,
        &protein::Protein {
            source: protein::Source::Transfer,
            filter: vec![protein::Predicate::UidEq(transfer.into())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        },
        None,
        Some(person),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|row| row["uid"] == transfer)
    .unwrap()
}

async fn create(engine: &Engine, form: &Form, signer: &Signer) -> String {
    engine.set_signer(signer.clone()).await.unwrap();
    let action: Backend = serde_json::from_value(form.payload().unwrap()).unwrap();
    let uid = engine
        .act(action.clone(), None)
        .await
        .unwrap()
        .created
        .unwrap();
    assert_eq!(
        engine.act(action, None).await.unwrap().created,
        Some(uid.clone())
    );
    uid
}

async fn accept(engine: &Engine, transfer: &str, person: &str, signer: &Signer) {
    engine.set_signer(signer.clone()).await.unwrap();
    let invitation = store::transfers::invitations_for_transfer(&engine.store.pool, transfer)
        .await
        .unwrap()
        .into_iter()
        .find(|invitation| invitation.addressed_person_uid == person)
        .unwrap();
    let revision = store::transfers::get(&engine.store.pool, transfer)
        .await
        .unwrap()
        .unwrap()
        .revision as u64;
    engine
        .act(
            Backend::AcceptTransferInvitation {
                transfer: Some(transfer.into()),
                invitation: invitation.uid,
                person: Some(person.into()),
                expected_revision: revision,
                request_id: nucleus::new_uid("op"),
            },
            None,
        )
        .await
        .unwrap();
}

async fn agree(engine: &Engine, transfer: &str, person: &str, signer: &Signer) {
    engine.set_signer(signer.clone()).await.unwrap();
    let revision = store::transfers::get(&engine.store.pool, transfer)
        .await
        .unwrap()
        .unwrap()
        .revision as u64;
    for level in [1, 2] {
        engine
            .act(
                Backend::SetTransferAgreementLevel {
                    transfer: transfer.into(),
                    expected_revision: revision,
                    request_id: nucleus::new_uid("op"),
                    person: Some(person.into()),
                    level,
                },
                None,
            )
            .await
            .unwrap();
    }
}

#[cfg_attr(test, tokio::test)]
async fn activity_drafts_roundtrip_revisions_and_agreements_without_inventing_delivery() {
    let engine = Engine::open_memory().await.unwrap();
    let (owner, owner_key) = actor(&engine, "Guitar player").await;
    let (other, other_key) = actor(&engine, "Audience").await;
    let need = record(
        &engine,
        "Hear or be heard",
        nucleus::RecordKind::Plain,
        -1.0,
    )
    .await;
    let unit = store::concepts::create(&engine.store.pool, "performance", &[])
        .await
        .unwrap();
    let listening = store::concepts::create(&engine.store.pool, "listening", &[])
        .await
        .unwrap();
    let returned = store::concepts::create(&engine.store.pool, "return-contribution", &[])
        .await
        .unwrap();
    for kind in ACTIVITIES {
        let mut form = preset(&owner, kind);
        form.data["source"] = json!(need);
        form.data["invitees"] = json!([other]);
        let meaning = if kind == "Need to contribute" {
            &listening
        } else {
            &unit
        };
        form.data["promises"][0]["unit"] = json!(meaning);
        if kind == "Paid contribution" {
            form.data["promises"][1]["unit"] = json!(returned);
            form.data["promises"][1]["delta"] = json!(-4);
            form.data["promises"][1]["amount"] = json!(4);
        }
        form.data["promises"][0]["item"]["title"] = json!(if kind == "Need to contribute" {
            "Listening to a guitar performance"
        } else {
            "Guitar performance"
        });
        model::bind_preset(&mut form);
        let Backend::CreateTransferDraft { promises, .. } =
            serde_json::from_value(form.payload().unwrap()).unwrap()
        else {
            panic!("Activity draft");
        };
        assert_eq!(promises[0].unit.as_ref(), Some(meaning));
        let uid = create(&engine, &form, &owner_key).await;
        accept(&engine, &uid, &other, &other_key).await;
        engine.set_signer(owner_key.clone()).await.unwrap();
        let saved = snapshot(&engine, &uid, &owner).await;
        let mut revision = model::edit(&saved, false, &owner);
        revision.fields.clear();
        revision.data["head"] = json!(format!("{kind} reviewed"));
        engine
            .act(
                serde_json::from_value(revision.payload().unwrap()).unwrap(),
                None,
            )
            .await
            .unwrap();
        agree(&engine, &uid, &owner, &owner_key).await;
        agree(&engine, &uid, &other, &other_key).await;
        let levels = store::transfers::party_levels(&engine.store.pool, &uid)
            .await
            .unwrap();
        assert_eq!(levels.len(), 2);
        assert!(levels.iter().all(|party| party.2 == 2));
        let units: Vec<Option<String>> = store::sqlx::query_scalar(
            "SELECT unit_uid FROM promise WHERE transfer_uid=? AND state!='withdrawn'",
        )
        .bind(&uid)
        .fetch_all(&engine.store.pool)
        .await
        .unwrap();
        assert!(units.iter().any(|unit| unit.as_ref() == Some(meaning)));
        if kind == "Paid contribution" {
            assert!(units.iter().any(|unit| unit.as_ref() == Some(&returned)));
        }
        let saved = snapshot(&engine, &uid, &owner).await;
        assert!(
            saved["occurrences"]
                .as_array()
                .is_none_or(|values| values.is_empty())
        );
        assert_eq!(
            store::records::get(&engine.store.pool, &need)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .to_f64(),
            -1.0
        );
        assert!(engine.location_sources().await.is_empty());
    }
}

#[cfg_attr(test, tokio::test)]
async fn alternative_and_child_drafts_roundtrip_links_and_do_not_agree_other_solutions() {
    let engine = Engine::open_memory().await.unwrap();
    let (owner, owner_key) = actor(&engine, "Requester").await;
    let (other, other_key) = actor(&engine, "Contributor").await;
    let need = record(
        &engine,
        "Five contributions needed",
        nucleus::RecordKind::Plain,
        -5.0,
    )
    .await;
    let mut form = crate::transfers::workflows::activity(&owner, "Need to receive").unwrap();
    form.data["source"] = json!(need);
    form.data["invitees"] = json!([other]);
    model::bind_preset(&mut form);
    let root = create(&engine, &form, &owner_key).await;
    let row = snapshot(&engine, &root, &owner).await;
    let (mut first, _) = proposals(&owner, &row);
    first.data["invitees"] = json!([other]);
    model::bind_preset(&mut first);
    let selected = create(&engine, &first, &owner_key).await;
    let (mut second, _) = proposals(&owner, &row);
    second.data["invitees"] = json!([other]);
    model::bind_preset(&mut second);
    second.data["head"] = json!("Another proposal");
    let unchosen = create(&engine, &second, &owner_key).await;
    let row = snapshot(&engine, &selected, &owner).await;
    let mut children = Vec::new();
    for amount in [2, 3] {
        let (_, mut child) = proposals(&owner, &row);
        child.data["invitees"] = json!([other]);
        child.data["promises"][0]["delta"] = json!(-amount);
        child.data["promises"][0]["amount"] = json!(amount);
        model::bind_preset(&mut child);
        children.push(create(&engine, &child, &owner_key).await);
    }
    let saved = snapshot(&engine, &selected, &owner).await;
    let ids = saved["children"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|child| child["uid"].as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids, children.iter().map(String::as_str).collect());
    for (index, child) in children.iter().enumerate() {
        let row = snapshot(&engine, child, &owner).await;
        assert_eq!(row["parent"], selected);
        assert_eq!(row["source"], selected);
        let promises = store::transfers::promises_of(&engine.store.pool, child)
            .await
            .unwrap();
        assert_eq!(promises.len(), 1);
        assert_eq!(promises[0].delta, -(index as f64 + 2.0));
    }
    accept(&engine, &selected, &other, &other_key).await;
    agree(&engine, &selected, &owner, &owner_key).await;
    agree(&engine, &selected, &other, &other_key).await;
    assert!(
        store::transfers::party_levels(&engine.store.pool, &selected)
            .await
            .unwrap()
            .iter()
            .all(|party| party.2 == 2)
    );
    assert!(
        store::transfers::party_levels(&engine.store.pool, &unchosen)
            .await
            .unwrap()
            .iter()
            .all(|party| party.2 == 0)
    );
    assert_eq!(
        store::records::get(&engine.store.pool, &need)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_f64(),
        -5.0
    );
    assert!(engine.location_sources().await.is_empty());
}

#[cfg_attr(test, tokio::test)]
async fn transport_presets_use_saved_addresses_and_reject_invalid_terms_without_side_effects() {
    let engine = Engine::open_memory().await.unwrap();
    let (owner, owner_key) = actor(&engine, "Driver").await;
    let (passenger, _) = actor(&engine, "Passenger").await;
    let mut endpoints = Vec::new();
    for (index, name) in ["Street4", "Station"].into_iter().enumerate() {
        let uid = record(&engine, name, nucleus::RecordKind::Plain, 0.0).await;
        engine
            .act(
                Backend::SetPlace {
                    target: uid.clone(),
                    lat: -23.5 + index as f64 / 10.0,
                    lon: -46.6,
                    address: Some(name.into()),
                },
                None,
            )
            .await
            .unwrap();
        endpoints.push(uid);
    }
    let counts = || async {
        store::sqlx::query_as::<_,(i64,i64,i64)>("SELECT (SELECT COUNT(*) FROM transfer),(SELECT COUNT(*) FROM promise),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&engine.store.pool).await.unwrap()
    };
    for activity in ["Ride", "Delivery"] {
        for direction in ["offer", "need"] {
            let mut form = preset(&owner, activity);
            form.data["invitees"] = json!([passenger]);
            form.data["promises"][0]["item"]["transport"] = json!({"activity":if activity == "Ride" {"ride"} else {"delivery"},"direction":direction,"origin":endpoints[0],"destination":endpoints[1],"count":2,"capacity":2});
            let now = nucleus::execution::now();
            form.data["promises"][0]["window_start"] =
                json!((now - chrono::Duration::minutes(5)).to_rfc3339());
            form.data["promises"][0]["window_end"] =
                json!((now + chrono::Duration::minutes(30)).to_rfc3339());
            model::bind_preset(&mut form);
            engine.set_signer(owner_key.clone()).await.unwrap();
            for (path, invalid) in [
                ("/promises/0/window_start", Value::Null),
                ("/promises/0/window_end", json!("2026-10-17T11:00:00Z")),
                ("/promises/0/item/transport/count", json!(0)),
                ("/promises/0/item/transport/count", json!(3)),
            ] {
                let before = counts().await;
                let mut changed = form.clone();
                *changed.data.pointer_mut(path).unwrap() = invalid;
                if let Ok(payload) = changed.payload() {
                    assert!(
                        engine
                            .act(serde_json::from_value(payload).unwrap(), None)
                            .await
                            .is_err()
                    );
                }
                assert_eq!(counts().await, before);
                assert!(engine.location_sources().await.is_empty());
            }
            let uid = create(&engine, &form, &owner_key).await;
            let saved = snapshot(&engine, &uid, &owner).await;
            assert_eq!(saved["promises"][0]["transport"]["origin"], endpoints[0]);
            assert_eq!(
                saved["promises"][0]["transport"]["destination"],
                endpoints[1]
            );
            assert_eq!(saved["promises"][0]["transport"]["count"], 2);
            assert!(saved["occurrences"].as_array().is_none_or(Vec::is_empty));
            assert!(engine.location_sources().await.is_empty());
            let mut public_form = form.clone();
            public_form.request_id = nucleus::new_uid("public-transport");
            for promise in public_form.data["promises"].as_array_mut().unwrap() {
                promise["uid"] = json!(nucleus::new_uid("p"));
                if promise["item"]["exchange"].is_object() {
                    promise["item"]["exchange"]["uid"] = json!(nucleus::new_uid("exchange"));
                }
            }
            public_form.data["invitees"] = json!([]);
            model::bind_preset(&mut public_form);
            let public_uid = create(&engine, &public_form, &owner_key).await;
            let prepared = engine
                .social_command(
                    nucleus::social::Command::PrepareFromRecord { source: public_uid },
                    None,
                    nucleus::execution::now(),
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let public = prepared["draft"].to_string();
            assert!(!public.contains(&endpoints[0]));
            assert!(!public.contains(&endpoints[1]));
            assert!(!public.contains("Street4"));
            let (mut app, castle) = setup(&owner);
            detail::render(app.world_mut(), castle, castle, &saved);
            assert_eq!(
                app.world_mut()
                    .query::<&crate::native::map_castle::MapSand>()
                    .iter(app.world())
                    .count(),
                0
            );
            assert_eq!(model::transport::sharing_record(&saved, &owner), owner);
            assert_eq!(
                model::transport::sharing_record(&saved, &passenger),
                passenger
            );
            let driver_panel = crate::native::sand_panel::column(app.world_mut(), castle);
            let passenger_panel = crate::native::sand_panel::column(app.world_mut(), castle);
            crate::native::location::sharing_review(
                app.world_mut(),
                driver_panel,
                &owner,
                Some(&uid),
                Some(&owner),
                "Driver sharing review",
            );
            crate::native::location::sharing_review(
                app.world_mut(),
                passenger_panel,
                &passenger,
                Some(&uid),
                Some(&passenger),
                "Passenger sharing review",
            );
            activate(app.world_mut(), "Driver sharing review");
            activate(app.world_mut(), "Passenger sharing review");
            assert_eq!(
                app.world_mut()
                    .query::<&crate::location::Panel>()
                    .iter(app.world())
                    .count(),
                2
            );
            activate(app.world_mut(), "Driver sharing review");
            assert_eq!(
                app.world_mut()
                    .query::<&crate::location::Panel>()
                    .iter(app.world())
                    .count(),
                1
            );
            assert!(engine.location_sources().await.is_empty());
        }
    }
}

#[cfg_attr(test, tokio::test)]
async fn hidden_transport_address_refuses_a_valid_draft_without_leaking_its_name() {
    let engine = Engine::open_memory().await.unwrap();
    let (owner, key) = actor(&engine, "Address reader").await;
    let (passenger, _) = actor(&engine, "Other party").await;
    let endpoint = record(&engine, "Secret Street4", nucleus::RecordKind::Plain, 0.0).await;
    let destination = record(
        &engine,
        "Public destination",
        nucleus::RecordKind::Plain,
        0.0,
    )
    .await;
    for (target, address, latitude) in [
        (endpoint.clone(), "Secret Street4", -23.5),
        (destination.clone(), "Public destination", -23.6),
    ] {
        engine
            .act(
                Backend::SetPlace {
                    target,
                    lat: latitude,
                    lon: -46.6,
                    address: Some(address.into()),
                },
                None,
            )
            .await
            .unwrap();
    }
    let role = store::auth::ensure_role(&engine.store.pool, "Transport composer")
        .await
        .unwrap();
    for resource in ["record", "transfer"] {
        for operation in ["read", "create", "update"] {
            let permission =
                store::auth::ensure_permission(&engine.store.pool, resource, operation)
                    .await
                    .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
    }
    store::auth::set_user_role(&engine.store.pool, &owner, role)
        .await
        .unwrap();
    engine
        .set_read_filter(&owner, Some(&protein::Predicate::UidEq(owner.clone())))
        .await
        .unwrap();
    let mut form = preset(&owner, "Ride");
    form.data["invitees"] = json!([passenger]);
    form.data["promises"][0]["item"]["transport"]["origin"] = json!(endpoint);
    form.data["promises"][0]["item"]["transport"]["destination"] = json!(destination);
    let now = nucleus::execution::now();
    form.data["promises"][0]["window_start"] =
        json!((now - chrono::Duration::minutes(5)).to_rfc3339());
    form.data["promises"][0]["window_end"] =
        json!((now + chrono::Duration::minutes(30)).to_rfc3339());
    model::bind_preset(&mut form);
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    engine.set_signer(key).await.unwrap();
    let error = engine
        .act(
            serde_json::from_value(form.payload().unwrap()).unwrap(),
            Some(owner),
        )
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("Secret Street4"));
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap(),
        before
    );
    assert!(engine.location_sources().await.is_empty());
}

#[cfg_attr(test, tokio::test)]
async fn four_lince_transport_control_draft_finishes_sharing_before_later_settlement() {
    let driver = nucleus::new_uid("r");
    let passenger = nucleus::new_uid("r");
    let origin = nucleus::new_uid("r");
    let destination = nucleus::new_uid("r");
    let seat = nucleus::new_uid("r");
    let mut form = preset(&driver, "Ride");
    form.data["invitees"] = json!([passenger]);
    form.data["promises"][0]["uid"] = json!("bike-give");
    form.data["promises"][0]["record"] = json!(seat);
    form.data["promises"][0]["item"]["transport"] = json!({"activity":"ride","direction":"offer","origin":origin,"destination":destination,"count":1,"capacity":2});
    form.data["promises"][0]["window_start"] =
        json!((chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339());
    form.data["promises"][0]["window_end"] =
        json!((chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339());
    model::bind_preset(&mut form);
    let mut payload = form.payload().unwrap();
    let replacements = [
        (driver, "$ana"),
        (passenger, "$carlos"),
        (origin, "$journey-origin"),
        (destination, "$journey-destination"),
        (seat, "$bike"),
    ];
    fn bind(value: &mut Value, replacements: &[(String, &str)]) {
        match value {
            Value::String(text) => {
                if let Some((_, target)) = replacements.iter().find(|(source, _)| source == text) {
                    *text = (*target).into();
                }
            }
            Value::Array(values) => {
                for value in values {
                    bind(value, replacements);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    bind(value, replacements);
                }
            }
            _ => {}
        }
    }
    bind(&mut payload, &replacements);
    payload["request_id"] = json!("sale");
    payload["slug"] = json!("sale");
    let evidence = simulation::location_workflows::transport_journey_from_draft(
        serde_json::from_value(payload).unwrap(),
    )
    .await;
    assert!(evidence.viewers["b"].live.fix.is_some());
    assert_eq!(evidence.viewers["family"].live.radius_metres, Some(3000));
    assert!(!evidence.family_live_denial.is_empty());
}

crate::native::laboratory_cases! {
    async hidden_transport_address_refuses_a_valid_draft_without_leaking_its_name timeout 120,
    async four_lince_transport_control_draft_finishes_sharing_before_later_settlement timeout 120,
    async transport_presets_use_saved_addresses_and_reject_invalid_terms_without_side_effects timeout 120,
    actual_activity_presets_keep_parties_units_and_reviewed_route_identities_separate,
    actual_solution_shortcuts_create_independent_drafts_and_preserve_the_parent_request,
    async activity_drafts_roundtrip_revisions_and_agreements_without_inventing_delivery timeout 120,
    async alternative_and_child_drafts_roundtrip_links_and_do_not_agree_other_solutions timeout 120,
}
