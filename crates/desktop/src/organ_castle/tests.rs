use super::*;
use crate::sand_panel::tests::{app, connect, settle};
use bevy::text::EditableText;
use image::ImageEncoder;

#[path = "tests/social_journey.rs"]
mod social_journey;

fn fixture() -> (App, Entity) {
    let mut app = app();
    app.add_plugins(OrganCastlePlugin);
    let root = app.world_mut().spawn_empty().id();
    let owner = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::Organ,
        "",
        bevy::math::DVec2::ZERO,
    );
    (app, owner)
}

fn find(world: &mut World, action: &str) -> Entity {
    world
        .query::<(Entity, &forms::Form)>()
        .iter(world)
        .find(|(_, form)| form.payload["action"] == action)
        .unwrap_or_else(|| panic!("Missing action {action}"))
        .0
}

fn set(world: &mut World, form: Entity, path: &str, value: &str) {
    let field = forms::input_text(world, form, path).unwrap();
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(value);
}

#[test]
fn scopes_preserve_all_named_and_minimum_states() {
    assert_eq!(forms::scope(&json!("all"), "head").unwrap(), Value::Null);
    assert_eq!(forms::scope(&json!("none"), "head").unwrap(), json!([]));
    assert_eq!(
        forms::scope(&json!("some"), "head, body,head").unwrap(),
        json!(["body", "head"])
    );
    assert!(forms::scope(&json!("some"), " , ").is_err());
    assert!(forms::scope(&json!("bad"), "head").is_err());
}

#[test]
fn pairing_and_enrolment_qr_round_trip_with_a_quiet_border() {
    for code in [
        "lince1|node|root|bGluY2U=|",
        "lincecell1|node|organ|root|secret|",
    ] {
        let (width, rgba) = qr::pixels(code).unwrap();
        assert!(rgba[..width as usize * 4 * 16].iter().all(|b| *b == 255));
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&rgba, width, width, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert_eq!(qr::decode_image(&png).unwrap(), code);
    }
    assert!(qr::decode_image(b"invalid").is_err());
    assert!(qr::decode_image(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
}

#[test]
fn all_contact_controls_use_typed_actions_and_do_not_persist_secrets() {
    let (mut app, owner) = fixture();
    let row = json!({"uid":"peer","head":"Friend","body":"","contact":{"delivery":"auto","trust":"known","proximity":1,"node_id":"abc","sync_out":true,"sync_in":false,"scope_fields":null,"accept_fields":[],"hidden_records":[{"uid":"hidden","head":"Private"}],"quarantined":[]},"extension":{"enabled":false,"path":"/tmp/notes","filter":"","formats":["lingua"]}});
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: subscription(owner, "organs"),
            rows: vec![row.clone()],
        },
    );
    let actions = [
        "rename-organ-contact",
        "forget-organ-contact",
        "set-contact-trust",
        "set-contact-proximity",
        "set-sync-policy",
        "set-contact-delivery",
        "reconnect-contact",
        "set-contact-scope",
        "set-contact-accept-scope",
        "hide-record-from-contact",
        "start-conversation",
        "grant-organ-login",
        "revoke-organ-login",
        "audit-contact",
        "set-extension",
        "file-sync-status",
        "add-known-organ",
        "roster-enrol-token",
        "roster-join-organ",
        "root-key-export",
        "root-key-detach",
        "roster-status",
        "mailbox-status",
        "mailbox-saved-status",
        "mailbox-set-copies",
        "mailbox-requests",
        "mailbox-pickup-points",
        "mailbox-outbound",
        "mailbox-collect-now",
        "mailbox-issue-invite",
        "mailbox-carry-for",
        "mailbox-ask-carry",
        "mailbox-add-pickup",
        "mailbox-mail-now",
        "mailbox-stop-carrying",
        "mailbox-remove-pickup",
        "mailbox-answer-request",
        "mailbox-use-invite",
    ];
    for action in actions {
        let form = find(app.world_mut(), action);
        let payload = forms::payload(app.world(), form).unwrap();
        let _: engine::actions::Action =
            serde_json::from_value(payload).unwrap_or_else(|e| panic!("{action}: {e}"));
    }
    let rename = find(app.world_mut(), "rename-organ-contact");
    set(app.world_mut(), rename, "/name", "Draft name");
    receive(
        app.world_mut(),
        &ServerMessage::Update {
            id: subscription(owner, "organs"),
            rows: vec![row],
        },
    );
    assert_eq!(
        forms::payload(app.world(), rename).unwrap()["name"],
        "Draft name"
    );
    let join = find(app.world_mut(), "roster-join-organ");
    set(app.world_mut(), join, "/code", "private enrolment code");
    assert!(crate::sand_text::snapshot(app.world(), owner).is_empty());
    let forgot = find(app.world_mut(), "forget-organ-contact");
    forms::Submit.apply(app.world_mut(), forgot);
    assert!(
        app.world()
            .get::<forms::Form>(forgot)
            .unwrap()
            .pending
            .is_none()
    );
    assert!(app.world().resource::<Requests>().actions.is_empty());
    let fs = find(app.world_mut(), "set-extension");
    set(
        app.world_mut(),
        fs,
        "/fds/filter",
        "{\"not_a_predicate\":true}",
    );
    assert!(forms::payload(app.world(), fs).is_err());
}

#[test]
fn saved_mail_recovery_uses_typed_actions_without_persisting_ciphertext() {
    let (mut app, owner) = fixture();
    let form = find(app.world_mut(), "mailbox-saved-status");
    finish(
        app.world_mut(),
        form,
        Ok(
            json!({"saved_mail":[{"uid":"mb-example","state":"quarantine","error":"Missing keys"}]}),
        ),
    );
    let retry = find(app.world_mut(), "mailbox-retry-saved");
    let payload = forms::payload(app.world(), retry).unwrap();
    assert_eq!(
        payload,
        json!({"action":"mailbox-retry-saved","uid":"mb-example"})
    );
    assert!(matches!(
        serde_json::from_value::<engine::actions::Action>(payload).unwrap(),
        engine::actions::Action::MailboxRetrySaved { uid } if uid == "mb-example"
    ));
    assert!(crate::sand_text::snapshot(app.world(), owner).is_empty());
}

#[test]
fn scan_fills_only_and_failed_actions_preserve_drafts() {
    let (mut app, _) = fixture();
    let add = find(app.world_mut(), "add-known-organ");
    let scan = find(app.world_mut(), "scan-file");
    finish(
        app.world_mut(),
        scan,
        Ok(json!({"scanned":"lince1|node|||"})),
    );
    assert_eq!(
        forms::payload(app.world(), add).unwrap()["invite"],
        "lince1|node|||"
    );
    assert!(app.world().resource::<Requests>().actions.is_empty());
    app.world_mut().get_mut::<forms::Form>(add).unwrap().pending = Some("test".into());
    app.world_mut()
        .resource_mut::<Requests>()
        .actions
        .insert("test".into(), (add, std::time::Instant::now()));
    receive(
        app.world_mut(),
        &ServerMessage::Error {
            id: "test".into(),
            message: "Refused".into(),
            code: None,
        },
    );
    assert!(
        app.world()
            .get::<forms::Form>(add)
            .unwrap()
            .pending
            .is_none()
    );
    assert_eq!(
        forms::payload(app.world(), add).unwrap()["invite"],
        "lince1|node|||"
    );
}

#[tokio::test]
async fn contacts_pair_rename_scope_and_forget_through_the_cell() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let (mut app, owner) = fixture();
    connect(&mut app, engine.clone());
    app.update();
    let add = find(app.world_mut(), "add-known-organ");
    set(
        app.world_mut(),
        add,
        "/invite",
        &format!("lince1|{}|||", "ab".repeat(32)),
    );
    set(app.world_mut(), add, "/name", "Friend");
    forms::Submit.apply(app.world_mut(), add);
    settle(&mut app, |world| {
        world.get::<forms::Form>(add).unwrap().pending.is_none()
            && world
                .get::<OrganCastle>(owner)
                .unwrap()
                .rows
                .get("organs")
                .is_some_and(|rows| rows.iter().any(|row| row["head"] == "Friend"))
    })
    .await;
    let uid = app.world().get::<OrganCastle>(owner).unwrap().rows["organs"]
        .iter()
        .find(|row| row["head"] == "Friend")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_string();
    Command::Select(uid.clone()).apply(app.world_mut(), owner);
    let rename = find(app.world_mut(), "rename-organ-contact");
    set(app.world_mut(), rename, "/name", "Private name");
    forms::Submit.apply(app.world_mut(), rename);
    settle(&mut app, |world| {
        world.get::<forms::Form>(rename).unwrap().pending.is_none()
    })
    .await;
    let rows = protein::execute(&engine.store, &query("organs"))
        .await
        .unwrap();
    assert_eq!(
        rows.iter().find(|row| row["uid"] == uid).unwrap()["head"],
        "Private name"
    );
    let scope = find(app.world_mut(), "set-contact-scope");
    dispatch(
        app.world_mut(),
        scope,
        json!({"action":"set-contact-scope","target":uid,"fields":[]}),
    )
    .unwrap();
    settle(&mut app, |world| {
        world.get::<forms::Form>(scope).unwrap().pending.is_none()
    })
    .await;
    let rows = protein::execute(&engine.store, &query("organs"))
        .await
        .unwrap();
    assert_eq!(
        rows.iter().find(|row| row["uid"] == uid).unwrap()["contact"]["scope_fields"],
        json!([])
    );
    let forget = find(app.world_mut(), "forget-organ-contact");
    dispatch(
        app.world_mut(),
        forget,
        json!({"action":"forget-organ-contact","target":uid}),
    )
    .unwrap();
    settle(&mut app, |world| {
        world
            .get::<forms::Form>(forget)
            .is_none_or(|form| form.pending.is_none())
    })
    .await;
    let rows = protein::execute(&engine.store, &query("organs"))
        .await
        .unwrap();
    assert!(!rows.iter().any(|row| row["uid"] == uid));
}

#[test]
fn roster_and_pairing_updates_keep_contact_drafts_and_guard_device_removal() {
    let (mut app, owner) = fixture();
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: subscription(owner, "organs"),
            rows: vec![
                json!({"uid":"local","slug":"local-organ","head":"Me"}),
                json!({"uid":"peer","head":"Friend","contact":{"trust":"known","proximity":1}}),
            ],
        },
    );
    Command::Select("peer".into()).apply(app.world_mut(), owner);
    let rename = find(app.world_mut(), "rename-organ-contact");
    set(app.world_mut(), rename, "/name", "Unfinished name");
    receive(
        app.world_mut(),
        &ServerMessage::Update {
            id: subscription(owner, "pairing"),
            rows: vec![json!({"uid":"peer","extension":{"invite":"lince1|node|||"}})],
        },
    );
    assert_eq!(
        forms::payload(app.world(), rename).unwrap()["name"],
        "Unfinished name"
    );
    assert!(
        app.world_mut()
            .query::<&ImageNode>()
            .iter(app.world())
            .count()
            > 0
    );
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: subscription(owner, "roster"),
            rows: vec![
                json!({"uid":"local","slug":"local-organ","extension":{"version":1,"cells":[{"cell_uid":"a","label":"Laptop"},{"cell_uid":"b","label":"Phone"}]}}),
            ],
        },
    );
    let revoke = find(app.world_mut(), "roster-revoke-cell");
    let payload = forms::payload(app.world(), revoke).unwrap();
    let _: engine::actions::Action = serde_json::from_value(payload).unwrap();
    forms::Submit.apply(app.world_mut(), revoke);
    assert!(app.world().resource::<Requests>().actions.is_empty());
    assert_eq!(
        forms::payload(app.world(), rename).unwrap()["name"],
        "Unfinished name"
    );
}

#[test]
fn karma_controls_distinguish_exclusive_selection_explicit_addition_and_local_stop() {
    let (mut app, owner) = fixture();
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: subscription(owner, "organs"),
            rows: vec![json!({"uid":"local","slug":"local-organ","head":"Me"})],
        },
    );
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: subscription(owner, "roster"),
            rows: vec![
                json!({"uid":"local","slug":"local-organ","extension":{"version":7,"cells":[{"cell_uid":"a","label":"Laptop","capabilities":["karma"]},{"cell_uid":"b","label":"Phone","capabilities":[]}]}}),
            ],
        },
    );
    let forms: Vec<_> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter(|(_, form)| form.payload["action"] == "roster-set-karma-execution")
        .map(|(entity, form)| (entity, form.payload.clone(), form.confirmation.clone()))
        .collect();
    let exclusive = forms
        .iter()
        .find(|(_, payload, _)| {
            payload["cell_uid"] == "b"
                && payload["enabled"] == true
                && payload["additional"] == false
        })
        .unwrap();
    assert!(exclusive.2.is_none());
    let additional = forms
        .iter()
        .find(|(_, payload, _)| payload["additional"] == true)
        .unwrap();
    assert!(additional.2.is_some());
    assert_eq!(additional.1["expected_roster_version"], 7);
    let _: engine::actions::Action = serde_json::from_value(additional.1.clone()).unwrap();
    forms::Submit.apply(app.world_mut(), additional.0);
    assert!(app.world().resource::<Requests>().actions.is_empty());
    let local = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .filter(|form| form.payload["namespace"] == "lince.karma-runtime")
        .map(|form| form.payload["fds"]["running"].as_bool().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(local, vec![false, true]);
}

#[test]
fn social_composer_defaults_to_anonymous_and_preserves_exact_optional_fields() {
    let (mut app, _) = fixture();
    let form = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .find(|(_, form)| {
            form.payload["action"] == "social" && form.payload["request"]["command"] == "save-draft"
        })
        .unwrap()
        .0;
    set(
        app.world_mut(),
        form,
        "/request/draft/title",
        "Bicycle repair",
    );
    set(app.world_mut(), form, "/request/draft/quantity", "1.250");
    set(app.world_mut(), form, "/request/draft/unit", "hour");
    let payload = forms::payload(app.world(), form).unwrap();
    let action: engine::actions::Action = serde_json::from_value(payload.clone()).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::Social {
            request: nucleus::social::Command::SaveDraft { .. }
        }
    ));
    assert_eq!(payload["request"]["draft"]["mode"], "anonymous");
    assert_eq!(payload["request"]["draft"]["quantity"], "1.250");
    assert_eq!(payload["request"]["draft"]["concept"], Value::Null);
    assert_eq!(payload["request"]["draft"]["destinations"], json!([]));
}

#[test]
fn publication_confirmation_keeps_the_exact_signed_preview() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn(Node::default()).id();
    let preview = json!({"record":"private-post","preview_hash":"digest","document":{"title":"Bicycle repair","text":"Public text","mode":"anonymous","direction":"contribution","state":"active","redistribute":false,"expires_at":2000000000,"signature":"exact-signature","destinations":[]}});
    social::result(app.world_mut(), owner, parent, &preview);
    let form = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .find(|form| {
            form.payload["action"] == "social" && form.payload["request"]["command"] == "publish"
        })
        .unwrap();
    assert_eq!(form.payload["request"]["document"], preview["document"]);
    assert_eq!(form.payload["request"]["preview_hash"], "digest");
    assert!(form.confirmation.is_some());
}

#[test]
fn social_profile_images_require_an_explicit_load_and_posts_have_typed_paging() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn(Node::default()).id();
    let organ = nucleus::new_uid("r");
    let hash = "a".repeat(64);
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"profile":{"authority":{"organ":organ},"fields":{"name":"Workshop","avatar":hash,"banner":null},"destinations":[]},"state":"active"}),
    );
    assert!(app.world().resource::<Requests>().actions.is_empty());
    let loads: Vec<_> = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .filter(|form| form.payload["request"]["command"] == "fetch-profile-image")
        .map(|form| form.payload.clone())
        .collect();
    assert_eq!(loads.len(), 1);
    let _: engine::actions::Action = serde_json::from_value(loads[0].clone()).unwrap();
    let after = nucleus::new_uid("r");
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"posts":[],"next_posts_after":after,"profile":null,"jobs":[],"settings":nucleus::social::ServiceSettings::default()}),
    );
    let page = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .find(|form| form.payload["request"]["command"] == "post-page")
        .unwrap();
    let _: engine::actions::Action = serde_json::from_value(page.payload.clone()).unwrap();
    assert_eq!(page.payload["request"]["after"], after);
}

#[test]
fn social_profile_editing_buttons_are_disabled_for_a_viewing_device() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn(Node::default()).id();
    social::profile(
        app.world_mut(),
        owner,
        parent,
        &json!({"editor":{"can_edit":false,"can_rotate":false},"published":{"state":"active"},"destinations":[]}),
    );
    let forms: Vec<_> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter(|(entity, form)| {
            app.world()
                .get::<ChildOf>(*entity)
                .is_some_and(|child| child.parent() == parent)
                && matches!(
                    form.payload["request"]["command"].as_str(),
                    Some(
                        "save-profile"
                            | "rotate-profile-authority"
                            | "withdraw-profile"
                            | "import-profile-image-data"
                    )
                )
        })
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(forms.len(), 4);
    for form in forms {
        let buttons: Vec<_> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(entity, _)| {
                let mut current = entity;
                while let Some(child) = app.world().get::<ChildOf>(current) {
                    current = child.parent();
                    if current == form {
                        return Some(entity);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        assert!(buttons.into_iter().all(|button| {
            app.world()
                .get::<bevy::ui::InteractionDisabled>(button)
                .is_some()
        }));
    }
}

#[test]
fn social_requests_offer_independent_reveal_connect_and_unblock_actions() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    let root = nucleus::new_uid("r");
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"requests":[{"record":root,"title":"Private conversation","messages":[],"state":{"state":"accepted","local_accepted":true},"reveal":{"local":{"profile":{}},"peer":{"profile":{"fields":{"name":"Friend"},"expires_at":1}},"local_connect":false}}],"blocks":[{"context":nucleus::new_uid("r"),"peer":"private-identity"}],"can_edit":true}),
    );
    let actions: Vec<Value> = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .map(|f| f.payload.clone())
        .collect();
    for command in [
        "send-private",
        "reveal-profile",
        "connect-participant",
        "unblock-participant",
    ] {
        let payload = actions
            .iter()
            .find(|a| a["request"]["command"] == command)
            .unwrap();
        assert!(matches!(
            serde_json::from_value::<engine::actions::Action>(payload.clone()).unwrap(),
            engine::actions::Action::Social { .. }
        ));
    }
    assert!(
        !actions
            .iter()
            .any(|a| a["request"]["decision"] == "accept" || a["request"]["decision"] == "decline")
    );
}

#[test]
fn social_request_write_controls_are_disabled_on_a_viewing_device() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"requests":[{"record":nucleus::new_uid("r"),"messages":[],"state":{"state":"pending","local_accepted":false}}],"blocks":[{"context":nucleus::new_uid("r"),"peer":"private-identity"}],"receive_failures":[{"context":nucleus::new_uid("r"),"service":"mailbox","envelope":"opaque","error":"Waiting for keys","discard":false}],"can_edit":false}),
    );
    let forms: Vec<Entity> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter_map(|(e, f)| {
            if f.payload["request"]["command"] == "requests" {
                return None;
            }
            let mut node = e;
            while let Some(child) = app.world().get::<ChildOf>(node) {
                node = child.parent();
                if node == parent {
                    return Some(e);
                }
            }
            None
        })
        .collect();
    assert!(!forms.is_empty());
    for form in forms {
        let buttons: Vec<Entity> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(e, _)| {
                let mut node = e;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == form {
                        return Some(e);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        assert!(buttons.iter().all(|e| {
            app.world()
                .get::<bevy::ui::InteractionDisabled>(*e)
                .is_some()
        }));
    }
}

#[test]
fn social_ciphertext_discard_requires_review_and_refused_messages_cannot_resume() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({
            "requests":[{"record":nucleus::new_uid("r"),"messages":[{"uid":nucleus::new_uid("r"),"body":"Saved text","delivery":{"stage":"recipient-refused"}}],"state":{"state":"closed"}}],
            "receive_failures":[{"context":nucleus::new_uid("r"),"service":"mailbox","envelope":"opaque","error":"Waiting for keys","discard":false}],
            "can_edit":true
        }),
    );
    let actions: Vec<_> = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .collect();
    let discard = actions
        .iter()
        .find(|form| form.payload["request"]["command"] == "discard-private")
        .unwrap();
    let action: engine::actions::Action = serde_json::from_value(discard.payload.clone()).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::Social {
            request: nucleus::social::Command::DiscardPrivate { .. }
        }
    ));
    assert!(discard.confirmation.is_some());
    assert!(
        !actions
            .iter()
            .any(|form| form.payload["request"]["command"] == "resume-private")
    );
}

#[test]
fn social_publication_view_distinguishes_a_superseded_host_acknowledgement() {
    for state in ["cancelled", "failed", "expired", "pending", "accepted"] {
        let (mut app, owner) = fixture();
        let parent = app.world_mut().spawn_empty().id();
        social::result(
            app.world_mut(),
            owner,
            parent,
            &json!({"posts":[],"profile":null,"jobs":[{"kind":"profile","state":state,"destination":"selected host","receipt":{"accepted":true}}],"settings":nucleus::social::ServiceSettings::default()}),
        );
        assert_eq!(app.world_mut().query::<&Text>().iter(app.world()).any(|text|text.0=="The host acknowledged this earlier version; it is no longer the current publication."),matches!(state,"cancelled"|"failed"|"expired"));
        assert!(app.world().resource::<Requests>().actions.is_empty());
    }
}

#[test]
fn social_delivery_distinguishes_single_mailbox_storage_waiting_and_recipient_receipt() {
    let single = social::delivery_label(
        "mailbox-stored",
        &json!([{"service":"a","stored":1},{"service":"b","pending":1,"error":"Offline"}]),
    );
    assert!(single.contains("One mailbox"));
    assert!(single.contains("not confirmed receipt"));
    let multiple = social::delivery_label("mailbox-stored", &json!([{"stored":1},{"stored":2}]));
    assert!(multiple.contains("2 mailboxes"));
    assert!(social::delivery_label("waiting", &Value::Null).contains("waiting for keys"));
    assert_eq!(
        social::delivery_label("recipient-durable", &Value::Null),
        "The recipient saved this Message"
    );
    assert_eq!(
        social::delivery_label("recipient-refused", &Value::Null),
        "The recipient declined this Message"
    );
}

#[test]
fn unchanged_roster_refresh_keeps_an_open_device_confirmation() {
    let (mut app, owner) = fixture();
    let id = subscription(owner, "roster");
    receive(
        app.world_mut(),
        &ServerMessage::Snapshot {
            id: id.clone(),
            rows: vec![json!({"slug":"local-organ","extension":null})],
        },
    );
    let create = find(app.world_mut(), "roster-create-organ");
    forms::Submit.apply(app.world_mut(), create);
    let output = app.world().get::<forms::Form>(create).unwrap().output;
    let children = app.world().get::<Children>(output).unwrap().to_vec();
    receive(
        app.world_mut(),
        &ServerMessage::Update {
            id: id.clone(),
            rows: vec![json!({"slug":"local-organ","extension":null})],
        },
    );
    receive(
        app.world_mut(),
        &ServerMessage::Update {
            id,
            rows: vec![
                json!({"slug":"local-organ","extension":null}),
                json!({"slug":"contact","extension":null}),
            ],
        },
    );
    assert!(app.world().get::<forms::Form>(create).is_some());
    assert_eq!(
        app.world().get::<Children>(output).unwrap().to_vec(),
        children
    );
    assert!(children.iter().any(|child| {
        app.world()
            .get::<crate::icons::Tooltip>(*child)
            .is_some_and(|tooltip| tooltip.0 == "Confirm")
    }));
}

#[test]
fn social_connect_waits_for_current_profile_proofs_and_shows_recovery() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    let recovery = "Ask this person to reveal their current reviewed profile again";
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({
            "requests":[{"record":nucleus::new_uid("r"),"messages":[],"state":{"state":"accepted","local_accepted":true},"reveal":{"local":{"profile":{}},"peer":{"profile":{}},"local_connect":false},"verification":{"can_connect":false,"needs_fresh_proof":true,"recovery":recovery}}],"can_edit":true
        }),
    );
    let connect = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .find(|(_, form)| form.payload["request"]["command"] == "connect-participant")
        .unwrap()
        .0;
    let buttons: Vec<_> = app
        .world_mut()
        .query::<(Entity, &crate::actions::ActionButton)>()
        .iter(app.world())
        .filter_map(|(button, _)| {
            let mut node = button;
            while let Some(child) = app.world().get::<ChildOf>(node) {
                node = child.parent();
                if node == connect {
                    return Some(button);
                }
            }
            None
        })
        .collect();
    assert!(!buttons.is_empty());
    assert!(buttons.iter().all(|button| {
        app.world()
            .get::<bevy::ui::InteractionDisabled>(*button)
            .is_some()
    }));
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == recovery)
    );
}

#[test]
fn social_search_continuation_preserves_query_and_chosen_services() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    let after = nucleus::new_uid("post");
    let query = json!({"text":"bicycles","direction":"need","language":"pt","area":"Recife","concept":"","unit":"","after":null});
    let services = json!(["selected-directory"]);
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"results":[],"query":query,"services":services,"next_after":after}),
    );
    let form = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .find(|form| {
            form.payload["request"]["command"] == "search"
                && form.payload["request"]["query"]["after"] == after
        })
        .unwrap();
    assert_eq!(form.payload["request"]["services"], services);
    assert_eq!(form.payload["request"]["query"]["text"], "bicycles");
    assert_eq!(form.payload["request"]["query"]["area"], "Recife");
    let action: engine::actions::Action = serde_json::from_value(form.payload.clone()).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::Social {
            request: nucleus::social::Command::Search { .. }
        }
    ));
}

#[test]
fn social_server_inspection_is_deliberate_and_hosting_controls_respect_permissions() {
    let (mut app, owner) = fixture();
    let inspector = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .find(|form| form.payload["request"]["command"] == "inspect-service")
        .unwrap();
    let action: engine::actions::Action =
        serde_json::from_value(inspector.payload.clone()).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::Social {
            request: nucleus::social::Command::InspectService { .. }
        }
    ));
    let parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"posts":[],"profile":null,"jobs":[],"settings":nucleus::social::ServiceSettings::default(),"can_manage_services":false}),
    );
    let (entity, settings) = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .find(|(_, form)| form.payload["request"]["command"] == "configure-services")
        .unwrap();
    let _: engine::actions::Action = serde_json::from_value(settings.payload.clone()).unwrap();
    let buttons: Vec<_> = app
        .world_mut()
        .query::<(Entity, &crate::actions::ActionButton)>()
        .iter(app.world())
        .filter_map(|(button, _)| {
            let mut node = button;
            while let Some(child) = app.world().get::<ChildOf>(node) {
                node = child.parent();
                if node == entity {
                    return Some(button);
                }
            }
            None
        })
        .collect();
    assert!(!buttons.is_empty());
    assert!(buttons.iter().all(|button| {
        app.world()
            .get::<bevy::ui::InteractionDisabled>(*button)
            .is_some()
    }));
    assert!(app.world().resource::<Requests>().actions.is_empty());
}

#[test]
fn remembered_server_roles_fill_separate_native_actions_without_automatic_requests() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    let publication = "publication-endpoint";
    let directory = "query-endpoint";
    let mailbox = "mailbox-endpoint";
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"servers":[
        {"endpoint":publication,"label":"Publisher","operator":"A","publication":true,"query":false,"mailbox":false},
        {"endpoint":directory,"label":"Directory","operator":"B","publication":false,"query":true,"mailbox":false},
        {"endpoint":mailbox,"label":"Mailbox","operator":"C","publication":false,"query":false,"mailbox":true}
    ],"can_manage_services":true}),
    );
    let forms: Vec<_> = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .collect();
    let query = forms
        .iter()
        .find(|form| {
            form.payload["request"]["command"] == "search"
                && form.payload["request"]["services"] == json!([directory])
        })
        .unwrap();
    let draft = forms
        .iter()
        .find(|form| {
            form.payload["request"]["command"] == "save-draft"
                && form.payload["request"]["draft"]["destinations"] == json!([publication])
        })
        .unwrap();
    let save = forms
        .iter()
        .find(|form| {
            form.payload["request"]["command"] == "save-server"
                && form.payload["request"]["choice"]["endpoint"] == directory
        })
        .unwrap();
    let remove = forms
        .iter()
        .find(|form| {
            form.payload["request"]["command"] == "remove-server"
                && form.payload["request"]["endpoint"] == mailbox
        })
        .unwrap();
    assert!(remove.confirmation.is_some());
    for form in [query, draft, save, remove] {
        let _: engine::actions::Action = serde_json::from_value(form.payload.clone()).unwrap();
    }
    assert!(app.world().resource::<Requests>().actions.is_empty());
    let result_parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        result_parent,
        &json!({"results":[{"document":{"id":nucleus::new_uid("post"),"title":"Help","reply":{},"state":"active"}}]}),
    );
    let intro = app
        .world_mut()
        .query::<&forms::Form>()
        .iter(app.world())
        .find(|form| form.payload["request"]["command"] == "open-request")
        .unwrap();
    assert_eq!(intro.payload["request"]["services"], json!([mailbox]));
}

#[test]
fn deployment_managed_hosting_disables_role_edits_but_keeps_deliberate_operator_actions() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"posts":[],"settings":nucleus::social::ServiceSettings::default(),"can_manage_services":true,"services_managed":true}),
    );
    let controls: Vec<_> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter(|(_, form)| {
            matches!(
                form.payload["request"]["command"].as_str(),
                Some("configure-services" | "service-health" | "rebuild-public-index")
            )
        })
        .map(|(entity, form)| (entity, form.payload.clone(), form.confirmation.is_some()))
        .collect();
    assert_eq!(controls.len(), 3);
    for (entity, payload, confirmation) in controls {
        let _: engine::actions::Action = serde_json::from_value(payload.clone()).unwrap();
        if payload["request"]["command"] == "rebuild-public-index" {
            assert!(confirmation);
        }
        let buttons: Vec<_> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(button, _)| {
                let mut node = button;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == entity {
                        return Some(button);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        for button in buttons {
            assert_eq!(
                app.world()
                    .get::<bevy::ui::InteractionDisabled>(button)
                    .is_some(),
                payload["request"]["command"] == "configure-services"
            );
        }
    }
    assert!(app.world().resource::<Requests>().actions.is_empty());
}

#[test]
fn gossip_controls_require_device_and_separate_contact_consent_without_private_sync_grants() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"gossip":{"enabled":false,"queued":0,"contacts":[{"name":"Friend","endpoint":null,"choice":{"organ":nucleus::new_uid("r"),"send":false,"receive":false}}]},"can_manage_services":false}),
    );
    let forms: Vec<_> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter(|(_, form)| {
            matches!(
                form.payload["request"]["command"].as_str(),
                Some("configure-gossip" | "set-gossip-contact")
            )
        })
        .map(|(entity, form)| (entity, form.payload.clone()))
        .collect();
    assert_eq!(forms.len(), 2);
    for (entity, payload) in forms {
        let _: engine::actions::Action = serde_json::from_value(payload.clone()).unwrap();
        assert_ne!(payload["request"]["choice"]["send"], true);
        let buttons: Vec<_> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(button, _)| {
                let mut node = button;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == entity {
                        return Some(button);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        assert!(buttons.iter().all(|button| {
            app.world()
                .get::<bevy::ui::InteractionDisabled>(*button)
                .is_some()
        }));
    }
    assert!(app.world().resource::<Requests>().actions.is_empty());
}

#[test]
fn contact_query_controls_are_typed_deliberate_and_disabled_for_a_viewing_device() {
    let (mut app, owner) = fixture();
    let parent = app.world_mut().spawn_empty().id();
    let peer = nucleus::new_uid("r");
    social::result(
        app.world_mut(),
        owner,
        parent,
        &json!({"asks":{"enabled":true,"contacts":[{"name":"Friend","endpoint":"pinned-contact","choice":{"organ":peer,"ask":true,"answer":false,"forward":false}}],"queries":[{"id":nucleus::new_uid("ask"),"query":nucleus::social::Search::default(),"state":"pending","deadline":30,"count":0}]},"can_manage_services":false}),
    );
    let forms: Vec<_> = app
        .world_mut()
        .query::<(Entity, &forms::Form)>()
        .iter(app.world())
        .filter(|(_, form)| {
            matches!(
                form.payload["request"]["command"].as_str(),
                Some(
                    "configure-ask" | "set-ask-contact" | "start-ask" | "cancel-ask" | "clear-asks"
                )
            )
        })
        .map(|(entity, form)| (entity, form.payload.clone(), form.confirmation.clone()))
        .collect();
    assert_eq!(forms.len(), 5);
    for (entity, payload, confirmation) in forms {
        let _: engine::actions::Action =
            serde_json::from_value(forms::payload(app.world(), entity).unwrap()).unwrap();
        if payload["request"]["command"] == "start-ask" {
            assert_eq!(payload["request"]["contacts"], json!([peer]));
            assert!(confirmation.as_deref().unwrap().contains("pass it onward"));
        }
        let buttons: Vec<_> = app
            .world_mut()
            .query::<(Entity, &crate::actions::ActionButton)>()
            .iter(app.world())
            .filter_map(|(button, _)| {
                let mut node = button;
                while let Some(child) = app.world().get::<ChildOf>(node) {
                    node = child.parent();
                    if node == entity {
                        return Some(button);
                    }
                }
                None
            })
            .collect();
        assert!(!buttons.is_empty());
        assert!(buttons.iter().all(|button| {
            app.world()
                .get::<bevy::ui::InteractionDisabled>(*button)
                .is_some()
        }));
    }
    assert!(app.world().resource::<Requests>().actions.is_empty());
}
