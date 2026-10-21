use super::*;
use nucleus::trust_analysis::claims::{
    self as claim,
    recipes::{self, commands::Command as RecipeCommand},
};
#[path = "recipes/remote.rs"]
mod remote;
#[path = "recipes/serving.rs"]
mod serving;

fn request(person: &Person, id: &str, request: RecipeCommand) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: id.into(),
        request: Command::Claims {
            request: claim::commands::Command::Recipes { request },
        },
    }
}

pub(super) async fn automatic_summary_uses_public_receiver_evidence(
    reader: &Engine,
    person: &Person,
    inputs: &Query,
    analysis: &Value,
    origin: &Engine,
) {
    let _publisher = Authorizer::new(reader, 78).await;
    let measurement = serde_json::from_value(analysis["groups"][0]["measurement"].clone()).unwrap();
    let descriptor = claim::Descriptor {
        subject: inputs.subject.clone(),
        role: inputs.role,
        measurement,
        level: nucleus::trust_analysis::query::EvidenceLevel::NeedLinkedReceiver,
        metric: claim::Metric::Quantity,
        counterparty_basis: inputs.counterparty_basis,
        from: None,
        until: None,
    };
    let draft = recipes::commands::Draft {
        claim: claim::commands::Draft {
            category: Default::default(),
            issuer: Identity::Person(person.uid.clone()),
            claim: None,
            title: "Automatic public receiver-confirmed contribution history".into(),
            descriptor,
            comparator: claim::Comparator::AtLeast,
            amount: String::new(),
            as_of: None,
            lifetime_seconds: 2 * 86400,
            expected_revision: 0,
            withdrawn: false,
            audience: nucleus::trust_analysis::contacts::Audience::Public,
            key_id: person.signer.key_id.clone(),
            public_key: person.signer.public_key_b64(),
        },
        recipe: None,
        expected_revision: 0,
        inputs: inputs.clone(),
        permit_seconds: 3 * 86400,
        refresh_seconds: 60,
        start_enabled: true,
    };
    let preview = reader
        .act(
            request(
                person,
                "analysis:recipe-preview",
                RecipeCommand::Preview { draft },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let mut review: recipes::commands::Review =
        serde_json::from_value(preview["trust"]["review"].clone()).unwrap();
    engine::trust_analysis::sign_recipe_permit(&mut review.recipe.permit, &person.signer).unwrap();
    let record = reader
        .act(
            request(
                person,
                "analysis:recipe-save",
                RecipeCommand::Save {
                    review,
                    expected_review: serde_json::from_value(preview["trust"]["hash"].clone())
                        .unwrap(),
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let outcome = reader
        .act(
            request(
                person,
                "analysis:recipe-calculate",
                RecipeCommand::Recompute {
                    record: record.clone(),
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
    assert_eq!(outcome.facts.len(), 1);
    let output = outcome.created.unwrap();
    let saved =
        store::profile_claims::get_on(&mut *reader.store.pool.acquire().await.unwrap(), &output)
            .await
            .unwrap();
    assert!(!saved.document.statement.withdrawn);
    assert_eq!(
        saved.document.statement.amount.canonical(),
        "12.000000000000000001"
    );
    assert!(matches!(
        &saved.document.statement.authorship,
        claim::Authorship::Generated {
            state: recipes::EvaluationState::Available,
            partial: false,
            ..
        }
    ));
    assert_eq!(
        reader
            .authenticate_profile_claim(&saved.document, nucleus::execution::now().timestamp())
            .await
            .unwrap(),
        engine::trust_analysis::IdentityAuthenticity::Accepted
    );
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    reader.schedule_claim_recipes().await.unwrap();
    assert_eq!(reader.refresh_claim_recipes_once().await.unwrap(), 1);
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let occurrences: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
    assert_eq!(occurrences, 0);
    let publication: String = store::sqlx::query_scalar(
        "SELECT record_uid FROM transfer_receipt_record WHERE kind='publication' LIMIT 1",
    )
    .fetch_one(&reader.store.pool)
    .await
    .unwrap();
    let original = store::transfer_receipts::get(&reader.store.pool, &publication)
        .await
        .unwrap();
    let expiry = original
        .document
        .public_bundle()
        .unwrap()
        .publication
        .scope
        .expires_at;
    assert_eq!(saved.document.statement.expires_at, expiry);
    remote::verify_original_receipt_fragments(reader, origin, inputs).await;
    let serving = serving::prepare(reader, person, &record, &saved.document).await;
    let clock = nucleus::execution::Execution::new([127; 32], (expiry + 1) * 1000).unwrap();
    let expired = clock
        .scope(reader.act(
            request(
                person,
                "analysis:recipe-expired-input",
                RecipeCommand::Recompute {
                    record: record.clone(),
                },
            ),
            Some(person.uid.clone()),
        ))
        .await
        .unwrap();
    let output = expired.created.unwrap();
    let unavailable =
        store::profile_claims::get_on(&mut *reader.store.pool.acquire().await.unwrap(), &output)
            .await
            .unwrap();
    assert!(unavailable.document.statement.withdrawn);
    assert!(matches!(
        &unavailable.document.statement.authorship,
        claim::Authorship::Generated {
            state: recipes::EvaluationState::Unknown,
            partial: true,
            ..
        }
    ));
    serving::check_updated(reader, person, &record, &unavailable.document, &serving).await;
}
