pub mod support;

use engine::{
    Engine,
    actions::Action,
    trust::Signer,
    trust_analysis::{IdentityAuthenticity, sign_contact_document},
};
use nucleus::identity::{Command, PersonKey};
use nucleus::trust_analysis::{
    Identity, IdentityContext,
    contacts::{Audience, CheckedDisclosure, Document, Statement, permitted_edges},
    paths::{Policy, search},
};
use std::collections::BTreeMap;

fn document(issuer: Identity, contact: Identity, signer: &Signer) -> Document {
    let now = nucleus::execution::now().timestamp();
    let mut document = Document {
        statement: Statement {
            issuer,
            contact,
            revision: 1,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: String::new(),
        public_key: String::new(),
        signature: String::new(),
    };
    sign_contact_document(&mut document, signer).unwrap();
    document
}

async fn recognize_person(reader: &Engine, person: &support::Person) {
    let owner = store::organs::local(&reader.store.pool)
        .await
        .unwrap()
        .unwrap();
    let mut tx = store::write_tx(&reader.store.pool).await.unwrap();
    store::records::create_with_uid_on(
        &mut tx,
        &person.uid,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Independently received Person profile",
            body: "",
            quantity: store::exact::integer(1),
        },
        &owner.uid,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let key = PersonKey {
        subject: person.uid.clone(),
        key_id: person.signer.key_id.clone(),
        public_key: person.signer.public_key_b64(),
    };
    let preview = reader
        .act(
            Action::Identity {
                request: Command::PreviewPersonKey { key: key.clone() },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["identity"]
        .clone();
    reader
        .act(
            Action::Identity {
                request: Command::AcceptPersonKey {
                    key,
                    expected_profile_revision: preview["profile_revision"].as_i64().unwrap(),
                    expected_review: serde_json::from_value(preview["review_hash"].clone())
                        .unwrap(),
                },
            },
            None,
        )
        .await
        .unwrap();
}

#[test]
fn person_disclosure_needs_independent_recognition_and_never_adopts_keys_or_executes_transfers() {
    support::run_async_test("contact-person-proof", || async {
        let source = support::engine().await;
        let reader = support::engine().await;
        let person = support::person(&source, "contact.person").await;
        let other = support::person(&reader, "contact.other").await;
        let proof = document(
            Identity::Person(person.uid.clone()),
            Identity::Person(other.uid.clone()),
            &person.signer,
        );
        let now = nucleus::execution::now().timestamp();
        let before: (i64, i64, i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&reader.store.pool).await.unwrap();
        assert!(
            reader
                .authenticate_contact_disclosure(&proof, now)
                .await
                .is_err()
        );
        let after: (i64, i64, i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&reader.store.pool).await.unwrap();
        assert_eq!(before, after);
        recognize_person(&reader, &person).await;
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&proof, now)
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
        let mut changed = proof.clone();
        changed.statement.contact = Identity::Organ(other.uid.clone());
        assert!(
            reader
                .authenticate_contact_disclosure(&changed, now)
                .await
                .is_err()
        );
        changed = proof.clone();
        changed.statement.revision += 1;
        assert!(
            reader
                .authenticate_contact_disclosure(&changed, now)
                .await
                .is_err()
        );
        changed = proof.clone();
        changed.statement.audience = Audience::default();
        assert!(
            reader
                .authenticate_contact_disclosure(&changed, now)
                .await
                .is_err()
        );
        changed = proof.clone();
        changed.signature = person
            .signer
            .sign_bytes(b"lince.transfer.receipt.signature.v1");
        assert!(
            reader
                .authenticate_contact_disclosure(&changed, now)
                .await
                .is_err()
        );
        let (key, signature) = reader.revocation_certificate(&person.signer);
        assert!(
            reader
                .adopt_revocation(&person.uid, &key, &signature)
                .await
                .unwrap()
        );
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&proof, now)
                .await
                .unwrap(),
            IdentityAuthenticity::WithheldAuthority
        );
        let counts: (i64, i64) = store::sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM transfer),(SELECT COUNT(*) FROM transfer_occurrence)",
        )
        .fetch_one(&reader.store.pool)
        .await
        .unwrap();
        assert_eq!(counts, (0, 0));
    });
}

#[test]
fn organ_contacts_require_owner_authority_and_do_not_inherit_person_or_receipt_authority() {
    support::run_async_test("contact-organ-proof", || async {
        let source = support::engine().await;
        let reader = support::engine().await;
        let organ = store::organs::local(&source.store.pool)
            .await
            .unwrap()
            .unwrap();
        let other = store::organs::local(&reader.store.pool)
            .await
            .unwrap()
            .unwrap();
        let root = Signer::from_bytes(&organ.uid, engine::roster::ROOT_KEY_ID, [91; 32]);
        source.publish_root_key(&root).await.unwrap();
        let proof = document(
            Identity::Organ(organ.uid.clone()),
            Identity::Organ(other.uid.clone()),
            &root,
        );
        let now = nucleus::execution::now().timestamp();
        assert!(
            reader
                .authenticate_contact_disclosure(&proof, now)
                .await
                .is_err()
        );
        reader
            .adopt_introduction(&source.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&proof, now)
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
        let operational =
            Signer::from_bytes(&organ.uid, "cell:unrelated-receipt-purpose", [92; 32]);
        engine::trust::adopt_key(
            &reader.store,
            &organ.uid,
            &operational.key_id,
            &operational.public_key_b64(),
        )
        .await
        .unwrap();
        let mut wrong = proof.clone();
        wrong.key_id = operational.key_id.clone();
        wrong.public_key = operational.public_key_b64();
        wrong.signature = operational.sign_bytes(&wrong.signing_bytes().unwrap());
        assert!(
            reader
                .authenticate_contact_disclosure(&wrong, now)
                .await
                .is_err()
        );
        wrong.signature.clear();
        assert!(sign_contact_document(&mut wrong, &operational).is_err());
        wrong = proof.clone();
        wrong.statement.issuer = Identity::Person(organ.uid.clone());
        wrong.signature.clear();
        sign_contact_document(&mut wrong, &root).unwrap();
        assert!(
            reader
                .authenticate_contact_disclosure(&wrong, now)
                .await
                .is_err()
        );
        let successor = Signer::from_bytes(&organ.uid, engine::roster::ROOT_KEY_ID, [93; 32]);
        source
            .sign_succession(&root, &successor.public_key_b64())
            .await
            .unwrap();
        let succession = source
            .published_successions(&organ.uid)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert!(
            reader
                .adopt_succession(
                    &organ.uid,
                    &succession.old_key,
                    &succession.new_key,
                    &succession.created_at,
                    &succession.signature
                )
                .await
                .unwrap()
        );
        let mut rotated = document(
            Identity::Organ(organ.uid.clone()),
            Identity::Organ(other.uid.clone()),
            &successor,
        );
        rotated.statement.revision = 2;
        rotated.signature.clear();
        sign_contact_document(&mut rotated, &successor).unwrap();
        let now = nucleus::execution::now().timestamp();
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&rotated, now)
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
        assert_eq!(
            engine::trust::key_of(&reader.store, &organ.uid, engine::roster::ROOT_KEY_ID)
                .await
                .unwrap(),
            Some(root.public_key_b64())
        );
        let (key, signature) = reader.revocation_certificate(&root);
        assert!(
            reader
                .adopt_revocation(&organ.uid, &key, &signature)
                .await
                .unwrap()
        );
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&rotated, now)
                .await
                .unwrap(),
            IdentityAuthenticity::WithheldAuthority
        );
        let unrelated = Signer::from_bytes(&organ.uid, engine::roster::ROOT_KEY_ID, [94; 32]);
        let (key, signature) = reader.revocation_certificate(&unrelated);
        assert!(
            reader
                .adopt_revocation(&organ.uid, &key, &signature)
                .await
                .unwrap()
        );
        let unrelated_proof = document(
            Identity::Organ(organ.uid.clone()),
            Identity::Organ(other.uid),
            &unrelated,
        );
        assert!(matches!(
            reader
                .authenticate_contact_disclosure(
                    &unrelated_proof,
                    nucleus::execution::now().timestamp()
                )
                .await,
            Err(engine::EngineError::Conflict {
                code: "contact_disclosure_unknown_identity",
                ..
            })
        ));
    });
}

#[test]
fn four_lince_signed_contacts_support_three_hops_and_withdrawal_without_contact_list_export() {
    support::run_async_test("four-lince-contact-proofs", || async {
        let mut nodes = Vec::new();
        let mut signers = Vec::new();
        let mut identities = Vec::new();
        for seed in 101..105 {
            let node = support::engine().await;
            let organ = store::organs::local(&node.store.pool)
                .await
                .unwrap()
                .unwrap();
            let root = Signer::from_bytes(&organ.uid, engine::roster::ROOT_KEY_ID, [seed; 32]);
            node.publish_root_key(&root).await.unwrap();
            identities.push(Identity::Organ(organ.uid));
            signers.push(root);
            nodes.push(node);
        }
        let reader = &nodes[3];
        for node in &nodes[..3] {
            reader
                .adopt_introduction(&node.introduction().await.unwrap(), 1)
                .await
                .unwrap();
        }
        let mut documents = Vec::new();
        for (left, right) in [(0, 1), (1, 2), (2, 3)] {
            documents.push(document(
                identities[left].clone(),
                identities[right].clone(),
                &signers[left],
            ));
            documents.push(document(
                identities[right].clone(),
                identities[left].clone(),
                &signers[right],
            ));
        }
        let now = nucleus::execution::now().timestamp();
        let mut checked = Vec::new();
        for document in documents {
            assert_eq!(
                reader
                    .authenticate_contact_disclosure(&document, now)
                    .await
                    .unwrap(),
                IdentityAuthenticity::Accepted
            );
            checked.push(CheckedDisclosure {
                document,
                readable: true,
                authority_current: true,
            });
        }
        let contexts: BTreeMap<_, _> = identities
            .iter()
            .cloned()
            .map(|identity| {
                (
                    identity,
                    IdentityContext {
                        readable: true,
                        known: true,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let policy = Policy::for_anchor(identities[3].clone());
        let edges = permitted_edges(&checked, &contexts, &identities[3], now).unwrap();
        let found = search(&edges, &contexts, &policy).unwrap();
        let path = found
            .paths
            .iter()
            .find(|path| path.identities.last() == Some(&identities[0]))
            .unwrap();
        assert_eq!(path.distance(), 3);
        assert_eq!(
            path.identities,
            identities.iter().rev().cloned().collect::<Vec<_>>()
        );
        let mut withdrawal = checked[2].document.clone();
        withdrawal.statement.revision = 2;
        withdrawal.statement.withdrawn = true;
        withdrawal.signature.clear();
        sign_contact_document(&mut withdrawal, &signers[1]).unwrap();
        assert_eq!(
            reader
                .authenticate_contact_disclosure(&withdrawal, now)
                .await
                .unwrap(),
            IdentityAuthenticity::Accepted
        );
        checked.push(CheckedDisclosure {
            document: withdrawal,
            readable: true,
            authority_current: true,
        });
        let edges = permitted_edges(&checked, &contexts, &identities[3], now).unwrap();
        let found = search(&edges, &contexts, &policy).unwrap();
        assert_eq!(found.paths.len(), 1);
        assert_eq!(
            found.paths[0].identities,
            vec![identities[3].clone(), identities[2].clone()]
        );
        assert!(checked.iter().all(|proof| {
            let bytes = serde_json::to_string(&proof.document).unwrap();
            !bytes.contains("proximity")
                && !bytes.contains("sync_in")
                && !bytes.contains("sync_out")
        }));
    });
}
