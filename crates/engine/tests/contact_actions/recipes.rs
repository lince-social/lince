use super::*;
use nucleus::trust_analysis::claims::{
    self as model,
    recipes::{
        self as model_recipe, Mode,
        commands::{Command as RecipeCommand, Draft},
    },
};

pub(super) async fn publishing(node: &Engine) -> engine::trust::Signer {
    let organ = store::organs::local(&node.store.pool)
        .await
        .unwrap()
        .unwrap();
    let cell = store::cells::local(&node.store.pool)
        .await
        .unwrap()
        .unwrap();
    let root = engine::trust::Signer::generate(&organ.uid, engine::roster::ROOT_KEY_ID);
    let writer =
        engine::trust::Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    node.publish_root_key(&root).await.unwrap();
    node.set_organ_signer(writer.clone()).await.unwrap();
    node.publish_roster(
        &root,
        vec![engine::roster::CellEntry {
            cell_uid: cell.uid,
            node_id: iroh::SecretKey::from_bytes(&[84; 32]).public().to_string(),
            label: "Recipe publishing Cell".into(),
            operational_key: writer.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: engine::roster::full_capabilities(),
        }],
    )
    .await
    .unwrap();
    root
}

fn request(person: &support::Person, id: &str, request: RecipeCommand) -> Action {
    action(
        person,
        id,
        Command::Claims {
            request: model::commands::Command::Recipes { request },
        },
    )
}

pub(super) async fn act(
    node: &Engine,
    person: &support::Person,
    id: &str,
    command: RecipeCommand,
) -> engine::actions::ActionOutcome {
    let mut action = request(person, id, command);
    engine::trust_analysis::prepare_contact_action(&mut action, &person.signer).unwrap();
    node.act(action, Some(person.uid.clone())).await.unwrap()
}

pub(super) fn draft(person: &support::Person) -> Draft {
    let mut inputs =
        nucleus::trust_analysis::query::Query::for_subject(Identity::Person(person.uid.clone()));
    inputs.rely_on_all_visible = true;
    Draft {
        claim: claims::draft(person, true),
        recipe: None,
        expected_revision: 0,
        inputs,
        permit_seconds: 86400,
        refresh_seconds: 60,
        start_enabled: true,
    }
}

pub(super) async fn save(node: &Engine, person: &support::Person, id: &str) -> String {
    let reviewed = act(
        node,
        person,
        &format!("{id}:preview"),
        RecipeCommand::Preview {
            draft: draft(person),
        },
    )
    .await
    .data
    .unwrap();
    act(
        node,
        person,
        &format!("{id}:save"),
        RecipeCommand::Save {
            review: serde_json::from_value(reviewed["trust"]["review"].clone()).unwrap(),
            expected_review: serde_json::from_value(reviewed["trust"]["hash"].clone()).unwrap(),
        },
    )
    .await
    .created
    .unwrap()
}

#[test]
fn reviewed_recipe_publishes_scoped_withdrawals_without_private_keys_or_repeated_effects() {
    support::run_async_test("recipe-scoped-publication", || async {
        let node = support::engine().await;
        let reader = support::engine().await;
        let person = support::person(&node, "recipe.controller").await;
        let viewer = support::person(&reader, "recipe.viewer").await;
        authorize(&node, &person).await;
        authorize(&reader, &viewer).await;
        let _root = publishing(&node).await;
        recognize(&reader, &person).await;
        let record = save(&node, &person, "recipe:first").await;
        let alternate = support::person(&node, "recipe.other-device-user").await;
        node.set_signer(alternate.signer).await.unwrap();
        let first = act(
            &node,
            &person,
            "recipe:calculate",
            RecipeCommand::Recompute {
                record: record.clone(),
            },
        )
        .await;
        let output = first.created.unwrap();
        assert_eq!(first.facts.len(), 1);
        let saved =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &output)
                .await
                .unwrap();
        assert!(saved.document.statement.withdrawn);
        assert!(matches!(
            &saved.document.statement.authorship,
            model::Authorship::Generated {
                state: model_recipe::EvaluationState::Unknown,
                ..
            }
        ));
        assert_eq!(
            saved.fact.actor_uid.as_deref(),
            Some(
                store::organs::local(&node.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid
                    .as_str()
            )
        );
        let exported = node
            .act(
                action(
                    &person,
                    "recipe:export",
                    Command::Claims {
                        request: model::commands::Command::Export {
                            record: output.clone(),
                            as_organ: false,
                        },
                    },
                ),
                Some(person.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]["export"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(!exported.contains(&record));
        assert!(!exported.contains("\"controller\""));
        assert!(!exported.contains("\"inputs\""));
        let imported = reader
            .act(
                action(
                    &viewer,
                    "recipe:import",
                    Command::Claims {
                        request: model::commands::Command::Import {
                            document: exported,
                            as_organ: false,
                        },
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let imported = store::profile_claims::get_on(
            &mut *reader.store.pool.acquire().await.unwrap(),
            &imported,
        )
        .await
        .unwrap();
        assert_eq!(imported.document, saved.document);
        let mut forged = saved.document.clone();
        forged.signature = person.signer.sign_bytes(&forged.signing_bytes().unwrap());
        assert!(
            reader
                .authenticate_profile_claim(&forged, nucleus::execution::now().timestamp())
                .await
                .is_err()
        );
        assert_eq!(
            imported.fact.actor_uid.as_deref(),
            Some(viewer.uid.as_str())
        );
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 1);
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        node.set_signer(person.signer.clone()).await.unwrap();
        act(
            &node,
            &person,
            "recipe:pause",
            RecipeCommand::Control {
                record: record.clone(),
                expected_revision: 0,
                mode: Mode::Paused,
            },
        )
        .await;
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 0);
        act(
            &node,
            &person,
            "recipe:enable",
            RecipeCommand::Control {
                record: record.clone(),
                expected_revision: 1,
                mode: Mode::Enabled,
            },
        )
        .await;
        act(
            &node,
            &person,
            "recipe:revoke",
            RecipeCommand::Control {
                record: record.clone(),
                expected_revision: 2,
                mode: Mode::Revoked,
            },
        )
        .await;
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 1);
        let state = act(
            &node,
            &person,
            "recipe:status",
            RecipeCommand::Inspect {
                record: record.clone(),
            },
        )
        .await
        .data
        .unwrap();
        assert_eq!(state["trust"]["state"]["status"], "revoked");
        let last = state["trust"]["state"]["output"].as_str().unwrap();
        assert_ne!(last, output);
        let last =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), last)
                .await
                .unwrap();
        assert!(matches!(
            &last.document.statement.authorship,
            model::Authorship::Generated {
                state: model_recipe::EvaluationState::Revoked,
                ..
            }
        ));
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 0);
        assert!(
            node.act(
                request(
                    &person,
                    "recipe:restart-revoked",
                    RecipeCommand::Control {
                        record,
                        expected_revision: 3,
                        mode: Mode::Enabled
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let occurrences: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
        assert_eq!(occurrences, 0);
    });
}

#[test]
fn recipe_reviews_reject_scope_changes_private_criteria_and_foreign_controllers() {
    support::run_async_test("recipe-review-isolation", || async {
        let node = support::engine().await;
        let person = support::person(&node, "recipe.owner").await;
        let other = support::person(&node, "recipe.other").await;
        authorize(&node, &person).await;
        authorize(&node, &other).await;
        node.set_signer(person.signer.clone()).await.unwrap();
        let root = publishing(&node).await;
        let reviewed = act(
            &node,
            &person,
            "recipe:preview",
            RecipeCommand::Preview {
                draft: draft(&person),
            },
        )
        .await
        .data
        .unwrap();
        let mut review: model_recipe::commands::Review =
            serde_json::from_value(reviewed["trust"]["review"].clone()).unwrap();
        review.recipe.definition.terms.audience = Audience::default();
        assert!(
            node.act(
                request(
                    &person,
                    "recipe:changed",
                    RecipeCommand::Save {
                        review,
                        expected_review: serde_json::from_value(reviewed["trust"]["hash"].clone())
                            .unwrap()
                    }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let mut private = draft(&person);
        private
            .inputs
            .issuers
            .include
            .push(nucleus::trust_analysis::Condition {
                known: Some(true),
                ..Default::default()
            });
        assert!(
            node.act(
                request(
                    &person,
                    "recipe:private-query",
                    RecipeCommand::Preview { draft: private }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let record = save(&node, &person, "recipe:retained").await;
        assert!(
            node.act(
                request(
                    &other,
                    "recipe:foreign-controller",
                    RecipeCommand::Inspect {
                        record: record.clone()
                    }
                ),
                Some(other.uid.clone())
            )
            .await
            .is_err()
        );
        node.publish_roster(&root, Vec::new()).await.unwrap();
        assert!(
            node.act(
                request(
                    &person,
                    "recipe:removed-device",
                    RecipeCommand::Recompute { record }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn recipe_authorizations_are_private_immutable_and_hidden_newer_versions_stop_older_jobs() {
    support::run_async_test("recipe-version-controls", || async {
        let node = support::engine().await;
        let person = support::person(&node, "recipe.versions").await;
        authorize(&node, &person).await;
        let _root = publishing(&node).await;
        let first = save(&node, &person, "recipe:version-one").await;
        let saved =
            store::claim_recipes::get_on(&mut *node.store.pool.acquire().await.unwrap(), &first)
                .await
                .unwrap();
        let policy = store::data_visibility::policy(
            &node.store.pool,
            &first,
            nucleus::visibility::Data::Record,
        )
        .await
        .unwrap()
        .unwrap();
        let organ = store::organs::local(&node.store.pool)
            .await
            .unwrap()
            .unwrap();
        assert!(
            policy
                .policy
                .decide_principal(&organ.uid, Some(&person.uid), Some(0))
                .allowed
        );
        assert!(
            !policy
                .policy
                .decide_principal(&organ.uid, None, Some(0))
                .allowed
        );
        assert!(
            store::sqlx::query(
                "UPDATE profile_claim_recipe_record SET revision=2 WHERE record_uid=?"
            )
            .bind(&first)
            .execute(&node.store.pool)
            .await
            .is_err()
        );
        assert!(
            store::sqlx::query(
                "UPDATE profile_claim_recipe_control SET mode='paused' WHERE record_uid=?"
            )
            .bind(&first)
            .execute(&node.store.pool)
            .await
            .is_err()
        );
        let mut fields = serde_json::to_value(&saved.recipe).unwrap();
        fields["definition"]["terms"]["title"] = json!("Unreviewed scope");
        assert!(
            node.act(
                Action::SetExtension {
                    target: first.clone(),
                    namespace: model_recipe::NAMESPACE.into(),
                    fds: fields
                },
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
        let mut revised = draft(&person);
        revised.recipe = Some(saved.recipe.definition.recipe.clone());
        revised.expected_revision = 1;
        revised.claim.claim = Some(saved.recipe.definition.terms.claim.clone());
        let review = act(
            &node,
            &person,
            "recipe:v2-review",
            RecipeCommand::Preview { draft: revised },
        )
        .await
        .data
        .unwrap();
        let second = act(
            &node,
            &person,
            "recipe:v2-save",
            RecipeCommand::Save {
                review: serde_json::from_value(review["trust"]["review"].clone()).unwrap(),
                expected_review: serde_json::from_value(review["trust"]["hash"].clone()).unwrap(),
            },
        )
        .await
        .created
        .unwrap();
        store::sqlx::query("UPDATE record SET deleted_at=? WHERE uid=?")
            .bind(nucleus::execution::now().to_rfc3339())
            .bind(second)
            .execute(&node.store.pool)
            .await
            .unwrap();
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 0);
        assert!(
            node.act(
                request(
                    &person,
                    "recipe:old-recompute",
                    RecipeCommand::Recompute { record: first }
                ),
                Some(person.uid.clone())
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn a_new_manual_withdrawal_suspends_the_old_recipe_instead_of_republishing_it() {
    support::run_async_test("recipe-manual-override", || async {
        let node = support::engine().await;
        let person = support::person(&node, "recipe.manual-override").await;
        authorize(&node, &person).await;
        let _root = publishing(&node).await;
        let record = save(&node, &person, "recipe:before-manual").await;
        let generated = act(
            &node,
            &person,
            "recipe:initial-output",
            RecipeCommand::Recompute {
                record: record.clone(),
            },
        )
        .await
        .created
        .unwrap();
        let generated = store::profile_claims::get_on(
            &mut *node.store.pool.acquire().await.unwrap(),
            &generated,
        )
        .await
        .unwrap();
        let mut withdrawal = claims::draft(&person, true);
        withdrawal.claim = Some(generated.document.statement.claim.clone());
        withdrawal.descriptor = generated.document.statement.descriptor;
        withdrawal.expected_revision = generated.document.statement.revision;
        withdrawal.amount = "0".into();
        withdrawal.withdrawn = true;
        let review = claims::reviewed(&node, &person, withdrawal, "recipe:manual-review").await;
        let manual = claims::publish(&node, &person, &review, "recipe:manual-withdrawal")
            .await
            .unwrap()
            .created
            .unwrap();
        node.schedule_claim_recipes().await.unwrap();
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 1);
        let state = act(
            &node,
            &person,
            "recipe:suspended-status",
            RecipeCommand::Inspect { record },
        )
        .await
        .data
        .unwrap();
        assert_eq!(state["trust"]["state"]["status"], "output_superseded");
        let manual =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &manual)
                .await
                .unwrap();
        assert!(
            store::profile_claims::current_on(
                &mut *node.store.pool.acquire().await.unwrap(),
                &person.uid,
                &manual.document.statement
            )
            .await
            .unwrap()
        );
        assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 0);
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    });
}

#[test]
fn organ_recipe_uses_its_scoped_operational_permit_after_the_owner_key_goes_offline() {
    support::run_async_test("organ-recipe-owner-offline", || async {
        let node = support::engine().await;
        let person = support::person(&node, "recipe.organ-controller").await;
        authorize(&node, &person).await;
        let root = publishing(&node).await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owner-key");
        std::fs::write(&path, root.secret_bytes()).unwrap();
        node.set_root_key_path(path.clone());
        let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
            .await
            .unwrap();
        let permission = store::auth::ensure_permission(&node.store.pool, "organ", "update")
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
        let mut proposal = draft(&person);
        proposal.claim.issuer = Identity::Organ(root.actor_uid.clone());
        proposal.claim.descriptor.subject = Identity::Organ(root.actor_uid.clone());
        proposal.inputs.subject = Identity::Organ(root.actor_uid.clone());
        let reviewed = act(
            &node,
            &person,
            "recipe:organ-preview",
            RecipeCommand::Preview { draft: proposal },
        )
        .await
        .data
        .unwrap();
        let record = act(
            &node,
            &person,
            "recipe:organ-save",
            RecipeCommand::Save {
                review: serde_json::from_value(reviewed["trust"]["review"].clone()).unwrap(),
                expected_review: serde_json::from_value(reviewed["trust"]["hash"].clone()).unwrap(),
            },
        )
        .await
        .created
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        let output = act(
            &node,
            &person,
            "recipe:organ-generate",
            RecipeCommand::Recompute { record },
        )
        .await
        .created
        .unwrap();
        let saved =
            store::profile_claims::get_on(&mut *node.store.pool.acquire().await.unwrap(), &output)
                .await
                .unwrap();
        assert_eq!(
            saved.document.statement.issuer,
            Identity::Organ(root.actor_uid.clone())
        );
        assert!(
            !serde_json::to_string(&saved.document)
                .unwrap()
                .contains(&person.uid)
        );
        std::fs::write(&path, root.secret_bytes()).unwrap();
        let profile = node.act(Action::Social { request:nucleus::social::Command::SaveProfile {
            fields:nucleus::social::ProfileFields { name:"Generated contribution summary".into(), claims:vec![saved.document.clone()], ..Default::default() }, parents:vec![], destinations:vec![],
        } }, Some(person.uid.clone())).await.unwrap().data.unwrap();
        assert_eq!(profile["profile"]["fields"]["claims"], json!([saved.document]));
    });
}

#[test]
fn unavailable_publishing_keys_and_revoked_controller_permissions_suspend_without_outputs() {
    support::run_async_test("recipe-suspended-authority", || async {
        for missing_key in [true, false] {
            let node = support::engine().await;
            let person = support::person(&node, "recipe.suspended").await;
            authorize(&node, &person).await;
            let _root = publishing(&node).await;
            let record = save(&node, &person, "recipe:before-suspension").await;
            if missing_key {
                let organ = store::organs::local(&node.store.pool)
                    .await
                    .unwrap()
                    .unwrap();
                let other = engine::trust::Signer::generate(
                    &organ.uid,
                    &engine::roster::cell_key_id(&nucleus::new_uid("r")),
                );
                node.set_organ_signer(other).await.unwrap();
            } else {
                let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
                    .await
                    .unwrap();
                let permission =
                    store::auth::ensure_permission(&node.store.pool, "record", "create")
                        .await
                        .unwrap();
                store::auth::revoke(&node.store.pool, role, permission)
                    .await
                    .unwrap();
            }
            node.schedule_claim_recipes().await.unwrap();
            assert_eq!(node.refresh_claim_recipes_once().await.unwrap(), 1);
            let inspected = act(
                &node,
                &person,
                "recipe:suspended-inspect",
                RecipeCommand::Inspect { record },
            )
            .await
            .data
            .unwrap();
            assert_eq!(
                inspected["trust"]["state"]["status"],
                if missing_key {
                    "signing_unavailable"
                } else {
                    "permission_or_authority_withheld"
                }
            );
            let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_record")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
    });
}
