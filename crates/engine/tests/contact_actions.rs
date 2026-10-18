#[path = "contact_actions/claims.rs"]
mod claims;
#[path = "contact_actions/privacy.rs"]
mod privacy;
#[path = "contact_actions/recipes.rs"]
mod recipes;
#[path = "contact_actions/snapshots.rs"]
mod snapshots;
#[path = "contact_actions/serving.rs"]
mod serving;
#[path = "contact_actions/discovery.rs"]
mod discovery;
#[path = "contact_actions/remote.rs"]
mod remote;
#[path = "contact_actions/rolling.rs"]
mod rolling;
#[path = "contact_actions/custody.rs"]
mod custody;
pub mod support;
use engine::{Engine, actions::Action};
use nucleus::trust_analysis::{
    Identity,
    commands::{Command, ContactDraft, ContactReview},
    contacts::Audience,
    paths,
};
use serde_json::{Value, json};

fn action(person: &support::Person, id: &str, request: Command) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: id.into(),
        request,
    }
}

async fn authorize(node: &Engine, person: &support::Person) {
    let role = store::auth::ensure_role(&node.store.pool, "Contact evidence owner")
        .await
        .unwrap();
    for operation in ["read", "create", "update"] {
        let permission = store::auth::ensure_permission(&node.store.pool, "record", operation)
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&node.store.pool, &person.uid, &person.uid, "fixture", role)
        .await
        .unwrap();
    node.set_signer(person.signer.clone()).await.unwrap();
}

async fn recognize(node: &Engine, person: &support::Person) {
    let organ = store::organs::local(&node.store.pool)
        .await
        .unwrap()
        .unwrap();
    let mut tx = store::write_tx(&node.store.pool).await.unwrap();
    store::records::create_with_uid_on(
        &mut tx,
        &person.uid,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Independently received profile",
            body: "",
            quantity: store::exact::integer(1),
        },
        &organ.uid,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let key = nucleus::identity::PersonKey {
        subject: person.uid.clone(),
        key_id: person.signer.key_id.clone(),
        public_key: person.signer.public_key_b64(),
    };
    let preview = node
        .act(
            Action::Identity {
                request: nucleus::identity::Command::PreviewPersonKey { key: key.clone() },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["identity"]
        .clone();
    node.act(
        Action::Identity {
            request: nucleus::identity::Command::AcceptPersonKey {
                key,
                expected_profile_revision: preview["profile_revision"].as_i64().unwrap(),
                expected_review: serde_json::from_value(preview["review_hash"].clone()).unwrap(),
            },
        },
        None,
    )
    .await
    .unwrap();
}

fn draft(
    person: &support::Person,
    contact: &support::Person,
    revision: u64,
    withdrawn: bool,
) -> ContactDraft {
    ContactDraft {
        issuer: Identity::Person(person.uid.clone()),
        contact: Identity::Person(contact.uid.clone()),
        audience: Audience::Public,
        lifetime_seconds: 3600,
        expected_revision: revision,
        withdrawn,
        key_id: String::new(),
        public_key: String::new(),
    }
}

async fn preview(node: &Engine, person: &support::Person, draft: ContactDraft, id: &str) -> Value {
    node.set_signer(person.signer.clone()).await.unwrap();
    node.act(
        action(person, id, Command::PreviewContact { draft }),
        Some(person.uid.clone()),
    )
    .await
    .unwrap()
    .data
    .unwrap()["trust"]
        .clone()
}

async fn publish(node: &Engine, person: &support::Person, preview: &Value, id: &str) -> String {
    let mut command = action(
        person,
        id,
        Command::PublishContact {
            review: serde_json::from_value(preview["review"].clone()).unwrap(),
            expected_review: serde_json::from_value(preview["hash"].clone()).unwrap(),
        },
    );
    engine::trust_analysis::prepare_contact_action(&mut command, &person.signer).unwrap();
    node.act(command, Some(person.uid.clone()))
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn export(node: &Engine, person: &support::Person, record: &str, id: &str) -> String {
    node.act(
        action(
            person,
            id,
            Command::ExportContact {
                record: record.into(),
                as_organ: false,
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
        .to_owned()
}

#[test]
fn reviewed_contact_actions_reject_edits_stale_profiles_and_wrong_roles_without_effects() {
    support::run_async_test("reviewed-contact-actions", || async {
        let node = support::engine().await;
        let a = support::person(&node, "contact.action.a").await;
        let b = support::person(&node, "contact.action.b").await;
        authorize(&node, &a).await;
        authorize(&node, &b).await;
        node.set_signer(a.signer.clone()).await.unwrap();
        let watch = node.watch_query_changes();
        let first = preview(&node, &a, draft(&a, &b, 0, false), "contact:preview").await;
        assert!(!watch.has_changed().unwrap());
        let mut edited: ContactReview = serde_json::from_value(first["review"].clone()).unwrap();
        edited.document.statement.audience = Audience::default();
        assert!(
            node.act(
                action(
                    &a,
                    "contact:changed",
                    Command::PublishContact {
                        review: edited,
                        expected_review: serde_json::from_value(first["hash"].clone()).unwrap()
                    }
                ),
                Some(a.uid.clone())
            )
            .await
            .is_err()
        );
        node.act(
            Action::EditRecordText {
                target: b.uid.clone(),
                head: Some("Changed profile".into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
        assert!(
            node.act(
                action(
                    &a,
                    "contact:stale-profile",
                    Command::PublishContact {
                        review: serde_json::from_value(first["review"].clone()).unwrap(),
                        expected_review: serde_json::from_value(first["hash"].clone()).unwrap()
                    }
                ),
                Some(a.uid.clone())
            )
            .await
            .is_err()
        );
        let mut other = draft(&a, &b, 0, false);
        other.issuer = Identity::Person(b.uid.clone());
        other.contact = Identity::Person(a.uid.clone());
        assert!(
            node.act(
                action(
                    &a,
                    "contact:wrong-issuer",
                    Command::PreviewContact { draft: other }
                ),
                Some(a.uid.clone())
            )
            .await
            .is_err()
        );
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM contact_disclosure_record")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let fresh = preview(&node, &a, draft(&a, &b, 0, false), "contact:fresh").await;
        let record = publish(&node, &a, &fresh, "contact:publish").await;
        let bytes = export(&node, &a, &record, "contact:export").await;
        assert!(bytes.contains(&a.uid));
        assert!(
            node.act(
                action(
                    &b,
                    "contact:other-reader",
                    Command::InspectContact {
                        record: record.clone(),
                        as_organ: false
                    }
                ),
                Some(b.uid.clone())
            )
            .await
            .is_err()
        );
        assert!(
            store::records::get(&node.store.pool, &record)
                .await
                .unwrap()
                .unwrap()
                .quantity
                .is_zero()
        );
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        let repeated = publish(&node, &a, &fresh, "contact:publish").await;
        assert_eq!(repeated, record);
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        let mut stale = draft(&a, &b, 0, true);
        stale.lifetime_seconds = 3600;
        assert!(
            node.act(
                action(
                    &a,
                    "contact:old-revision",
                    Command::PreviewContact { draft: stale }
                ),
                Some(a.uid.clone())
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn four_lince_actions_publish_import_search_and_withdraw_without_resource_effects() {
    support::run_async_test("four-lince-contact-actions", || async {
        let mut nodes = Vec::new();
        let mut persons = Vec::new();
        for index in 0..4 {
            let node = support::engine().await;
            let person = support::person(&node, &format!("contact.workflow.{index}")).await;
            authorize(&node, &person).await;
            nodes.push(node);
            persons.push(person);
        }
        for (index, node) in nodes.iter().enumerate() {
            for (other, person) in persons.iter().enumerate() {
                if index != other {
                    recognize(node, person).await;
                }
            }
        }
        let reader = &nodes[3];
        let viewer = &persons[3];
        let mut original = None;
        for (source, target) in [(0, 1), (1, 0), (1, 2), (2, 1), (2, 3), (3, 2)] {
            let prefix = format!("contact:{source}:{target}");
            let reviewed = preview(
                &nodes[source],
                &persons[source],
                draft(&persons[source], &persons[target], 0, false),
                &format!("{prefix}:preview"),
            )
            .await;
            let record = publish(
                &nodes[source],
                &persons[source],
                &reviewed,
                &format!("{prefix}:publish"),
            )
            .await;
            let bytes = export(
                &nodes[source],
                &persons[source],
                &record,
                &format!("{prefix}:export"),
            )
            .await;
            reader.set_signer(viewer.signer.clone()).await.unwrap();
            reader
                .act(
                    action(
                        viewer,
                        &format!("{prefix}:import"),
                        Command::ImportContact {
                            document: bytes.clone(),
                            as_organ: false,
                        },
                    ),
                    Some(viewer.uid.clone()),
                )
                .await
                .unwrap();
            if (source, target) == (1, 2) {
                original = Some(bytes);
            }
        }
        let policy = paths::Policy::for_anchor(Identity::Person(viewer.uid.clone()));
        let result = reader
            .act(
                action(
                    viewer,
                    "contact:paths",
                    Command::ContactPaths {
                        policy: policy.clone(),
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        let paths = result["result"]["paths"].as_array().unwrap();
        assert_eq!(paths.len(), 3);
        assert_eq!(result["proof_records"].as_object().unwrap().len(), 6);
        assert!(
            paths
                .iter()
                .any(|path| path["identities"].as_array().unwrap().len() == 4)
        );
        let mut excluded = policy.clone();
        excluded.exclude_everywhere = vec![Identity::Person(persons[1].uid.clone())];
        let filtered = reader
            .act(
                action(
                    viewer,
                    "contact:filtered",
                    Command::ContactPaths {
                        policy: excluded,
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        assert!(!filtered.to_string().contains(&persons[1].uid));
        let withdrawn = preview(
            &nodes[1],
            &persons[1],
            draft(&persons[1], &persons[2], 1, true),
            "contact:withdraw:preview",
        )
        .await;
        let record = publish(
            &nodes[1],
            &persons[1],
            &withdrawn,
            "contact:withdraw:publish",
        )
        .await;
        let bytes = export(&nodes[1], &persons[1], &record, "contact:withdraw:export").await;
        reader.set_signer(viewer.signer.clone()).await.unwrap();
        reader
            .act(
                action(
                    viewer,
                    "contact:withdraw:import",
                    Command::ImportContact {
                        document: bytes,
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap();
        reader
            .act(
                action(
                    viewer,
                    "contact:old:import",
                    Command::ImportContact {
                        document: original.unwrap(),
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap();
        let result = reader
            .act(
                action(
                    viewer,
                    "contact:after-withdraw",
                    Command::ContactPaths {
                        policy,
                        as_organ: false,
                    },
                ),
                Some(viewer.uid.clone()),
            )
            .await
            .unwrap()
            .data
            .unwrap()["trust"]
            .clone();
        assert_eq!(result["result"]["paths"].as_array().unwrap().len(), 1);
        for node in &nodes {
            let counts: (i64, i64) = store::sqlx::query_as(
                "SELECT (SELECT COUNT(*) FROM transfer),(SELECT COUNT(*) FROM transfer_occurrence)",
            )
            .fetch_one(&node.store.pool)
            .await
            .unwrap();
            assert_eq!(counts, (0, 0));
        }
    });
}

#[test]
fn organ_audiences_require_explicit_administration_and_imports_do_not_adopt_identities() {
    support::run_async_test("contact-organ-audiences", || async {
        let source = support::engine().await;
        let reader = support::engine().await;
        let issuer = support::person(&source, "contact.issuer").await;
        let recipient = support::person(&reader, "contact.recipient").await;
        authorize(&source, &issuer).await;
        authorize(&reader, &recipient).await;
        let organ = store::organs::local(&reader.store.pool)
            .await
            .unwrap()
            .unwrap();
        recognize(&source, &recipient).await;
        let mut selected = draft(&issuer, &recipient, 0, false);
        selected.audience = Audience::Selected {
            viewers: vec![Identity::Organ(organ.uid)],
        };
        let reviewed = preview(&source, &issuer, selected, "contact:organ:preview").await;
        let record = publish(&source, &issuer, &reviewed, "contact:organ:publish").await;
        let saved = store::contact_disclosures::get(&source.store.pool, &record)
            .await
            .unwrap();
        let bytes = serde_json::to_string(&saved.document).unwrap();
        let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_key")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert!(
            reader
                .act(
                    action(
                        &recipient,
                        "contact:unknown",
                        Command::ImportContact {
                            document: bytes.clone(),
                            as_organ: true
                        }
                    ),
                    None
                )
                .await
                .is_err()
        );
        let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_key")
            .fetch_one(&reader.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
        recognize(&reader, &issuer).await;
        assert!(
            reader
                .act(
                    action(
                        &recipient,
                        "contact:person-only",
                        Command::ImportContact {
                            document: bytes.clone(),
                            as_organ: false
                        }
                    ),
                    Some(recipient.uid.clone())
                )
                .await
                .is_err()
        );
        assert!(
            reader
                .act(
                    action(
                        &recipient,
                        "contact:not-admin",
                        Command::ImportContact {
                            document: bytes.clone(),
                            as_organ: true
                        }
                    ),
                    Some(recipient.uid.clone())
                )
                .await
                .is_err()
        );
        let imported = reader
            .act(
                action(
                    &recipient,
                    "contact:explicit-admin",
                    Command::ImportContact {
                        document: bytes,
                        as_organ: true,
                    },
                ),
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        assert!(
            reader
                .act(
                    action(
                        &recipient,
                        "contact:person-inspect",
                        Command::InspectContact {
                            record: imported.clone(),
                            as_organ: false
                        }
                    ),
                    Some(recipient.uid.clone())
                )
                .await
                .is_err()
        );
        let inspected = reader
            .act(
                action(
                    &recipient,
                    "contact:admin-inspect",
                    Command::InspectContact {
                        record: imported,
                        as_organ: true,
                    },
                ),
                None,
            )
            .await
            .unwrap();
        assert_eq!(inspected.data.unwrap()["trust"]["authority_current"], true);
        assert!(
            reader
                .act(
                    action(
                        &recipient,
                        "contact:instruction",
                        Command::ImportContact {
                            document: json!({"action":"set-quantity","quantity":1000}).to_string(),
                            as_organ: false
                        }
                    ),
                    Some(recipient.uid.clone())
                )
                .await
                .is_err()
        );
    });
}
