use super::*;

fn fixture(directory: &std::path::Path, person: &str) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<lince_interface::theme::Typography>();
    world.init_resource::<bevy::input_focus::InputFocus>();
    let mut mobile = Mobile::new(directory.into());
    mobile.ready = true;
    mobile.identity = crate::session::Identity::Person(person.into());
    mobile.navigation.open(Page::Transfers);
    world.insert_resource(mobile);
    let parent = world.spawn(Node::default()).id();
    (world, parent)
}

fn button(world: &mut World, parent: Entity, caption: &str) {
    app::capture(world);
    controls::clear(world, parent);
    render(world, parent);
    let entity = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == caption)
        .unwrap_or_else(|| panic!("Missing {caption}"))
        .0;
    let button = world.get::<ChildOf>(entity).unwrap().parent();
    let app::Intent::Workflow(intent) = world.get::<app::ButtonIntent>(button).unwrap().0.clone()
    else {
        panic!("Workflow button")
    };
    apply(world, intent).unwrap();
    controls::clear(world, parent);
    render(world, parent);
}

fn field(world: &mut World, path: &str, value: &str) {
    let id = world
        .resource::<State>()
        .form
        .as_ref()
        .unwrap()
        .request_id
        .clone();
    let key = format!("workflow-{id}/{path}");
    let entity = world
        .query::<(Entity, &app::Input)>()
        .iter(world)
        .find(|(_, input)| input.key == key)
        .unwrap()
        .0;
    world
        .get_mut::<bevy::text::EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

#[test]
fn real_mobile_transport_controls_review_one_normal_draft_and_clear_changed_person_results() {
    let temporary = tempfile::tempdir().unwrap();
    let person = nucleus::new_uid("r");
    let passenger = nucleus::new_uid("r");
    let (mut world, parent) = fixture(temporary.path(), &person);
    button(&mut world, parent, "Ride");
    for (path, value) in [
        ("/promises/0/item/transport/origin", nucleus::new_uid("r")),
        (
            "/promises/0/item/transport/destination",
            nucleus::new_uid("r"),
        ),
        ("/promises/0/window_start", "2026-10-17T12:00:00Z".into()),
        ("/promises/0/window_end", "2026-10-17T13:00:00Z".into()),
    ] {
        field(&mut world, path, &value);
    }
    button(&mut world, parent, "Next");
    field(&mut world, "/invitees", &passenger);
    button(&mut world, parent, "Next");
    button(&mut world, parent, "Next");
    button(&mut world, parent, "Review draft");
    assert_eq!(
        world.resource::<State>().form.as_ref().unwrap().step,
        Some(4)
    );
    assert!(world.resource::<State>().review.is_some());
    let payload = world
        .resource::<State>()
        .form
        .as_ref()
        .unwrap()
        .payload()
        .unwrap();
    let action: engine::actions::Action = serde_json::from_value(payload).unwrap();
    let engine::actions::Action::CreateTransferDraft { promises, .. } = action else {
        panic!("Ordinary draft")
    };
    assert_eq!(
        promises[0]
            .item
            .as_ref()
            .unwrap()
            .transport
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        promises[0]
            .item
            .as_ref()
            .unwrap()
            .exchange
            .as_ref()
            .unwrap()
            .receiver,
        passenger
    );
    world.resource_mut::<State>().pending = Some(("mobile-workflow_old".into(), "analysis".into()));
    world.resource_mut::<State>().result = json!({"private":"old actor"});
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    ensure_scope(&mut world);
    assert!(world.resource::<State>().form.is_none());
    assert!(world.resource::<State>().result.is_null());
    receive(
        &mut world,
        &cell::ServerMessage::ActionOk {
            id: "mobile-workflow_old".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(json!({"private":"late actor"})),
        },
    );
    assert!(world.resource::<State>().result.is_null());
}

#[test]
fn restore_keeps_only_editable_person_drafts_and_requires_review_again() {
    let temporary = tempfile::tempdir().unwrap();
    let person = nucleus::new_uid("r");
    let (mut world, parent) = fixture(temporary.path(), &person);
    button(&mut world, parent, "Delivery");
    {
        let mut state = world.resource_mut::<State>();
        let form = state.form.as_mut().unwrap();
        form.step = Some(3);
        form.review = json!({"old approval":"never restore"});
    }
    persist_draft(&mut world);
    let mobile = world.resource::<Mobile>();
    let saved = crate::storage::Saved {
        organ: "test-organ".into(),
        drafts: mobile.drafts.clone(),
        ..Default::default()
    };
    crate::storage::write(temporary.path(), &saved).unwrap();
    let raw = std::fs::read_to_string(temporary.path().join("mobile-drafts.json")).unwrap();
    assert!(!raw.contains("old approval"));
    assert!(!raw.contains("session_uid"));
    assert!(!raw.contains("captured_at_ms"));
    let restored = crate::storage::read(temporary.path(), &saved.organ)
        .unwrap()
        .unwrap();
    let (mut next, _) = fixture(temporary.path(), &person);
    next.resource_mut::<Mobile>().drafts = restored.drafts;
    ensure_scope(&mut next);
    assert!(next.resource::<State>().form.is_some());
    assert!(next.resource::<State>().review.is_none());
    assert!(next.resource::<State>().pending.is_none());
    assert!(next.resource::<State>().remote_review.is_none());
    assert!(next.resource::<State>().result.is_null());
    assert!(
        next.resource::<State>()
            .form
            .as_ref()
            .unwrap()
            .review
            .is_null()
    );
    next.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    ensure_scope(&mut next);
    assert!(next.resource::<State>().form.is_none());
}

#[test]
fn credential_forms_use_normal_review_and_scope_changes_clear_private_drafts() {
    let directory = tempfile::tempdir().unwrap();
    let person = nucleus::new_uid("r");
    let (mut world, parent) = fixture(directory.path(), &person);
    ensure_scope(&mut world);
    let meaning = nucleus::vocabulary::reference::MeaningReference {
        vocabulary_uid: nucleus::new_uid("g"),
        concept_uid: nucleus::new_uid("c"),
        revision: 1,
        definition_hash: "a".repeat(64),
    };
    let descriptor = nucleus::trust_analysis::claims::Descriptor {
        subject: nucleus::trust_analysis::Identity::Person(nucleus::new_uid("r")),
        role: nucleus::transfer::outcome::Role::Giver,
        measurement: nucleus::transfer::receipt::Measurement {
            meaning,
            unit: None,
            object_commitment: None,
        },
        level: nucleus::trust_analysis::query::EvidenceLevel::NeedLinkedReceiver,
        metric: nucleus::trust_analysis::claims::Metric::Quantity,
        counterparty_basis: nucleus::trust_analysis::query::CounterpartyBasis::Organ,
        from: None,
        until: None,
    };
    let form = claim_form(
        &person,
        json!({"category":"credential","issuer":{"kind":"person","uid":person},"claim":null,"title":"Activity credential","descriptor":descriptor,"comparator":"at_least","amount":"1","as_of":null,"lifetime_seconds":3600,"expected_revision":0,"withdrawn":false,"audience":{"kind":"selected","viewers":[{"kind":"person","uid":person}]}}),
    );
    open(&mut world, form).unwrap();
    render(&mut world, parent);
    assert!(world.resource::<State>().review.is_none());
    apply(&mut world, Intent::ReviewAction).unwrap();
    assert!(world.resource::<State>().review.is_some());
    let payload = world
        .resource::<State>()
        .form
        .as_ref()
        .unwrap()
        .payload()
        .unwrap();
    let action: engine::actions::Action = serde_json::from_value(payload).unwrap();
    assert!(matches!(
        action,
        engine::actions::Action::TrustAnalysis {
            request: nucleus::trust_analysis::commands::Command::Claims {
                request: nucleus::trust_analysis::claims::commands::Command::Preview { .. }
            },
            ..
        }
    ));
    world.resource_mut::<Mobile>().identity =
        crate::session::Identity::Person(nucleus::new_uid("r"));
    ensure_scope(&mut world);
    assert!(world.resource::<State>().form.is_none());
    assert!(world.resource::<State>().review.is_none());
}
