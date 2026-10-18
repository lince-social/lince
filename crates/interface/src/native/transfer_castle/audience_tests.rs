use super::*;

fn activate(world: &mut World, parent: Entity, caption: &str) {
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
            let mut current = Some(button);
            while let Some(entity) = current {
                if entity == parent {
                    return Some(button);
                }
                current = world.get::<ChildOf>(entity).map(ChildOf::parent);
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
    app.add_plugins(crate::visibility::VisibilityUiPlugin);
    let root = app
        .world_mut()
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    let person = nucleus::new_uid("r");
    let source = nucleus::new_uid("r");
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::ZERO,
        TransferCastle {
            person: person.clone(),
            selected: source.clone(),
            ..Default::default()
        },
    );
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    view.ready = true;
    view.context = json!({"viewer":{"local":true},"capabilities":{"create":true}});
    view.rows = vec![
        json!({"uid":source,"revision":1,"head":"Request","capabilities":{"edit_terms":true},"parties":[],"promises":[],"occurrences":[]}),
    ];
    ui::render(app.world_mut(), owner);
    activate(app.world_mut(), owner, "Audience expansion");
    let mut panel = app.world_mut().get_mut::<Panel>(owner).unwrap();
    panel.pending.clear();
    panel.context = Some(Context {
        record_uid: source,
        data: Data::Record,
        saved: None,
        organs: vec![],
        persons: vec![],
        records: vec![],
    });
    ui::render(app.world_mut(), owner);
    (app, owner)
}

fn review_message(world: &mut World, owner: Entity) -> ServerMessage {
    capture(world, owner);
    let definition = world.get::<Panel>(owner).unwrap().definition.clone();
    world.get_mut::<Panel>(owner).unwrap().pending.insert(
        "audience-review".into(),
        Pending::Review(definition.clone()),
    );
    let review = Review {
        source: nucleus::transfer::karma::Snapshot {
            transfer: definition.source_uid.clone(),
            revision: definition.source_revision,
            active: false,
            published: false,
            ready: false,
            participants: Default::default(),
        },
        definition,
        expected_revision: 0,
        replies: "local-replies".into(),
    };
    ServerMessage::ActionOk {
        id: "audience-review".into(),
        created: None,
        facts: 0,
        warnings: vec![],
        data: Some(json!({"commitment":review.commitment().unwrap(),"review":review})),
    }
}

#[cfg_attr(test, test)]
fn visibility_policy_drafts_edit_conditions_without_saving_any_record_policy() {
    let (mut app, owner) = setup();
    let editor = app.world().get::<Panel>(owner).unwrap().stages[0].0;
    activate(app.world_mut(), editor, "Add inclusion");
    assert_eq!(
        crate::visibility::edited_policy(app.world_mut(), editor)
            .unwrap()
            .include
            .len(),
        1
    );
    let exclusions = app.world().get::<Panel>(owner).unwrap().exclusions.unwrap();
    activate(app.world_mut(), exclusions, "Add exclusion");
    assert_eq!(
        crate::visibility::edited_policy(app.world_mut(), exclusions)
            .unwrap()
            .exclude
            .len(),
        1
    );
    assert!(
        app.world()
            .resource::<Messages<crate::visibility::Request>>()
            .is_empty()
    );
    assert!(app.world().get::<Panel>(owner).unwrap().pending.is_empty());
    activate(app.world_mut(), owner, "Add audience stage");
    assert_eq!(
        app.world()
            .get::<Panel>(owner)
            .unwrap()
            .definition
            .stages
            .len(),
        2
    );
    assert!(app.world().get::<Panel>(owner).unwrap().review.is_none());
    activate(app.world_mut(), owner, "Remove stage");
    assert_eq!(
        app.world()
            .get::<Panel>(owner)
            .unwrap()
            .definition
            .stages
            .len(),
        1
    );
    let message = review_message(app.world_mut(), owner);
    let ServerMessage::ActionOk {
        data: Some(data), ..
    } = message
    else {
        unreachable!()
    };
    let mut review: Review = serde_json::from_value(data["review"].clone()).unwrap();
    review.definition.projection.title = "Earlier saved request".into();
    review.definition.stages.push(Stage {
        after_ms: 30_000,
        include: vec![],
    });
    let saved = Saved {
        commitment: review.commitment().unwrap(),
        review: review.clone(),
        revision: 3,
        stage: Some(0),
        next_at_ms: 0,
        status: plan::Status::Paused,
        reason: "Reply".into(),
        rule_uid: None,
        policy_revision: 1,
    };
    {
        let mut panel = app.world_mut().get_mut::<Panel>(owner).unwrap();
        panel.pending.clear();
        panel.plans = vec![saved];
    }
    ui::render(app.world_mut(), owner);
    activate(app.world_mut(), owner, "Review / resume this plan");
    capture(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<Panel>(owner).unwrap().definition,
        review.definition
    );
}

#[cfg_attr(test, test)]
fn audience_review_reply_cannot_approve_changed_fields_or_another_person() {
    for changed in ["field", "person", "source"] {
        let (mut app, owner) = setup();
        let message = review_message(app.world_mut(), owner);
        match changed {
            "field" => {
                let entity = app
                    .world()
                    .get::<Panel>(owner)
                    .unwrap()
                    .fields
                    .iter()
                    .find(|(key, _)| key == "title")
                    .unwrap()
                    .1;
                app.world_mut()
                    .entity_mut(entity)
                    .insert(bevy::text::EditableText::new("Changed title"));
            }
            "person" => {
                app.world_mut()
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .person = nucleus::new_uid("r")
            }
            "source" => {
                app.world_mut()
                    .get_mut::<TransferCastle>(owner)
                    .unwrap()
                    .selected = nucleus::new_uid("r")
            }
            _ => unreachable!(),
        }
        assert!(receive(app.world_mut(), &message));
        assert!(
            app.world()
                .get::<Panel>(owner)
                .is_none_or(|panel| panel.review.is_none())
        );
    }
}

#[cfg_attr(test, test)]
fn enabling_audience_stages_requires_the_same_current_review_and_never_auto_submits() {
    let (mut app, owner) = setup();
    let message = review_message(app.world_mut(), owner);
    assert!(receive(app.world_mut(), &message));
    assert!(app.world().get::<Panel>(owner).unwrap().review.is_some());
    assert!(app.world().get::<Panel>(owner).unwrap().pending.is_empty());
    let entity = app
        .world()
        .get::<Panel>(owner)
        .unwrap()
        .fields
        .iter()
        .find(|(key, _)| key == "quantity")
        .unwrap()
        .1;
    app.world_mut()
        .entity_mut(entity)
        .insert(bevy::text::EditableText::new("100"));
    activate(app.world_mut(), owner, "Enable reviewed stages");
    let panel = app.world().get::<Panel>(owner).unwrap();
    assert!(panel.review.is_none());
    assert!(panel.pending.is_empty());
    assert_eq!(panel.definition.projection.quantity.as_deref(), Some("100"));
    assert!(panel.notice.contains("Review"));
}

crate::native::laboratory_cases! {
    visibility_policy_drafts_edit_conditions_without_saving_any_record_policy,
    audience_review_reply_cannot_approve_changed_fields_or_another_person,
    enabling_audience_stages_requires_the_same_current_review_and_never_auto_submits,
}
