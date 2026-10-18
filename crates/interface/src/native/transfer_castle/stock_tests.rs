use super::*;
use cell::ServerMessage;
use nucleus::transfer::surplus::Preview;

fn activate(world: &mut World, owner: Entity, caption: &str) {
    let labels = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .filter(|(_, text)| text.0 == caption)
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    let button = labels
        .into_iter()
        .find_map(|label| {
            let button = world.get::<ChildOf>(label)?.parent();
            let mut ancestor = Some(button);
            while let Some(entity) = ancestor {
                if entity == owner {
                    return Some(button);
                }
                ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            None
        })
        .unwrap_or_else(|| panic!("Missing {caption}"));
    if let Some(action) = world.get::<crate::native::actions::ActionButton>(button) {
        let (target, actions) = (action.target, action.actions.clone());
        actions.run(world, target);
    } else {
        world.trigger(bevy::ui_widgets::Activate { entity: button });
        world.flush();
    }
}

fn setup() -> (App, Entity) {
    let mut app = super::super::tests::app();
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let person = nucleus::new_uid("r");
    let transfer = nucleus::new_uid("r");
    let stock = nucleus::new_uid("r");
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.clone(),
            selected: transfer.clone(),
            ..Default::default()
        },
    );
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    view.ready = true;
    view.context = json!({"viewer":{"local":true},"capabilities":{"create":true}});
    view.rows = vec![
        json!({"uid":transfer,"revision":1,"head":"Apples","capabilities":{"edit_terms":true},"promises":[{"uid":"apples","record":stock,"party":person,"title":"Apples","delta":-10.0}],"parties":[],"occurrences":[]}),
    ];
    ui::render(app.world_mut(), owner);
    activate(app.world_mut(), owner, "Stock policy · Apples");
    (app, owner)
}

fn editing(world: &World, owner: Entity) -> Form {
    world
        .get::<TransferCastle>(owner)
        .unwrap()
        .form
        .clone()
        .unwrap()
}

fn preview() -> Preview {
    Preview {
        actual: "29".into(),
        available: "29".into(),
        reserved: "9".into(),
        reserved_offer: "9".into(),
        offered: "9".into(),
        proposed: "9".into(),
        personal: "20".into(),
        shortfall: "0".into(),
        frozen: false,
        reason: "Unaccepted".into(),
    }
}

fn pending(world: &mut World, owner: Entity) -> Form {
    ui::capture(world, owner);
    let mut form = editing(world, owner);
    form.commit_fields().unwrap();
    world.get_mut::<TransferCastle>(owner).unwrap().form = Some(form.clone());
    world.get_mut::<View>(owner).unwrap().pending = Some(super::super::Pending {
        id: "stock-response".into(),
        form: form.clone(),
    });
    form
}

fn deliver(app: &mut App, stock: Value) {
    app.world_mut()
        .write_message(CellMessage(ServerMessage::ActionOk {
            id: "stock-response".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(json!({"stock":stock})),
        }));
    app.update();
}

fn inspect(app: &mut App, owner: Entity, saved: Option<&Saved>) {
    let form = pending(app.world_mut(), owner);
    let definition: Definition =
        serde_json::from_value(form.data["request"]["definition"].clone()).unwrap();
    deliver(
        app,
        json!({"definition":definition,"preview":preview(),"saved":saved}),
    );
}

fn reviewed(form: &Form) -> Review {
    let definition: Definition =
        serde_json::from_value(form.data["request"]["definition"].clone()).unwrap();
    Review {
        source: nucleus::transfer::karma::Snapshot {
            transfer: form.transfer.clone().unwrap(),
            revision: form.revision.unwrap(),
            active: false,
            published: false,
            ready: false,
            participants: Default::default(),
        },
        definition,
        unit_uid: None,
        expected_revision: 0,
        preview: preview(),
        basis: "current-stock-basis".into(),
    }
}

#[cfg_attr(test, test)]
fn stock_controls_review_both_modes_and_require_a_separate_explicit_enable() {
    for mode in ["recompute_offer", "preserve_offer"] {
        let (mut app, owner) = setup();
        let first = editing(app.world(), owner);
        assert_eq!(first.revision, Some(1));
        assert!(matches!(
            serde_json::from_value::<engine::actions::Action>(first.payload().unwrap()).unwrap(),
            engine::actions::Action::Surplus {
                request: Request::Inspect { .. }
            }
        ));
        inspect(&mut app, owner, None);
        let mut form = editing(app.world(), owner);
        for field in &mut form.fields {
            if field.path.ends_with("/retain") {
                field.value = "20".into();
            }
            if field.path.ends_with("/mode") {
                field.value = mode.into();
            }
        }
        form.commit_fields().unwrap();
        app.world_mut()
            .get_mut::<TransferCastle>(owner)
            .unwrap()
            .form = Some(form);
        forms::render(app.world_mut(), owner);
        let form = pending(app.world_mut(), owner);
        let review = reviewed(&form);
        let commitment = review.commitment().unwrap();
        deliver(&mut app, json!({"review":review,"commitment":commitment}));
        let enabled = editing(app.world(), owner);
        assert_eq!(enabled.data["request"]["kind"], "enable");
        assert_eq!(enabled.review["Planned for myself"], "20");
        assert!(enabled.fields.is_empty());
        assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
        let engine::actions::Action::Surplus {
            request:
                Request::Enable {
                    review: bound,
                    commitment: hash,
                },
        } = serde_json::from_value(enabled.payload().unwrap()).unwrap()
        else {
            panic!("Explicit Enable")
        };
        assert_eq!(*bound, review);
        assert_eq!(hash, commitment);
        assert_eq!(
            bound.definition.recipe,
            Recipe::Offer {
                transfer_uid: enabled.transfer.clone().unwrap(),
                promise_uid: "apples".into(),
                retain: "20".into(),
                mode: if mode == "preserve_offer" {
                    Mode::PreserveOffer
                } else {
                    Mode::RecomputeOffer
                }
            }
        );
        activate(app.world_mut(), owner, "Cancel");
        assert!(
            app.world()
                .get::<TransferCastle>(owner)
                .unwrap()
                .form
                .is_none()
        );
        assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    }
}

#[cfg_attr(test, test)]
fn stock_review_replies_cannot_approve_changed_fields_person_transfer_or_stock() {
    for change in ["fields", "person", "transfer", "stock"] {
        let (mut app, owner) = setup();
        inspect(&mut app, owner, None);
        let original = pending(app.world_mut(), owner);
        let mut review = reviewed(&original);
        match change {
            "fields" => {
                let field = app
                    .world_mut()
                    .query::<(Entity, &bevy::text::EditableText)>()
                    .iter(app.world())
                    .find(|(_, text)| text.value().to_string() == original.fields[0].value)
                    .map(|(entity, _)| entity)
                    .unwrap();
                app.world_mut()
                    .entity_mut(field)
                    .insert(bevy::text::EditableText::new("500"));
            }
            "person" => ui::Command::Person(nucleus::new_uid("r")).apply(app.world_mut(), owner),
            "transfer" => {
                app.world_mut()
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .selected = nucleus::new_uid("r")
            }
            "stock" => review.definition.stock_uid = nucleus::new_uid("r"),
            _ => unreachable!(),
        }
        let commitment = review.commitment().unwrap();
        deliver(&mut app, json!({"review":review,"commitment":commitment}));
        assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
        assert!(
            app.world()
                .get::<TransferCastle>(owner)
                .unwrap()
                .form
                .as_ref()
                .is_none_or(|form| form.data["request"]["kind"] != "enable")
        );
    }
}

#[cfg_attr(test, test)]
fn saved_stock_controls_offer_reviewed_pause_and_replenishment_without_auto_enabling() {
    let (mut app, owner) = setup();
    let original = editing(app.world(), owner);
    let review = reviewed(&original);
    let saved = Saved {
        commitment: review.commitment().unwrap(),
        managed_revision: review.source.revision,
        review,
        revision: 4,
        rule_uid: Some(nucleus::new_uid("r")),
        enabled: true,
        reason: "Reviewed".into(),
        pending_need: None,
    };
    inspect(&mut app, owner, Some(&saved));
    activate(app.world_mut(), owner, "Pause stock policy");
    let pause = editing(app.world(), owner);
    let engine::actions::Action::Surplus {
        request:
            Request::Pause {
                policy_uid,
                controller_uid,
                expected_revision,
            },
    } = serde_json::from_value(pause.payload().unwrap()).unwrap()
    else {
        panic!("Pause")
    };
    assert_eq!(policy_uid, saved.review.definition.uid);
    assert_eq!(controller_uid, saved.review.definition.controller_uid);
    assert_eq!(expected_revision, 4);
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
    activate(app.world_mut(), owner, "Cancel");
    activate(app.world_mut(), owner, "Plan stock replenishment");
    let mut form = editing(app.world(), owner);
    form.fields[0].value = saved.review.definition.stock_uid.clone();
    form.commit_fields().unwrap();
    app.world_mut()
        .get_mut::<TransferCastle>(owner)
        .unwrap()
        .form = Some(form);
    forms::render(app.world_mut(), owner);
    inspect(&mut app, owner, None);
    let replenishment = editing(app.world(), owner);
    assert_eq!(replenishment.data["request"]["kind"], "review");
    assert_eq!(
        replenishment.data["request"]["definition"]["recipe"]["below"],
        "7"
    );
    assert_eq!(
        replenishment.data["request"]["definition"]["recipe"]["target"],
        "14"
    );
    assert!(app.world().get::<View>(owner).unwrap().pending.is_none());
}

crate::native::laboratory_cases! {
    stock_controls_review_both_modes_and_require_a_separate_explicit_enable,
    stock_review_replies_cannot_approve_changed_fields_person_transfer_or_stock,
    saved_stock_controls_offer_reviewed_pause_and_replenishment_without_auto_enabling,
}
