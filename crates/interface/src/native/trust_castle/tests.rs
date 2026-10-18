use super::*;

fn setup() -> (App, Entity) {
    let mut app = crate::native::sand_panel::tests::app();
    let root = app.world_mut().spawn_empty().id();
    let sand = app.world_mut().spawn(Node::default()).id();
    let owner = populate(app.world_mut(), root, sand);
    (app, owner)
}

fn activate_caption(world: &mut World, parent: Entity, caption: &str) {
    let labels = world.query::<(Entity, &Text)>().iter(world)
        .filter(|(_, text)| text.0 == caption)
        .map(|(entity, _)| entity).collect::<Vec<_>>();
    let buttons = labels.into_iter().filter_map(|label| {
        let button = world.get::<ChildOf>(label)?.parent();
        let mut cursor = Some(button);
        while let Some(entity) = cursor {
            if entity == parent {
                let action = world.get::<crate::native::actions::ActionButton>(button)?;
                return Some((action.target, action.actions.clone()));
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        None
    }).collect::<Vec<_>>();
    assert_eq!(buttons.len(), 1);
    buttons[0].1.run(world, buttons[0].0);
}

pub(crate) fn assert_subject_entry(world: &mut World, parent: Entity, subject: Identity) {
    let before = world.query_filtered::<Entity, With<View>>().iter(world).collect::<BTreeSet<_>>();
    activate_caption(world,parent,"Inspect permitted Transfer experience");
    let opened = world.query_filtered::<Entity, With<View>>().iter(world)
        .filter(|entity| !before.contains(entity)).collect::<Vec<_>>();
    assert_eq!(opened.len(), 1);
    let view = world.get::<View>(opened[0]).unwrap();
    assert_eq!(analysis::build(view).unwrap(), nucleus::trust_analysis::query::Query::for_subject(subject));
    assert!(view.analyze_after_context);
    assert!(view.pending.is_none());
    assert!(view.remote.job.is_none());
    assert!(view.remote.review.is_none());
    assert!(view.result.is_null());
}

#[cfg_attr(test, test)]
fn contact_controls_default_private_and_compose_typed_range_rules() {
    let (mut app, owner) = setup();
    let person = nucleus::new_uid("r");
    let contact = nucleus::new_uid("r");
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.contact = contact.clone();
        view.anchor = person.clone();
        view.rules = vec![
            Rule {
                scope: RuleScope::Intermediary,
                exclusion: false,
                identities: format!("organ:{contact}"),
                known: 1,
                lower: "1".into(),
                upper: "3".into(),
                lower_inclusive: false,
                upper_inclusive: true,
            },
            Rule {
                scope: RuleScope::Intermediary,
                exclusion: true,
                identities: format!("organ:{contact}"),
                ..Default::default()
            },
        ];
    }
    let view = app.world().get::<View>(owner).unwrap();
    let draft = draft(view).unwrap();
    assert!(
        matches!(&draft.audience,Audience::Selected {viewers} if viewers==&vec![Identity::Person(person.clone())])
    );
    let policy = path_policy(view).unwrap();
    assert_eq!(policy.max_depth, 3);
    assert!(!policy.allow_unilateral);
    assert_eq!(policy.intermediaries.include.len(), 1);
    assert_eq!(policy.intermediaries.exclude.len(), 1);
    let context = nucleus::trust_analysis::IdentityContext {
        readable: true,
        known: true,
        proximity: Some(2),
        ..Default::default()
    };
    assert!(
        !policy
            .intermediaries
            .allows(&Identity::Organ(contact), &context)
    );
    let action = engine::actions::Action::TrustAnalysis {
        person: Some(person),
        request_id: "native-contact-preview".into(),
        request: Backend::PreviewContact { draft },
    };
    let encoded = serde_json::to_value(action).unwrap();
    serde_json::from_value::<engine::actions::Action>(encoded).unwrap();
    render(app.world_mut(), owner);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Preview exact disclosure"));
    assert!(!labels.contains(&"Sign and retain reviewed disclosure"));
}

#[cfg_attr(test, test)]
fn viewer_changes_clear_private_drafts_and_late_trust_replies() {
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = "previous-person".into();
        view.contact = "private-contact".into();
        view.readers = "private-audience".into();
        view.import = "private-proof".into();
        view.result = json!({"kind":"contact_document","export":"private-export"});
        view.pending = Some(("old-result".into(), 0));
    }
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::SessionAuthenticated {
            id: "new-login".into(),
            session_id: "new-session".into(),
            person: nucleus::new_uid("r"),
            key_id: "new-key".into(),
        }));
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "old-result".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"trust":{"kind":"contact_document","export":"private-export"}})),
        }));
    app.add_systems(Update, receive);
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.contact.is_empty());
    assert!(view.readers.is_empty());
    assert!(view.import.is_empty());
    assert!(view.result.is_null());
    assert!(view.review.is_none());
    assert!(view.pending.is_none());
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!labels.contains("private-proof"));
    assert!(!labels.contains("private-export"));
    assert!(!labels.contains("private-contact"));
}

#[cfg_attr(test, test)]
fn edited_drafts_cannot_restore_a_late_confirmation() {
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let contact = nucleus::new_uid("r");
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.contact = contact.clone();
        view.pending = Some(("old-preview".into(), 0));
    }
    render(app.world_mut(), owner);
    let field = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| input.owner == owner && input.key == Key::Term)
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("31");
    let document = nucleus::trust_analysis::contacts::Document {
        statement: nucleus::trust_analysis::contacts::Statement {
            issuer: Identity::Person(person.clone()),
            contact: Identity::Person(contact),
            revision: 1,
            issued_at: 1000,
            expires_at: 2000,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: "review-key".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: String::new(),
    };
    let review = ContactReview {
        person,
        document,
        expected_revision: 0,
        reviewed_at: 1000,
        profiles: Vec::new(),
    };
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk {id:"old-preview".into(),created:None,facts:0,warnings:Vec::new(),data:Some(json!({"trust":{"kind":"contact_review","review":review,"hash":review.hash().unwrap()}}))}));
    app.add_systems(Update, receive);
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert_eq!(view.term, "31");
    assert!(view.review.is_none());
    assert!(view.pending.is_none());
}

#[cfg_attr(test, test)]
fn reader_query_roundtrip_keeps_independent_filters_weights_budgets_and_exclusions() {
    use nucleus::trust_analysis::{Condition, Filter, query::Query};
    let (mut app, owner) = setup();
    let subject = Identity::Person(nucleus::new_uid("r"));
    let anchor = Identity::Person(nucleus::new_uid("r"));
    let mut query = Query::for_subject(subject);
    let condition = Condition {
        identities: vec![Identity::Organ(nucleus::new_uid("r"))],
        known: Some(true),
        lower: Some(nucleus::visibility::Bound {
            value: 1,
            inclusive: false,
        }),
        upper: Some(nucleus::visibility::Bound {
            value: 4,
            inclusive: true,
        }),
    };
    query.issuers = Filter {
        include: vec![condition.clone()],
        exclude: Vec::new(),
    };
    query.counterparties = Filter {
        include: Vec::new(),
        exclude: vec![condition.clone()],
    };
    query.endpoints = Filter {
        include: vec![condition.clone()],
        exclude: Vec::new(),
    };
    query.exclude_everywhere = vec![Identity::Organ(nucleus::new_uid("r"))];
    let mut policy = paths::Policy::for_anchor(anchor);
    policy.max_identities = 37;
    policy.max_depth = 5;
    policy.allow_unilateral = true;
    policy.exclude_everywhere = vec![Identity::Person(nucleus::new_uid("r"))];
    policy.targets.include.push(condition.clone());
    policy.intermediaries.exclude.push(condition);
    query.contacts = Some(policy);
    query.issuer_cap = Some(nucleus::DecimalValue::parse_inferred("12.50").unwrap());
    query.first_contact_cap = Some(nucleus::DecimalValue::parse_inferred("9.25").unwrap());
    query.from = Some(1750000000);
    query.until = Some(1760000000);
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    analysis::load(&mut view, query.clone());
    assert_eq!(analysis::build(&view).unwrap(), query);
    clear_scope(&mut view);
    assert!(view.analysis.subject.is_empty());
    assert!(view.analysis.path_exclusions.is_empty());
    assert_eq!(view.analysis.levels, [true, false, false]);
    assert!(view.rules.is_empty());
    view.result = json!({"kind":"query_analysis","checked_at":1750000000,"analysis":{
        "policy":"local-policy","partial":false,
        "groups":[{"level":"need_linked_receiver","raw_quantity":{"scale":0,"value":"12"},"weighted_index":{"scale":0,"value":"3"},"occurrences":1}],
        "rows":[{"level":"need_linked_receiver","quantity":{"scale":0,"value":"12"},"selected":true}],
        "protocol":[
            {"current":true,"content":{"kind":"need","requested":{"scale":0,"value":"10"}}},
            {"current":true,"content":{"kind":"settled","quantity":nucleus::transfer::receipt::RecordedFloat::new(10.0).unwrap()}}
        ],"withheld":[]
    }});
    drop(view);
    render(app.world_mut(), owner);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(labels.contains("raw 12 · weighted index 3"));
    assert!(labels.contains("Receiver-authored Need: 10"));
    assert!(labels.contains("Canonical settled quantity: 10"));
    app.init_resource::<Messages<CellMessage>>();
    let generation = app.world().get::<View>(owner).unwrap().generation;
    app.world_mut().get_mut::<View>(owner).unwrap().pending =
        Some(("local-refresh".into(), generation));
    Command::QueryPage(1).apply(app.world_mut(), owner);
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "local-refresh".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"trust":{"kind":"query_analysis","marker":"fresh-local-evidence"}})),
        }));
    app.add_systems(Update, receive);
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert_eq!(view.result["marker"], "fresh-local-evidence");
    assert!(view.pending.is_none());
}

#[cfg_attr(test, test)]
fn claim_controls_separate_publisher_amount_from_visible_support_and_clear_private_reviews() {
    use nucleus::trust_analysis::claims::{Comparator, commands::Review};
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let measurement = nucleus::transfer::receipt::Measurement {
        meaning: nucleus::vocabulary::reference::MeaningReference {
            vocabulary_uid: nucleus::new_uid("g"),
            concept_uid: nucleus::new_uid("c"),
            revision: 1,
            definition_hash: "a".repeat(64),
        },
        unit: None,
        object_commitment: None,
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = person.clone();
        view.analysis.measurements.push(measurement);
        view.claims.amount = "100".into();
    }
    let view = app.world().get::<View>(owner).unwrap();
    let draft = claims::draft(view).unwrap();
    assert_eq!(draft.comparator, Comparator::AtLeast);
    assert!(
        matches!(&draft.audience, Audience::Selected { viewers } if viewers == &vec![Identity::Person(person.clone())])
    );
    assert_eq!(draft.descriptor.measurement, view.analysis.measurements[0]);
    let document = nucleus::trust_analysis::claims::Document {
        statement: nucleus::trust_analysis::claims::Statement {
            title: draft.title,
            issuer: draft.issuer,
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: draft.descriptor,
            comparator: draft.comparator,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: 1000,
            issued_at: 1000,
            expires_at: 2000,
            withdrawn: false,
            audience: draft.audience,
            authorship: nucleus::trust_analysis::claims::Authorship::Manual,
        },
        key_id: "claim-review-key".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: String::new(),
    };
    let review = Review {
        person,
        document,
        expected_revision: 0,
        reviewed_at: 1000,
        profiles: Vec::new(),
    };
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(("old-claim-preview".into(), 0));
    render(app.world_mut(), owner);
    let field = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| input.owner == owner && input.key == Key::Claim(claims::Field::Amount))
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("200");
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk {
        id: "old-claim-preview".into(), created: None, facts: 0, warnings: Vec::new(),
        data: Some(json!({"trust":{"kind":"claim_review","review":review,"hash":review.hash().unwrap()}})),
    }));
    app.add_systems(Update, receive);
    app.update();
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    assert_eq!(view.claims.amount, "200");
    assert!(view.claims.review.is_none());
    let result = json!({"kind":"claim_check","support":{"claimed":{"value":"100"},"observed":{"value":"10"},"state":"partial_support","partial":true}});
    view.analysis.page = 3;
    view.pending = Some(("fresh-claim-check".into(), view.generation));
    drop(view);
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "fresh-claim-check".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"trust":result})),
        }));
    app.update();
    assert_eq!(app.world().get::<View>(owner).unwrap().analysis.page, 0);
    render(app.world_mut(), owner);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(labels.contains("Publisher claims"));
    assert!(labels.contains("this reader sees support"));
    assert!(labels.contains("Partial support in visible evidence"));
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    view.claims.import = "private-claim-document".into();
    view.claims.review = Some((review.clone(), review.hash().unwrap()));
    clear_scope(&mut view);
    assert!(view.claims.import.is_empty());
    assert!(view.claims.review.is_none());
    assert!(view.result.is_null());
}

#[cfg_attr(test, test)]
fn activity_selection_without_history_ignores_a_response_after_the_draft_changes() {
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let measurement = nucleus::transfer::receipt::Measurement {
        meaning: nucleus::vocabulary::reference::MeaningReference {
            vocabulary_uid: nucleus::new_uid("g"),
            concept_uid: nucleus::new_uid("c"),
            revision: 3,
            definition_hash: "b".repeat(64),
        },
        unit: None,
        object_commitment: None,
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = nucleus::new_uid("r");
        view.analysis.subject = view.person.clone();
        view.analysis.concept = measurement.meaning.concept_uid.clone();
        view.pending = Some(("activity-selection".into(), 0));
    }
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "activity-selection".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"measurement":measurement})),
        }));
    app.add_systems(Update, receive);
    app.update();
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    assert_eq!(view.analysis.measurements, vec![measurement.clone()]);
    view.claims.amount = "100".into();
    assert_eq!(
        claims::draft(&view).unwrap().descriptor.measurement,
        measurement
    );
    view.analysis.measurements.clear();
    view.pending = Some(("stale-selection".into(), view.generation));
    view.generation = view.generation.wrapping_add(1);
    drop(view);
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "stale-selection".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"measurement":measurement})),
        }));
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.analysis.measurements.is_empty());
    assert!(view.pending.is_none());
}

#[cfg_attr(test, test)]
fn recipe_controls_default_paused_bind_public_scope_and_reject_late_edited_reviews() {
    use nucleus::trust_analysis::claims::recipes::{self, commands::Review};
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let measurement = nucleus::transfer::receipt::Measurement {
        meaning: nucleus::vocabulary::reference::MeaningReference {
            vocabulary_uid: nucleus::new_uid("g"),
            concept_uid: nucleus::new_uid("c"),
            revision: 1,
            definition_hash: "a".repeat(64),
        },
        unit: None,
        object_commitment: None,
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = person.clone();
        view.analysis.measurements.push(measurement);
    }
    let view = app.world().get::<View>(owner).unwrap();
    let draft = recipe_controls::draft(view).unwrap();
    assert!(!draft.start_enabled);
    assert_eq!(draft.refresh_seconds, 60);
    assert!(
        matches!(&draft.claim.audience,Audience::Selected {viewers} if viewers==&vec![Identity::Person(person.clone())])
    );
    let terms = recipes::Terms {
        issuer: draft.claim.issuer,
        claim: nucleus::new_uid("r"),
        title: draft.claim.title,
        descriptor: draft.claim.descriptor,
        comparator: draft.claim.comparator,
        audience: draft.claim.audience,
        issued_at: 1000,
        expires_at: 1000 + i64::from(draft.permit_seconds),
        lifetime_seconds: draft.claim.lifetime_seconds,
        delegate: recipes::Delegate {
            organ: nucleus::new_uid("r"),
            cell: nucleus::new_uid("r"),
            key_id: "test-publishing-cell".into(),
            public_key: format!("{}=", "A".repeat(43)),
        },
    };
    let definition = recipes::Definition {
        starting_claim_revision: 0,
        recipe: nucleus::new_uid("r"),
        revision: 1,
        controller: person,
        terms: terms.clone(),
        inputs: draft.inputs,
        start_enabled: false,
        refresh_seconds: 60,
    };
    let recipe = recipes::Recipe {
        permit: recipes::Permit {
            recipe: definition.id().unwrap(),
            revision: 1,
            terms,
            key_id: "test-owner".into(),
            public_key: format!("{}=", "A".repeat(43)),
            signature: String::new(),
        },
        definition,
    };
    let review = Review {
        recipe,
        expected_revision: 0,
        reviewed_at: 1000,
        profiles: Vec::new(),
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.recipes.review = Some((review.clone(), review.hash().unwrap()));
    }
    render(app.world_mut(), owner);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(labels.contains("Sign and save this exact publication permit"));
    assert!(labels.contains("Private controller and input policy remain outside"));
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(("old-recipe-review".into(), 0));
    let field = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| {
            input.owner == owner && input.key == Key::Recipe(recipe_controls::Field::Refresh)
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("90");
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk {id:"old-recipe-review".into(),created:None,facts:0,warnings:Vec::new(),data:Some(json!({"trust":{"kind":"recipe_review","review":review,"hash":review.hash().unwrap()}}))}));
    app.add_systems(Update, receive);
    app.update();
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    assert_eq!(view.recipes.refresh, "90");
    assert!(view.recipes.review.is_none());
    view.recipes.recipe = "private-recipe".into();
    view.recipes.review = Some((review.clone(), review.hash().unwrap()));
    clear_scope(&mut view);
    assert!(view.recipes.recipe.is_empty());
    assert!(view.recipes.review.is_none());
}

#[cfg_attr(test, test)]
fn profile_snapshot_checks_cache_only_the_exact_reader_document_and_source_policy() {
    use nucleus::trust_analysis::claims::{Authorship, Document, Statement};
    let (mut app, owner) = setup();
    let person = nucleus::new_uid("r");
    let signer = engine::trust::Signer::generate(&person, "snapshot-native");
    let now = nucleus::execution::now().timestamp();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = person.clone();
        view.analysis
            .measurements
            .push(nucleus::transfer::receipt::Measurement {
                meaning: nucleus::vocabulary::reference::MeaningReference {
                    vocabulary_uid: nucleus::new_uid("g"),
                    concept_uid: nucleus::new_uid("c"),
                    revision: 1,
                    definition_hash: "b".repeat(64),
                },
                unit: None,
                object_commitment: None,
            });
    }
    let draft = claims::draft(app.world().get::<View>(owner).unwrap()).unwrap();
    let mut document = Document {
        statement: Statement {
            title: draft.title,
            issuer: draft.issuer,
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: draft.descriptor,
            comparator: draft.comparator,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: now,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
        signature: String::new(),
    };
    document.statement.descriptor.subject = Identity::Organ(nucleus::new_uid("r"));
    engine::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.anchor = document.statement.descriptor.subject.uid().into();
        view.anchor_organ = true;
        profile_claims::load_snapshot(&mut view, &document);
        assert_eq!(view.anchor, person);
        assert!(!view.anchor_organ);
        assert_eq!(
            view.analysis.subject,
            document.statement.descriptor.subject.uid()
        );
    }
    let query = document
        .statement
        .descriptor
        .checking_query(
            &analysis::build(app.world().get::<View>(owner).unwrap()).unwrap(),
            now,
        )
        .unwrap();
    let result = json!({"kind":"claim_check","document":document,"query":query,"checked_at":now,"support":{"state":"unknown","claimed":{"value":"100"},"partial":true}});
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.claims.snapshot = Some(document.clone());
        profile_claims::cache(&mut view, &result);
        assert_eq!(view.claims.checks.len(), 1);
        profile_claims::cache(&mut view, &result);
        assert_eq!(view.claims.checks.len(), 1);
    }
    render(app.world_mut(), owner);
    let labels = |app: &mut App| {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(labels(&mut app).contains("Cached independent local check"));
    assert!(labels(&mut app).contains("Check snapshot locally using my source policy"));
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.analysis.all_visible = !view.analysis.all_visible;
    }
    render(app.world_mut(), owner);
    assert!(!labels(&mut app).contains("Cached independent local check"));
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.analysis.all_visible = !view.analysis.all_visible;
        view.as_organ = true;
    }
    render(app.world_mut(), owner);
    assert!(!labels(&mut app).contains("Cached independent local check"));
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    let mut large = result.clone();
    large["padding"] = json!("");
    profile_claims::cache(&mut view, &large);
    let bytes = serde_json::to_vec(&view.claims.checks).unwrap().len();
    large["padding"] = json!("x".repeat(nucleus::trust_analysis::MAX_BYTES - bytes + 1));
    profile_claims::cache(&mut view, &large);
    assert_eq!(view.claims.checks.len(), 1);
    assert!(
        serde_json::to_vec(&view.claims.checks).unwrap().len()
            <= nucleus::trust_analysis::MAX_BYTES
    );
    clear_scope(&mut view);
    assert!(view.claims.snapshot.is_none());
    assert!(view.claims.checks.is_empty());
}

#[cfg_attr(test, test)]
fn organ_root_controls_require_exact_independent_review_clear_on_viewer_change_and_ignore_old_replies()
 {
    use nucleus::identity::roots::{KeyStatus, Proof, Review, State};
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let organ = nucleus::new_uid("r");
    let old = engine::trust::Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    let new = engine::trust::Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    let now = nucleus::execution::now().timestamp();
    let mut proof = Proof {
        organ: organ.clone(),
        public_key: new.public_key_b64(),
        nonce: nucleus::new_uid("r"),
        issued_at: now,
        signature: String::new(),
    };
    engine::identity::sign_organ_root_proof(&mut proof, &new).unwrap();
    let review = Review {
        person: person.clone(),
        proof: proof.clone(),
        profile_revision: 0,
        previous: State {
            original: Some(KeyStatus {
                public_key: old.public_key_b64(),
                revoked: true,
            }),
            current: Some(KeyStatus {
                public_key: old.public_key_b64(),
                revoked: true,
            }),
            revision: 0,
            available: true,
        },
        reviewed_at: now,
    };
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.context = json!({"organ":organ,"organ_admin":true});
        view.roots.subject = organ.clone();
        view.roots.document = serde_json::to_string(&proof).unwrap();
        view.roots.focused = true;
    }
    assert!(
        root_controls::request(
            app.world().get::<View>(owner).unwrap(),
            &root_controls::UiCommand::Accept
        )
        .is_err()
    );
    render(app.world_mut(), owner);
    let labels = |app: &mut App| {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(!labels(&mut app).contains("Find permitted observed paths"));
    assert!(labels(&mut app).contains("Open full trust analysis"));
    assert!(!labels(&mut app).contains("Accept independently compared Organ owner key"));
    let result = json!({"kind":"identity","value":{"kind":"organ_root_preview","review":review,"review_hash":review.hash().unwrap(),"new_fingerprint":proof.fingerprint().unwrap()}});
    app.world_mut().get_mut::<View>(owner).unwrap().result = result.clone();
    render(app.world_mut(), owner);
    assert!(labels(&mut app).contains("Accept independently compared Organ owner key"));
    let request = root_controls::request(
        app.world().get::<View>(owner).unwrap(),
        &root_controls::UiCommand::Accept,
    )
    .unwrap();
    assert!(request.is_mutation());
    assert_eq!(request.permission(), "organ:update");
    let action = engine::actions::Action::Identity { request };
    serde_json::from_value::<engine::actions::Action>(serde_json::to_value(action).unwrap())
        .unwrap();
    app.world_mut().get_mut::<View>(owner).unwrap().context["organ_admin"] = json!(false);
    render(app.world_mut(), owner);
    assert!(!labels(&mut app).contains("Accept independently compared Organ owner key"));
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.context["organ_admin"] = json!(true);
        view.pending = Some(("old-root-preview".into(), view.generation));
    }
    render(app.world_mut(), owner);
    let field = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| {
            input.owner == owner && input.key == Key::OrganRoot(root_controls::Field::Document)
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("changed key-control proof");
    Command::OrganRoot(root_controls::UiCommand::Accept).apply(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<View>(owner).unwrap().roots.document,
        "changed key-control proof"
    );
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "old-root-preview".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"identity":result["value"]})),
        }));
    app.add_systems(Update, receive);
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.result.is_null());
    assert!(root_controls::request(view, &root_controls::UiCommand::Accept).is_err());
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    clear_scope(&mut view);
    assert!(view.roots.document.is_empty());
    assert!(view.roots.subject.is_empty());
}

#[cfg_attr(test, test)]
fn serving_controls_default_private_review_exact_conditions_and_clear_late_viewer_results() {
    use nucleus::trust_analysis::{
        contacts::{Document, Statement},
        serving::{self as model, Artifact, SourceKind},
    };
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let source = nucleus::new_uid("r");
    let peer = nucleus::new_uid("r");
    let now = nucleus::execution::now().timestamp();
    let signer = engine::trust::Signer::generate(&person, "person:v1");
    let mut document = Document {
        statement: Statement {
            issuer: Identity::Person(person.clone()),
            contact: Identity::Organ(peer.clone()),
            revision: 1,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
        signature: String::new(),
    };
    engine::trust_analysis::sign_contact_document(&mut document, &signer).unwrap();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.serving.source = source;
        view.serving.kind = SourceKind::Contact;
        view.serving.focused = true;
    }
    let request = serving_controls::selection(app.world().get::<View>(owner).unwrap()).unwrap();
    assert!(request.audience.include.is_empty());
    assert!(!request.as_organ);
    assert!(
        serving_controls::request(
            app.world().get::<View>(owner).unwrap(),
            &serving_controls::UiCommand::Publish
        )
        .is_err()
    );
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.rules = vec![
            Rule {
                scope: RuleScope::Serving,
                identities: format!("organ:{peer}"),
                upper: "3".into(),
                upper_inclusive: false,
                ..Default::default()
            },
            Rule {
                scope: RuleScope::Serving,
                exclusion: true,
                identities: format!("organ:{peer}"),
                ..Default::default()
            },
        ];
    }
    let request = serving_controls::selection(app.world().get::<View>(owner).unwrap()).unwrap();
    assert_eq!(request.audience.include.len(), 1);
    assert_eq!(request.audience.exclude.len(), 1);
    let review = model::Review {
        person: person.clone(),
        previous: None,
        request,
        artifact: Artifact::Contact(document),
        source_version: 1,
        source_revision: 0,
        reviewed_at: now,
        expires_at: now + 3600,
    };
    let result = json!({"kind":"serving_preview","review":review,"hash":review.hash().unwrap(),"artifact_hash":review.artifact.hash().unwrap()});
    app.world_mut().get_mut::<View>(owner).unwrap().result = result.clone();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.context = json!({"organ_admin":true});
        view.as_organ = true;
    }
    assert!(
        serving_controls::selection(app.world().get::<View>(owner).unwrap())
            .unwrap()
            .as_organ
    );
    assert!(
        serving_controls::request(
            app.world().get::<View>(owner).unwrap(),
            &serving_controls::UiCommand::Publish
        )
        .is_err()
    );
    render(app.world_mut(), owner);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Source reader: Organ grants"))
    );
    Command::Toggle("as_organ").apply(app.world_mut(), owner);
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    app.world_mut().get_mut::<View>(owner).unwrap().result = result.clone();
    render(app.world_mut(), owner);
    let labels = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(labels.contains("Enable exactly reviewed evidence sharing"));
    assert!(!labels.contains("Find permitted observed paths"));
    let request = serving_controls::request(
        app.world().get::<View>(owner).unwrap(),
        &serving_controls::UiCommand::Publish,
    )
    .unwrap();
    let action = engine::actions::Action::TrustAnalysis {
        person: Some(person),
        request_id: "serving-native".into(),
        request: Backend::Serving { request },
    };
    serde_json::from_value::<engine::actions::Action>(serde_json::to_value(action).unwrap())
        .unwrap();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.pending = Some(("serving-old-preview".into(), view.generation));
    }
    let field = app
        .world_mut()
        .query::<(Entity, &Input)>()
        .iter(app.world())
        .find(|(_, input)| {
            input.owner == owner && input.key == Key::Serving(serving_controls::Field::Term)
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("600");
    Command::Serving(serving_controls::UiCommand::Publish).apply(app.world_mut(), owner);
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "serving-old-preview".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"trust":result})),
        }));
    app.add_systems(Update, receive);
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    clear_scope(&mut view);
    assert!(view.serving.source.is_empty());
    assert!(!view.serving.focused);
    assert!(view.rules.is_empty());
}

#[cfg_attr(test, test)]
fn remote_controls_review_exact_questions_keep_local_policy_and_ignore_changed_viewer_replies() {
    use nucleus::trust_analysis::remote::{self as model, Endpoint, Review};
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let subject = nucleus::new_uid("r");
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = subject;
        view.analysis.organ = true;
    }
    let draft = remote_controls::selection(app.world().get::<View>(owner).unwrap()).unwrap();
    assert!(
        !serde_json::to_string(&draft.questions().unwrap())
            .unwrap()
            .contains(&person)
    );
    let review = Review {
        nonce: nucleus::new_uid("ask"),
        person: person.clone(),
        questions: draft.questions().unwrap(),
        draft,
        endpoints: vec![Endpoint {
            organ: nucleus::new_uid("r"),
            endpoint: "a".repeat(64),
            label: "Reviewed recipient".into(),
            profile_revision: 0,
        }],
        profiles: Vec::new(),
        reviewed_at: nucleus::execution::now().timestamp(),
    };
    let result =
        json!({"kind":"remote_review","review":review,"hash":review.hash().unwrap(),"local":{}});
    remote_controls::receive(
        &mut app.world_mut().get_mut::<View>(owner).unwrap(),
        &result,
    );
    let request = remote_controls::request(
        app.world().get::<View>(owner).unwrap(),
        &remote_controls::UiCommand::Start,
    )
    .unwrap();
    assert!(matches!(request, model::Command::Start { .. }));
    render(app.world_mut(), owner);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("Start exactly reviewed questions"))
    );
    Command::Remote(remote_controls::UiCommand::Contacts).apply(app.world_mut(), owner);
    assert!(
        remote_controls::request(
            app.world().get::<View>(owner).unwrap(),
            &remote_controls::UiCommand::Start
        )
        .is_err()
    );
    let generation = app.world().get::<View>(owner).unwrap().generation;
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.pending = Some(("remote-late".into(), generation));
        clear_scope(&mut view);
        view.person = nucleus::new_uid("r");
    }
    app.world_mut()
        .write_message(CellMessage(cell::ServerMessage::ActionOk {
            id: "remote-late".into(),
            created: None,
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"trust":result})),
        }));
    app.add_systems(Update, receive);
    app.update();
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.remote.review.is_none());
    assert!(view.remote.result.is_null());
    assert!(view.remote.document.is_null());
}

#[cfg_attr(test, test)]
fn remote_view_edits_and_closing_discard_jobs_through_the_original_connection() {
    let (mut app, owner) = setup();
    let person = nucleus::new_uid("r");
    let job = nucleus::new_uid("ask");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.remote.job = Some(job.clone());
        view.remote.document =
            json!({"kind":"remote_artifact","artifact":{"secret":"private evidence"}});
        view.pending = Some(("held".into(), view.generation));
    }
    render(app.world_mut(), owner);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("private evidence"))
    );
    remote_controls::track_with_sender(app.world_mut(), owner, sender.clone());
    app.world_mut().get_mut::<View>(owner).unwrap().generation += 1;
    remote_controls::maintain(app.world_mut());
    let cell::ClientMessage::Act {
        action:
            engine::actions::Action::TrustAnalysis {
                person: Some(actual),
                request:
                    Backend::Remote {
                        request:
                            nucleus::trust_analysis::remote::Command::Forget { job: actual_job },
                    },
                ..
            },
        ..
    } = receiver.try_recv().unwrap()
    else {
        panic!("Expected explicit job discard")
    };
    assert_eq!(actual, person);
    assert_eq!(actual_job, job);
    assert!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .remote
            .document
            .is_null()
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("private evidence"))
    );
    let second = nucleus::new_uid("ask");
    app.world_mut().get_mut::<View>(owner).unwrap().remote.job = Some(second.clone());
    remote_controls::track_with_sender(app.world_mut(), owner, sender);
    app.world_mut().despawn(owner);
    remote_controls::maintain(app.world_mut());
    let cell::ClientMessage::Act {
        action:
            engine::actions::Action::TrustAnalysis {
                request:
                    Backend::Remote {
                        request: nucleus::trust_analysis::remote::Command::Forget { job: actual },
                    },
                ..
            },
        ..
    } = receiver.try_recv().unwrap()
    else {
        panic!("Expected closing to discard transient work")
    };
    assert_eq!(actual, second);
}

#[cfg_attr(test, test)]
fn rolling_serving_controls_require_explicit_scope_review_and_clear_changed_or_late_results() {
    use nucleus::trust_analysis::{claims::{self as model, recipes}, serving::{self, Artifact, Scope, SourceKind}};
    let (mut app, owner) = setup();
    let now = nucleus::execution::now().timestamp();
    let person = nucleus::new_uid("r");
    let source = nucleus::new_uid("r");
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = person.clone();
        view.analysis.measurements.push(nucleus::transfer::receipt::Measurement {
            meaning: nucleus::vocabulary::reference::MeaningReference {
                vocabulary_uid: nucleus::new_uid("g"), concept_uid: nucleus::new_uid("c"), revision: 1, definition_hash: "b".repeat(64),
            }, unit: None, object_commitment: None,
        });
        view.serving.source = source;
        view.serving.kind = SourceKind::Claim;
    }
    assert_eq!(serving_controls::selection(app.world().get::<View>(owner).unwrap()).unwrap().scope, Scope::Fixed);
    serving_controls::apply(app.world_mut(), owner, &serving_controls::UiCommand::Scope);
    let request = serving_controls::selection(app.world().get::<View>(owner).unwrap()).unwrap();
    assert_eq!(request.scope, Scope::Recipe);
    assert!(request.audience.include.is_empty());
    let draft = claims::draft(app.world().get::<View>(owner).unwrap()).unwrap();
    let signer = engine::trust::Signer::generate(&person, "native-rolling-owner");
    let organ = nucleus::new_uid("r");
    let publisher = engine::trust::Signer::generate(&organ, "native-rolling-cell");
    let mut permit = recipes::Permit {
        recipe: nucleus::karma::canonical_hash("lince.test.recipe.v1", &"native rolling").unwrap(),
        revision: 1,
        terms: recipes::Terms {
            issuer: draft.issuer.clone(), claim: nucleus::new_uid("r"), title: draft.title.clone(), descriptor: draft.descriptor.clone(),
            comparator: draft.comparator, audience: Audience::Public, issued_at: now, expires_at: now + 86400, lifetime_seconds: 3600,
            delegate: recipes::Delegate { organ, cell: nucleus::new_uid("r"), key_id: publisher.key_id.clone(), public_key: publisher.public_key_b64() },
        }, key_id: signer.key_id.clone(), public_key: signer.public_key_b64(), signature: String::new(),
    };
    engine::trust_analysis::sign_recipe_permit(&mut permit, &signer).unwrap();
    let mut document = model::Document {
        statement: model::Statement {
            title: draft.title, issuer: draft.issuer, claim: permit.terms.claim.clone(), revision: 1,
            descriptor: draft.descriptor, comparator: draft.comparator, amount: nucleus::DecimalValue::parse_inferred("12").unwrap(),
            as_of: now, issued_at: now, expires_at: now + 3600, withdrawn: false, audience: Audience::Public,
            authorship: model::Authorship::Generated { recipe: permit.recipe.clone(), revision: 1, partial: false, state: recipes::EvaluationState::Available, permit: Box::new(permit) },
        }, key_id: publisher.key_id.clone(), public_key: publisher.public_key_b64(), signature: String::new(),
    };
    document.signature = publisher.sign_bytes(&document.signing_bytes().unwrap());
    let review = serving::Review { person, previous: None, request, artifact: Artifact::Claim(document), source_version: 1, source_revision: 0, reviewed_at: now, expires_at: now + 86400 };
    let result = json!({"kind":"serving_preview", "review":review, "hash":review.hash().unwrap(), "artifact_hash":review.artifact.hash().unwrap()});
    app.world_mut().get_mut::<View>(owner).unwrap().result = result.clone();
    assert!(serving_controls::request(app.world().get::<View>(owner).unwrap(), &serving_controls::UiCommand::Publish).is_ok());
    render(app.world_mut(), owner);
    let labels = app.world_mut().query::<&Text>().iter(app.world()).map(|text|text.0.as_str()).collect::<Vec<_>>().join(" ");
    assert!(labels.contains("The displayed output is an example"));
    assert!(labels.contains("current and future outputs"));
    let generation = app.world().get::<View>(owner).unwrap().generation;
    app.world_mut().get_mut::<View>(owner).unwrap().pending = Some(("old-rolling-review".into(),generation));
    serving_controls::apply(app.world_mut(), owner, &serving_controls::UiCommand::Scope);
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    assert!(serving_controls::request(app.world().get::<View>(owner).unwrap(), &serving_controls::UiCommand::Publish).is_err());
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk { id:"old-rolling-review".into(), created: None, facts: 0, warnings: Vec::new(), data: Some(json!({"trust":result})) }));
    app.add_systems(Update, receive);
    app.update();
    assert!(app.world().get::<View>(owner).unwrap().result.is_null());
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    view.serving.scope = Scope::Recipe;
    clear_scope(&mut view);
    assert_eq!(view.serving.scope, Scope::Fixed);
    assert!(view.serving.source.is_empty());
}

#[cfg_attr(test, test)]
fn merged_proof_controls_keep_verification_separate_and_review_context_before_network_work() {
    use nucleus::trust_analysis::remote::{self as model, ReceiptSource};
    let (mut app, owner) = setup();
    app.init_resource::<Messages<CellMessage>>();
    let person = nucleus::new_uid("r");
    let source = ReceiptSource { origin: nucleus::new_uid("r"), occurrence: nucleus::karma::canonical_hash("lince.test.source.v1", &"public source").unwrap() };
    let subject = Identity::Organ(nucleus::new_uid("r"));
    let artifact = nucleus::karma::canonical_hash("lince.test.artifact.v1", &"original").unwrap();
    let record = nucleus::new_uid("r");
    let job = nucleus::new_uid("ask");
    let verified = json!({"kind":"remote_verified","job":job,"checked_at":1000,"partial":true,"notice":"private merged proof","receipt_sources":[source],"contact_subjects":[subject],"signatures":[{"original":{"provenance":"record","record":record,"version":1},"signature":"accepted"},{"original":{"provenance":"artifact","hash":artifact},"signature":"unknown_identity"}]});
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = nucleus::new_uid("r");
        view.analysis.organ = true;
        view.remote.job = Some(job);
        view.remote.result = json!({"kind":"remote_progress","state":"completed"});
        assert!(remote_controls::receive(&mut view, &verified));
        assert!(matches!(remote_controls::request(&view, &remote_controls::UiCommand::Verify).unwrap(), model::Command::Verify { .. }));
        remote_controls::receive(&mut view, &json!({"kind":"remote_progress","state":"completed","artifacts":[]}));
        assert_eq!(view.remote.verified, verified);
    }
    render(app.world_mut(), owner);
    let text = app.world_mut().query::<&Text>().iter(app.world()).map(|text| text.0.as_str()).collect::<Vec<_>>().join(" ");
    assert!(text.contains("Verify and analyze permitted local and collected originals"));
    assert!(text.contains("Open permitted local proof Record"));
    assert!(text.contains("Open collected original proof"));
    assert!(text.contains("Review additional public proof-source and contact questions"));
    let generation = app.world().get::<View>(owner).unwrap().generation;
    Command::Remote(remote_controls::UiCommand::Context).apply(app.world_mut(), owner);
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        assert!(view.remote.job.is_none());
        assert!(view.remote.review.is_none());
        assert!(view.remote.verified.is_null());
        assert_eq!(view.remote.receipt_sources, vec![source]);
        assert_eq!(view.remote.contact_subjects, vec![subject]);
        let draft = remote_controls::selection(&view).unwrap();
        let questions = serde_json::to_value(draft.questions().unwrap()).unwrap();
        assert!(questions.as_array().unwrap().iter().any(|question| question["kind"] == "receipt_source"));
        assert!(!questions.to_string().contains(&person));
        view.pending = Some(("old-merged-result".into(), generation));
    }
    app.world_mut().write_message(CellMessage(cell::ServerMessage::ActionOk { id:"old-merged-result".into(), created:None, facts:0, warnings:Vec::new(), data:Some(json!({"trust":verified})) }));
    app.add_systems(Update, receive);
    app.update();
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    assert!(view.remote.verified.is_null());
    clear_scope(&mut view);
    assert!(view.remote.receipt_sources.is_empty());
    assert!(view.remote.contact_subjects.is_empty());
}

#[cfg_attr(test, test)]
fn combined_advisory_results_keep_raw_amounts_and_typed_proof_navigation() {
    let (mut app, owner) = setup();
    let person = nucleus::new_uid("r");
    let subject = Identity::Organ(nucleus::new_uid("r"));
    let record = nucleus::new_uid("r");
    let artifact = nucleus::karma::canonical_hash("lince.native.combined-proof.v1", &"original").unwrap();
    let root = app.world().get::<View>(owner).unwrap().root;
    app.world_mut().entity_mut(root).insert(crate::native::workspace::Workspaces::default());
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = subject.uid().into();
        view.analysis.organ = true;
        view.remote.job = Some(nucleus::new_uid("ask"));
        view.remote.result = json!({"state":"completed"});
        view.remote.verified = json!({"kind":"remote_verified","checked_at":1000,"partial":false,"analysis":{"groups":[{"raw_quantity":{"value":"12.000000000000000001"},"weighted_index":{"value":"6"},"occurrences":1}],"rows":[{"quantity":{"value":"12.000000000000000001"},"records":[record],"originals":[{"provenance":"artifact","hash":artifact}],"selected":true}],"protocol":[],"withheld":[]},"claim_checks":[],"signatures":[]});
        let draft = remote_controls::selection(&view).unwrap();
        assert_eq!(draft.query, analysis::build(&view).unwrap());
        assert!(matches!(remote_controls::request(&view, &remote_controls::UiCommand::Inspect(artifact.clone())).unwrap(), nucleus::trust_analysis::remote::Command::Inspect { artifact: actual, .. } if actual == artifact));
        assert!(view.remote.review.is_none());
    }
    render(app.world_mut(), owner);
    let text = app.world_mut().query::<&Text>().iter(app.world()).map(|text| text.0.as_str()).collect::<Vec<_>>().join(" ");
    assert!(text.contains("raw 12.000000000000000001"));
    assert!(text.contains("weighted index 6"));
    assert!(text.contains("Open original evidence Record"));
    assert!(text.contains("Inspect collected original proof"));
    activate_caption(app.world_mut(),owner,"Open original evidence Record");
    let areas = app.world_mut().query::<&crate::native::area::InfluenceArea>().iter(app.world()).filter_map(|area|area.protein.as_ref()).collect::<Vec<_>>();
    assert!(areas.iter().any(|config|matches!(config.source,crate::native::protein_area::Source::Local) && config.draft.query["where"][0]["all"][0]["uid_eq"] == record));
    let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
    clear_scope(&mut view);
    assert!(view.remote.verified.is_null());
    assert!(view.remote.job.is_none());
}

#[cfg_attr(test, test)]
fn changed_claim_review_discards_old_job_without_starting_new_network_work() {
    use nucleus::trust_analysis::claims::{Authorship, Document, Statement};
    let (mut app, owner) = setup();
    let person = nucleus::new_uid("r");
    let signer = engine::trust::Signer::generate(&person, "review-changed-claim");
    let now = nucleus::execution::now().timestamp();
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.person = person.clone();
        view.analysis.subject = person.clone();
        view.analysis.measurements = vec![nucleus::transfer::receipt::Measurement {
            meaning: nucleus::vocabulary::reference::MeaningReference { vocabulary_uid:nucleus::new_uid("g"), concept_uid:nucleus::new_uid("c"), revision:1, definition_hash:"a".repeat(64) }, unit:None, object_commitment:None,
        }];
    }
    let draft = claims::draft(app.world().get::<View>(owner).unwrap()).unwrap();
    let mut document = Document {
        statement: Statement { title:draft.title, issuer:draft.issuer, claim:nucleus::new_uid("r"), revision:1, descriptor:draft.descriptor, comparator:draft.comparator, amount:nucleus::DecimalValue::parse_inferred("100").unwrap(), as_of:now, issued_at:now, expires_at:now+3600, withdrawn:false, audience:Audience::Public, authorship:Authorship::Manual },
        key_id:signer.key_id.clone(), public_key:signer.public_key_b64(), signature:String::new(),
    };
    engine::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    let published = document.clone();
    document.statement.revision = 2;
    document.statement.descriptor.measurement.meaning.revision += 1;
    engine::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    let job = nucleus::new_uid("ask");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    {
        let mut view = app.world_mut().get_mut::<View>(owner).unwrap();
        view.claims.snapshot = Some(published.clone());
        view.remote.job = Some(job.clone());
        view.remote.verified = json!({"kind":"remote_verified","claim_checks":[{"review_required":true,"document":document}]});
    }
    remote_controls::track_with_sender(app.world_mut(), owner, sender);
    Command::Remote(remote_controls::UiCommand::ReviewClaim(0)).apply(app.world_mut(), owner);
    remote_controls::maintain(app.world_mut());
    let cell::ClientMessage::Act { action:engine::actions::Action::TrustAnalysis { request:Backend::Remote { request:nucleus::trust_analysis::remote::Command::Forget { job:actual } }, .. }, .. } = receiver.try_recv().unwrap() else { panic!("Only forget is sent before a fresh review") };
    assert_eq!(actual, job);
    assert!(receiver.try_recv().is_err());
    let view = app.world().get::<View>(owner).unwrap();
    assert!(view.remote.job.is_none());
    assert!(view.remote.review.is_none());
    assert_eq!(view.claims.snapshot.as_ref(), Some(&document));
    assert_eq!(published.statement.revision, 1);
    assert_eq!(published.statement.descriptor.measurement.meaning.revision, 1);
    assert_eq!(remote_controls::selection(view).unwrap().linked_claims[0].snapshot.as_ref(), Some(&document));
}

crate::native::laboratory_cases! {contact_controls_default_private_and_compose_typed_range_rules,viewer_changes_clear_private_drafts_and_late_trust_replies,edited_drafts_cannot_restore_a_late_confirmation,reader_query_roundtrip_keeps_independent_filters_weights_budgets_and_exclusions,claim_controls_separate_publisher_amount_from_visible_support_and_clear_private_reviews,activity_selection_without_history_ignores_a_response_after_the_draft_changes,recipe_controls_default_paused_bind_public_scope_and_reject_late_edited_reviews,profile_snapshot_checks_cache_only_the_exact_reader_document_and_source_policy,organ_root_controls_require_exact_independent_review_clear_on_viewer_change_and_ignore_old_replies,serving_controls_default_private_review_exact_conditions_and_clear_late_viewer_results,remote_controls_review_exact_questions_keep_local_policy_and_ignore_changed_viewer_replies,remote_view_edits_and_closing_discard_jobs_through_the_original_connection,rolling_serving_controls_require_explicit_scope_review_and_clear_changed_or_late_results,merged_proof_controls_keep_verification_separate_and_review_context_before_network_work,combined_advisory_results_keep_raw_amounts_and_typed_proof_navigation,changed_claim_review_discards_old_job_without_starting_new_network_work,custody_controls_require_current_policy_keep_exact_limits_and_clear_private_reviews,forwarding_controls_require_separate_consent_and_cancel_only_the_reviewed_offer,}

#[cfg_attr(test,test)]
fn forwarding_controls_require_separate_consent_and_cancel_only_the_reviewed_offer() {
    use nucleus::trust_analysis::custody::{Command as Request,Policy};
    let (mut app,owner)=setup();
    let peer=nucleus::new_uid("r");
    {
        let mut view=app.world_mut().get_mut::<View>(owner).unwrap();
        custody_controls::receive(&mut view,&json!({"kind":"custody_policy","state":{"version":1,"policy":Policy::default()}}));
        let Request::SetPolicy {policy,..}=custody_controls::request(&view,&custody_controls::UiCommand::Save).unwrap() else {panic!("Expected reviewed policy")};
        assert!(!policy.forwarding.enabled);
    }
    custody_controls::apply(app.world_mut(),owner,&custody_controls::UiCommand::Forwarding);
    let mut view=app.world_mut().get_mut::<View>(owner).unwrap();
    assert!(custody_controls::request(&view,&custody_controls::UiCommand::Save).is_err());
    view.rules.push(Rule {scope:RuleScope::CustodyForwardPeer,identities:format!("organ:{peer}"),..Default::default()});
    let Request::SetPolicy {policy,..}=custody_controls::request(&view,&custody_controls::UiCommand::Save).unwrap() else {panic!("Expected reviewed policy")};
    assert!(policy.forwarding.enabled);
    assert_eq!(policy.forwarding.peers.include[0].identities,vec![Identity::Organ(peer)]);
    let job=nucleus::new_uid("r");
    let expected=nucleus::karma::canonical_hash("lince.test.native.forwarding.offer.v1",&"reviewed").unwrap();
    let cancelled=custody_controls::request(&view,&custody_controls::UiCommand::Cancel(job.clone(),expected.clone())).unwrap();
    assert!(matches!(cancelled,Request::CancelDelivery {record,expected_offer} if record==job && expected_offer==expected));
    clear_scope(&mut view);
    assert!(custody_controls::request(&view,&custody_controls::UiCommand::Save).is_err());
}

#[cfg_attr(test, test)]
fn custody_controls_require_current_policy_keep_exact_limits_and_clear_private_reviews() {
    use nucleus::trust_analysis::{Condition,Filter,custody::{Command as Request,Policy}};
    let (mut app,owner)=setup();
    let mut policy=Policy::default();
    policy.enabled=true;
    policy.storage_bytes+=17;
    policy.retention_seconds=86401;
    policy.senders=Filter {include:vec![Condition {known:Some(true),..Default::default()}],exclude:vec![]};
    policy.readers=policy.senders.clone();
    {
        let mut view=app.world_mut().get_mut::<View>(owner).unwrap();
        assert!(custody_controls::request(&view,&custody_controls::UiCommand::Save).is_err());
        custody_controls::receive(&mut view,&json!({"kind":"custody_policy","state":{"version":7,"policy":policy}}));
        let request=custody_controls::request(&view,&custody_controls::UiCommand::Save).unwrap();
        let Request::SetPolicy {policy:actual,expected_version}=request else {panic!("Expected reviewed custody policy")};
        assert_eq!(expected_version,7);
        assert_eq!(actual,policy);
        custody_controls::capture(&mut view.custody,custody_controls::Field::Term,"0".into());
        assert!(custody_controls::request(&view,&custody_controls::UiCommand::Save).is_err());
        clear_scope(&mut view);
        assert!(custody_controls::request(&view,&custody_controls::UiCommand::Save).is_err());
        assert!(!view.rules.iter().any(|rule|matches!(rule.scope,RuleScope::CustodyReader|RuleScope::CustodySender)));
    }
}
