use super::*;
use crate::sand_panel::tests::{app, connect, settle};
use bevy::text::EditableText;
use image::ImageEncoder;

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
    let row = json!({"uid":"peer","head":"Friend","body":"","contact":{"trust":"known","proximity":1,"node_id":"abc","sync_out":true,"sync_in":false,"scope_fields":null,"accept_fields":[],"hidden_records":[{"uid":"hidden","head":"Private"}],"quarantined":[]},"extension":{"enabled":false,"path":"/tmp/notes","filter":"","formats":["lingua"]}});
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
