use super as karma;
use super::*;
use crate::cell_bridge::CellBridge;
use nucleus::transfer::{
    AgreementGuard,
    karma::{Participant, Snapshot},
};
use std::collections::BTreeMap;

#[path = "../../../engine/tests/support/mod.rs"]
mod support;

fn state(level: u8, revision: u64) -> Snapshot {
    Snapshot {
        transfer: "r_K7T3ZG08G5EBWRSTF8NBWMTZYV".into(),
        revision,
        active: false,
        published: false,
        ready: false,
        participants: BTreeMap::from([(
            "r_K7T3ZG08G5EBWRSTF8NBWMTZYW".into(),
            Participant {
                guard: AgreementGuard {
                    level,
                    change_uid: Some(format!("change-{level}")),
                },
                changed_at_ms: Some(0),
            },
        )]),
    }
}

fn setup() -> (App, Entity, String) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let before = state(2, 1);
    let person = before.participants.keys().next().unwrap().clone();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            selected: before.transfer.clone(),
            ..Default::default()
        },
    );
    app.world_mut().get_mut::<View>(owner).unwrap().rows =
        vec![json!({"uid":before.transfer,"revision":1,"head":"Trade"})];
    app.world_mut().entity_mut(owner).insert(Automation {
        transfer: before.transfer.clone(),
        person: person.clone(),
        data: Some(json!({"transfer":before.transfer,"person":person,"state":before,"rules":[]})),
        request: Some("inspect".into()),
        ..Default::default()
    });
    (app, owner, person)
}

#[test]
fn a_queued_acknowledgement_does_not_show_a_retreat_or_pause_rules() {
    let (mut app, owner, _) = setup();
    assert!(!karma::receive(
        app.world_mut(),
        &cell::ServerMessage::ActionOk {
            id: "queued-assignment".into(),
            created: Some("command".into()),
            facts: 0,
            warnings: Vec::new(),
            data: None
        }
    ));
    let state = app.world().get::<Automation>(owner).unwrap();
    assert!(!state.retreat);
    assert!(state.pauses.is_empty());
    assert!(state.pause_request.is_none());
}

#[test]
fn actual_retreat_recommends_pausing_but_term_reset_does_not() {
    for (revision, retreat) in [(1, true), (2, false)] {
        let (mut app, owner, person) = setup();
        let after = state(0, revision);
        assert!(karma::receive(
            app.world_mut(),
            &cell::ServerMessage::ActionOk {
                id: "inspect".into(),
                created: None,
                facts: 0,
                warnings: Vec::new(),
                data: Some(
                    json!({"transfer":after.transfer,"person":person,"state":after,"rules":[{"uid":"rule","revision":1,"name":"Advance agreement","paused":false,"effects":[{"fixed":true,"target":2,"can_raise_to_maximum":true}],"pending":[],"commands":[]}]})
                )
            }
        ));
        let state = app.world().get::<Automation>(owner).unwrap();
        assert_eq!(state.retreat, retreat);
        assert!(state.pauses.is_empty());
        assert!(state.pause_request.is_none());
    }
}

#[test]
fn direct_targets_include_the_selected_person_and_original_guard() {
    let (app, owner, person) = setup();
    let transfer = &app.world().get::<View>(owner).unwrap().rows[0];
    for level in 0..=2 {
        let payload = karma::target_action(app.world(), owner, transfer, &person, level).unwrap();
        let action: engine::actions::Action =
            serde_json::from_value(Form::action("Set agreement", payload).payload().unwrap())
                .unwrap();
        let engine::actions::Action::AssignTransferAgreementLevel {
            person: acting,
            level: target,
            expected,
            expected_state,
            ..
        } = action
        else {
            panic!("target assignment");
        };
        assert_eq!(acting, Some(person.clone()));
        assert_eq!(target, level);
        assert_eq!(expected.unwrap().level, 2);
        assert_eq!(expected_state.unwrap().participants.len(), 1);
    }
    assert!(
        karma::target_action(
            app.world(),
            owner,
            &json!({"uid":transfer["uid"],"revision":2}),
            &person,
            2
        )
        .is_none()
    );
    assert!(karma::target_action(app.world(), owner, transfer, "another-person", 2).is_none());
}

#[test]
fn selecting_pause_changes_only_the_selected_rule_through_the_live_cell() {
    std::thread::Builder::new().name("native-transfer-pause".into()).stack_size(16 * 1024 * 1024).spawn(|| {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let engine = support::engine().await;
        let a = support::person(&engine, "a").await;
        let b = support::person(&engine, "b").await;
        let stock = support::plain(&engine, "stock", 10.0).await;
        let transfer = support::create_transfer(&engine, &a, &[b.clone()], vec![support::promise("promise-a", &stock, &a, -1.0), support::promise("promise-b", &stock, &b, 1.0)], support::DraftOptions { slug: "trade", agreement: nucleus::transfer::AgreementType::Full, reserve_default: engine::actions::TransferReservePoint::Active, require_confirmation: true }).await;
        engine.set_signer(a.signer.clone()).await.unwrap();
        let mut rules = Vec::new();
        for request_id in ["first", "second"] {
            rules.push(engine.act(engine::actions::Action::SaveKarmaRule { identity: None, rule: None, expected_revision: None, fields: ["agreement_level(@trade, @a)", "always", "@trade: agreement(@a, 2)"].map(|source| nucleus::karma::rule_field::RuleFieldInput::Text { source: source.into() }), request_id: request_id.into() }, None).await.unwrap().created.unwrap());
        }
        let data = engine.act(engine::actions::Action::InspectTransferKarma { transfer: transfer.transfer.clone(), person: Some(a.uid.clone()) }, None).await.unwrap().data.unwrap();
        let engine = std::sync::Arc::new(engine);
        let runtime = cell::CellRuntime { commands: Default::default(), store: engine.store.clone(), engine: engine.clone(), lanes: std::sync::Arc::new(cell::LaneHub::new()), wire: Default::default(), fiote: None, speech: None, information: None };
        let bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
        let (mut app, owner, _) = setup();
        app.world_mut().get_mut::<TransferCastle>(owner).unwrap().selected = transfer.transfer.clone();
        app.world_mut().get_mut::<View>(owner).unwrap().rows = vec![json!({"uid":transfer.transfer,"revision":transfer.revision,"head":"Trade"})];
        app.world_mut().entity_mut(owner).insert(Automation { transfer: transfer.transfer, person: a.uid, data: Some(data), selected: HashSet::from([rules[0].clone()]), retreat: true, ..Default::default() });
        app.world_mut().insert_non_send(bridge);
        Command::Pause(false).apply(app.world_mut(), owner);
        let request = app.world().get::<Automation>(owner).unwrap().pause_request.clone().unwrap();
        loop {
            let message = tokio::time::timeout(std::time::Duration::from_secs(5), app.world_mut().non_send_mut::<CellBridge>().incoming.recv()).await.unwrap().unwrap();
            let done = matches!(&message, cell::ServerMessage::ActionOk { id, .. } if *id == request);
            karma::receive(app.world_mut(), &message);
            if done { break; }
        }
        assert!(store::recurrence::get(&engine.store.pool, &rules[0]).await.unwrap().unwrap().is_paused());
        assert!(!store::recurrence::get(&engine.store.pool, &rules[1]).await.unwrap().unwrap().is_paused());
        assert!(app.world().get::<Automation>(owner).unwrap().pauses.is_empty());
        });
    }).unwrap().join().unwrap();
}

#[test]
fn opening_a_related_rule_preserves_other_unsaved_drafts() {
    let (mut app, owner, _) = setup();
    app.add_plugins(crate::karma_castle::KarmaCastlePlugin);
    let root = app.world().get::<ChildOf>(owner).unwrap().parent();
    let draft = lince_interface::karma::Draft {
        name: "Unrelated unsaved Rule".into(),
        slug: "unfinished".into(),
        fields: ["@room", "!= 0", "@room: 0"].map(|text| lince_interface::karma::FieldDraft {
            text: text.into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let existing = crate::karma_castle::spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        crate::karma_castle::KarmaCastle {
            draft: Some(draft.clone()),
            search: "Unrelated unsaved draft".into(),
            ..Default::default()
        },
    );
    Command::Edit("selected-rule".into()).apply(app.world_mut(), owner);
    assert_eq!(
        app.world()
            .get::<crate::karma_castle::KarmaCastle>(existing)
            .unwrap()
            .draft
            .as_ref(),
        Some(&draft)
    );
    assert_eq!(
        app.world()
            .get::<crate::karma_castle::KarmaCastle>(existing)
            .unwrap()
            .search,
        "Unrelated unsaved draft"
    );
    assert_eq!(
        app.world_mut()
            .query::<&crate::karma_castle::KarmaCastle>()
            .iter(app.world())
            .count(),
        2
    );
}
