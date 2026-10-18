use super::*;
use super::{
    discovery::{discover, question},
    recipes, serving,
};
use nucleus::trust_analysis::{
    claims::{
        self,
        recipes::{Mode, commands::Command as RecipeCommand},
    },
    discovery::Question,
    serving::{Artifact, Chunk, Command as Serving, Request, Scope, Source, SourceKind},
};

async fn preview(
    node: &Engine,
    person: &support::Person,
    record: &str,
    audience: nucleus::trust_analysis::Filter,
) -> Value {
    node.act(
        serving::command(
            person,
            &nucleus::new_uid("rolling-test"),
            Serving::Preview {
                request: Request {
                    source: Source {
                        record: record.into(),
                        kind: SourceKind::Claim,
                    },
                    scope: Scope::Recipe,
                    as_organ: false,
                    audience,
                    lifetime_seconds: 86400,
                },
            },
        ),
        Some(person.uid.clone()),
    )
    .await
    .unwrap()
    .data
    .unwrap()["trust"]
        .clone()
}

async fn output(node: &Engine, person: &support::Person, record: &str) -> String {
    recipes::act(
        node,
        person,
        &nucleus::new_uid("rolling-test"),
        RecipeCommand::Recompute {
            record: record.into(),
        },
    )
    .await
    .created
    .unwrap()
}

async fn control(node: &Engine, person: &support::Person, record: &str, revision: u64, mode: Mode) {
    recipes::act(
        node,
        person,
        &nucleus::new_uid("rolling-test"),
        RecipeCommand::Control {
            record: record.into(),
            expected_revision: revision,
            mode,
        },
    )
    .await;
}

#[test]
fn recipe_serving_is_deliberate_rolls_only_within_the_permit_and_stops_on_pause_revoke_and_stop() {
    support::run_async_test("rolling-recipe-serving", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let outsider = support::engine().await;
        let person = support::person(&node, "rolling.controller").await;
        authorize(&node, &person).await;
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "record", "delete")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        let root = recipes::publishing(&node).await;
        let (_, endpoint, _) = serving::peer(&node, &reader, &person, 143).await;
        let (excluded_organ, excluded, _) = serving::peer(&node, &outsider, &person, 144).await;
        let recipe = recipes::save(&node, &person, "rolling:recipe").await;
        let first = output(&node, &person, &recipe).await;
        let fixed =
            serving::review(&node, &person, &first, SourceKind::Claim, serving::all()).await;
        let first_hash = serde_json::from_value(fixed["artifact_hash"].clone()).unwrap();
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_err());
        serving::approve(&node, &person, &fixed, "rolling:fixed")
            .await
            .unwrap();
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_ok());
        let mut audience = serving::all();
        audience.exclude.push(nucleus::trust_analysis::Condition {
            identities: vec![Identity::Organ(excluded_organ)],
            ..Default::default()
        });
        let reviewed = preview(&node, &person, &recipe, audience).await;
        let review: nucleus::trust_analysis::serving::Review =
            serde_json::from_value(reviewed["review"].clone()).unwrap();
        let Artifact::Claim(sample) = &review.artifact else {
            panic!("generated claim");
        };
        let claims::Authorship::Generated { permit, .. } = &sample.statement.authorship else {
            panic!("permit");
        };
        assert_eq!(review.expires_at, permit.terms.expires_at);
        assert!(review.expires_at > sample.statement.expires_at);
        let request = question(Question::ClaimSeries {
            issuer: sample.statement.issuer.clone(),
            claim: sample.statement.claim.clone(),
        });
        let grant = serving::approve(&node, &person, &reviewed, "rolling:approve")
            .await
            .unwrap()
            .created
            .unwrap();
        let before = serving::counts(&node).await;
        let replay = serving::approve(&node, &person, &reviewed, "rolling:approve")
            .await
            .unwrap();
        assert_eq!(replay.created.as_deref(), Some(grant.as_str()));
        let replay = replay.data.unwrap();
        assert_eq!(replay["trust"]["scope"], "recipe");
        assert_eq!(replay["trust"]["expires_at"], review.expires_at);
        assert_eq!(serving::counts(&node).await, before);
        let page = discover(&node, &endpoint, &request).await.unwrap();
        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].expires_at, sample.statement.expires_at);
        assert_eq!(serving::counts(&node).await, before);
        control(&node, &person, &recipe, 0, Mode::Paused).await;
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_ok());
        node.act(
            Action::DeleteRecord {
                target: fixed_record(&node, &person, &first).await,
            },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_err());
        assert!(
            discover(&node, &endpoint, &request)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        control(&node, &person, &recipe, 1, Mode::Enabled).await;
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_ok());
        control(&node, &person, &recipe, 2, Mode::Revoked).await;
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_err());
        let withdrawn = output(&node, &person, &recipe).await;
        let current = store::profile_claims::get_on(
            &mut *node.store.pool.acquire().await.unwrap(),
            &withdrawn,
        )
        .await
        .unwrap();
        let hash = Artifact::Claim(current.document.clone()).hash().unwrap();
        assert_ne!(first_hash, hash);
        let before = serving::counts(&node).await;
        let page = discover(&node, &endpoint, &request).await.unwrap();
        assert_eq!(page.headers.len(), 1);
        assert_eq!(page.headers[0].hash, hash);
        assert!(
            discover(&node, &excluded, &request)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        assert!(serving::fetch(&node, &excluded, &hash).await.is_err());
        let reply = serving::fetch(&node, &endpoint, &hash).await.unwrap();
        let chunk: Chunk = serde_json::from_value(reply["chunk"].clone()).unwrap();
        let artifact = nucleus::trust_analysis::serving::assemble(
            &hash,
            &[chunk],
            nucleus::execution::now().timestamp(),
        )
        .unwrap();
        assert_eq!(artifact, Artifact::Claim(current.document));
        let bytes = String::from_utf8(artifact.bytes().unwrap()).unwrap();
        for private in [&recipe, &grant, &withdrawn, &first] {
            assert!(!bytes.contains(private));
        }
        assert!(serving::fetch(&node, &endpoint, &first_hash).await.is_err());
        assert_eq!(serving::counts(&node).await, before);
        node.act(
            Action::DeleteRecord { target: grant },
            Some(person.uid.clone()),
        )
        .await
        .unwrap();
        assert!(
            discover(&node, &endpoint, &request)
                .await
                .unwrap()
                .headers
                .is_empty()
        );
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
        node.publish_roster(&root, Vec::new()).await.unwrap();
    });
}

async fn fixed_record(node: &Engine, person: &support::Person, source: &str) -> String {
    store::sqlx::query_scalar("SELECT record_uid FROM record_evidence_serving WHERE owner_person_uid=? AND source_record_uid=? ORDER BY sequence DESC LIMIT 1").bind(&person.uid).bind(source).fetch_one(&node.store.pool).await.unwrap()
}

#[test]
fn recipe_serving_rechecks_controller_permissions_and_current_publishing_roster() {
    support::run_async_test("rolling-recipe-authority", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let person = support::person(&node, "rolling.authority").await;
        authorize(&node, &person).await;
        let root = recipes::publishing(&node).await;
        let (_, endpoint, _) = serving::peer(&node, &reader, &person, 146).await;
        let recipe = recipes::save(&node, &person, "rolling:authority-recipe").await;
        output(&node, &person, &recipe).await;
        let reviewed = preview(&node, &person, &recipe, serving::all()).await;
        let hash = serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
        serving::approve(&node, &person, &reviewed, "rolling:authority-approval")
            .await
            .unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_ok());
        let roster = store::roster::get(&node.store.pool, &root.actor_uid)
            .await
            .unwrap()
            .unwrap();
        let roster: engine::roster::Roster = serde_json::from_str(&roster.payload).unwrap();
        node.publish_roster(&root, Vec::new()).await.unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
        node.publish_roster(&root, roster.cells).await.unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_ok());
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::revoke(&node.store.pool, role, permission)
            .await
            .unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_ok());
        let future = nucleus::execution::Execution::new(
            [147; 32],
            (reviewed["review"]["artifact"]["document"]["statement"]["expires_at"]
                .as_i64()
                .unwrap()
                + 1)
                * 1000,
        )
        .unwrap();
        future
            .scope(async {
                assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
            })
            .await;
    });
}

#[test]
fn recipe_serving_rejects_manual_claims_changed_permits_and_hidden_superseding_recipes() {
    support::run_async_test("rolling-recipe-scope", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let person = support::person(&node, "rolling.revisions").await;
        authorize(&node, &person).await;
        let root = recipes::publishing(&node).await;
        let (_, endpoint, _) = serving::peer(&node, &reader, &person, 145).await;
        let record = recipes::save(&node, &person, "rolling:scope-recipe").await;
        let first = output(&node, &person, &record).await;
        let reviewed = preview(&node, &person, &record, serving::all()).await;
        let hash = serde_json::from_value(reviewed["artifact_hash"].clone()).unwrap();
        let grant = serving::approve(&node, &person, &reviewed, "rolling:scope-grant")
            .await
            .unwrap()
            .created
            .unwrap();
        let mut request: Request =
            serde_json::from_value(reviewed["review"]["request"].clone()).unwrap();
        request.source.record = first;
        let before = serving::counts(&node).await;
        assert!(
            node.act(
                serving::command(
                    &person,
                    "rolling:output-as-recipe",
                    Serving::Preview {
                        request: request.clone()
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        request.source.kind = SourceKind::Contact;
        assert!(request.validate().is_err());
        assert_eq!(serving::counts(&node).await, before);
        let saved =
            store::claim_recipes::get_on(&mut *node.store.pool.acquire().await.unwrap(), &record)
                .await
                .unwrap();
        let mut revised = recipes::draft(&person);
        revised.recipe = Some(saved.recipe.definition.recipe.clone());
        revised.expected_revision = 1;
        revised.claim.claim = Some(saved.recipe.definition.terms.claim.clone());
        revised.claim.title = "A newly reviewed public meaning".into();
        let review = recipes::act(
            &node,
            &person,
            "rolling:v2-preview",
            RecipeCommand::Preview { draft: revised },
        )
        .await
        .data
        .unwrap();
        let newer = recipes::act(
            &node,
            &person,
            "rolling:v2-save",
            RecipeCommand::Save {
                review: serde_json::from_value(review["trust"]["review"].clone()).unwrap(),
                expected_review: serde_json::from_value(review["trust"]["hash"].clone()).unwrap(),
            },
        )
        .await
        .created
        .unwrap();
        output(&node, &person, &newer).await;
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
        let mut tx = store::write_tx(&node.store.pool).await.unwrap();
        store::sqlx::query("UPDATE record SET deleted_at=? WHERE uid=?")
            .bind(nucleus::execution::now().to_rfc3339())
            .bind(&newer)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
        assert!(
            store::evidence_serving::get_on(&mut *node.store.pool.acquire().await.unwrap(), &grant)
                .await
                .unwrap()
                .current
        );
        node.publish_roster(&root, Vec::new()).await.unwrap();
        assert!(serving::fetch(&node, &endpoint, &hash).await.is_err());
    });
}
