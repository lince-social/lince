use super::*;
use crate::trust_analysis::serving;

fn recipe() -> recipes::Recipe {
    let statement = statement();
    let mut inputs = Query::for_subject(statement.descriptor.subject.clone());
    inputs.measurements = vec![statement.descriptor.measurement.clone()];
    inputs.from = statement.descriptor.from;
    inputs.rely_on_all_visible = true;
    let definition = recipes::Definition {
        starting_claim_revision: 0,
        recipe: new_uid("r"),
        revision: 1,
        controller: statement.issuer.uid().into(),
        terms: recipes::Terms {
            issuer: statement.issuer,
            claim: statement.claim,
            title: statement.title,
            descriptor: statement.descriptor,
            comparator: statement.comparator,
            audience: Audience::Public,
            issued_at: 1001,
            expires_at: 2000,
            lifetime_seconds: 600,
            delegate: recipes::Delegate {
                organ: new_uid("r"),
                cell: new_uid("r"),
                key_id: "publishing-cell".into(),
                public_key: format!("{}=", "A".repeat(43)),
            },
        },
        inputs,
        start_enabled: false,
        refresh_seconds: 60,
    };
    recipes::Recipe {
        permit: recipes::Permit {
            recipe: definition.id().unwrap(),
            revision: 1,
            terms: definition.terms.clone(),
            key_id: "original-person".into(),
            public_key: format!("{}=", "A".repeat(43)),
            signature: format!("{}==", "A".repeat(86)),
        },
        definition,
    }
}

#[test]
fn generated_permits_bind_scope_without_exporting_private_recipe_policy() {
    let recipe = recipe();
    recipe.validate().unwrap();
    let mut statement = statement();
    statement.title = recipe.definition.terms.title.clone();
    statement.issuer = recipe.definition.terms.issuer.clone();
    statement.claim = recipe.definition.terms.claim.clone();
    statement.descriptor = recipe.definition.terms.descriptor.clone();
    statement.audience = Audience::Public;
    statement.issued_at = 1100;
    statement.as_of = 1100;
    statement.expires_at = 1700;
    statement.authorship = Authorship::Generated {
        recipe: recipe.permit.recipe.clone(),
        revision: 1,
        partial: false,
        state: recipes::EvaluationState::Available,
        permit: Box::new(recipe.permit.clone()),
    };
    let document = Document {
        statement,
        key_id: recipe.permit.terms.delegate.key_id.clone(),
        public_key: recipe.permit.terms.delegate.public_key.clone(),
        signature: format!("{}==", "A".repeat(86)),
    };
    document.validate().unwrap();
    let encoded = serde_json::to_string(&document).unwrap();
    assert!(!encoded.contains("\"inputs\""));
    assert!(!encoded.contains("\"controller\""));
    assert!(!encoded.contains(&recipe.definition.recipe));
    let serving = serving::Review {
        person: recipe.definition.controller.clone(),
        previous: None,
        request: serving::Request {
            source: serving::Source {
                record: new_uid("r"),
                kind: serving::SourceKind::Claim,
            },
            scope: serving::Scope::Recipe,
            as_organ: false,
            audience: Default::default(),
            lifetime_seconds: 1000,
        },
        artifact: serving::Artifact::Claim(document.clone()),
        source_version: 1,
        source_revision: 0,
        reviewed_at: 1100,
        expires_at: 2000,
    };
    serving.hash().unwrap();
    let mut newer = document.clone();
    newer.statement.amount = amount("125.000000000000000001");
    newer.statement.revision = 2;
    newer.statement.issued_at = 1200;
    newer.statement.as_of = 1200;
    newer.statement.expires_at = 1800;
    let artifact = serving::Artifact::Claim(newer.clone());
    assert!(serving.allows_current(&artifact, 1, 3));
    assert!(!serving.allows_current(&artifact, 2, 3));
    let mut fixed = serving.clone();
    fixed.request.scope = serving::Scope::Fixed;
    fixed.expires_at = 1700;
    fixed.hash().unwrap();
    assert!(!fixed.allows_current(&artifact, 1, 0));
    assert_ne!(fixed.hash().unwrap(), serving.hash().unwrap());
    let Authorship::Generated { permit, .. } = &mut newer.statement.authorship else {
        panic!("permit");
    };
    permit.terms.title = "Different reviewed title".into();
    newer.statement.title = permit.terms.title.clone();
    assert!(!serving.allows_current(&serving::Artifact::Claim(newer), 1, 0));
    let mut contact = serving.request.clone();
    contact.source.kind = serving::SourceKind::Contact;
    assert!(contact.validate().is_err());
    let mut manual = serving.clone();
    let serving::Artifact::Claim(manual_document) = &mut manual.artifact else {
        panic!("claim");
    };
    manual_document.statement.authorship = Authorship::Manual;
    assert!(manual.hash().is_err());
    let mut changed = document.clone();
    changed.statement.audience = Audience::default();
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.statement.descriptor.measurement.meaning.revision += 1;
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.statement.expires_at = 1800;
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.key_id = "unreviewed-key".into();
    assert!(changed.validate().is_err());
    let mut changed = recipe.clone();
    changed.definition.inputs.rely_on_all_visible = false;
    assert!(changed.validate().is_err());
}

#[test]
fn automatic_recipes_reject_private_graph_criteria_and_imported_instructions() {
    let recipe = recipe();
    let mut changed = recipe.definition.clone();
    changed.inputs.contacts = Some(paths::Policy::for_anchor(changed.terms.issuer.clone()));
    assert!(changed.validate().is_err());
    let mut changed = recipe.definition.clone();
    changed.inputs.issuers.include.push(Condition {
        known: Some(true),
        ..Default::default()
    });
    assert!(changed.validate().is_err());
    let mut changed = recipe.definition.clone();
    changed.inputs.counterparties.exclude.push(Condition {
        upper: Some(crate::visibility::Bound {
            value: 3,
            inclusive: true,
        }),
        ..Default::default()
    });
    assert!(changed.validate().is_err());
    let mut encoded = serde_json::to_value(&recipe).unwrap();
    encoded["execute"] = serde_json::json!({"kind":"set_quantity","quantity":1000});
    assert!(serde_json::from_value::<recipes::Recipe>(encoded).is_err());
}

#[test]
fn unavailable_generated_summaries_withdraw_instead_of_claiming_zero() {
    let recipe = recipe();
    let mut statement = statement();
    let terms = &recipe.definition.terms;
    statement.title = terms.title.clone();
    statement.issuer = terms.issuer.clone();
    statement.claim = terms.claim.clone();
    statement.descriptor = terms.descriptor.clone();
    statement.audience = terms.audience.clone();
    statement.issued_at = 1100;
    statement.as_of = 1100;
    statement.expires_at = 1700;
    statement.amount = amount("0");
    statement.authorship = Authorship::Generated {
        recipe: recipe.permit.recipe.clone(),
        revision: 1,
        partial: true,
        state: recipes::EvaluationState::Unknown,
        permit: Box::new(recipe.permit),
    };
    assert!(statement.validate().is_err());
    statement.withdrawn = true;
    statement.validate().unwrap();
    statement.amount = amount("100");
    assert!(statement.validate().is_err());
}
