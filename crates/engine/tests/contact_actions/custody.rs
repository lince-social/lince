use super::*;
#[path = "custody/forwarding.rs"]
mod forwarding;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use nucleus::{
    social::PublicRequest,
    trust_analysis::{
        Condition, Filter,
        custody::{Acknowledgment, Command as Custody, Offer, Policy},
        serving::Artifact,
    },
};

struct Fixture {
    receiver: Engine,
    owner: support::Person,
    author: support::Person,
    sender: String,
    sender_root: engine::trust::Signer,
    receiver_organ: String,
    receiver_root: engine::trust::Signer,
    receiver_endpoint: String,
    sender_endpoint: String,
    artifact: Artifact,
    now: i64,
}

async fn fixture(recognized: bool) -> Fixture {
    let receiver = support::engine().await;
    let source = support::engine().await;
    let owner = support::person(&receiver, "custody-operator").await;
    let author = support::person(&source, "public-original-author").await;
    authorize(&receiver, &owner).await;
    authorize(&source, &author).await;
    let role = store::auth::ensure_role(&receiver.store.pool, "Contact evidence owner")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&receiver.store.pool, "organ", "update")
        .await
        .unwrap();
    store::auth::grant(&receiver.store.pool, role, permission)
        .await
        .unwrap();
    let (sender, sender_endpoint, sender_root) =
        super::serving::peer(&receiver, &source, &owner, 121).await;
    let (receiver_organ, receiver_endpoint, receiver_root) =
        super::serving::peer(&source, &receiver, &author, 122).await;
    recognize(&source, &owner).await;
    if recognized {
        recognize(&receiver, &author).await;
    }
    let prepared = preview(
        &source,
        &author,
        draft(&author, &owner, 0, false),
        "custody:original-preview",
    )
    .await;
    let record = publish(&source, &author, &prepared, "custody:original-publish").await;
    let document = export(&source, &author, &record, "custody:original-export").await;
    let artifact = Artifact::Contact(serde_json::from_str(&document).unwrap());
    let now = nucleus::execution::now().timestamp();
    let fixture = Fixture {
        receiver,
        owner,
        author,
        sender,
        sender_root,
        receiver_organ,
        receiver_root,
        receiver_endpoint,
        sender_endpoint,
        artifact,
        now,
    };
    fixture.set_policy(true, 0).await;
    fixture
}

fn all_known() -> Filter {
    Filter {
        include: vec![Condition {
            known: Some(true),
            ..Default::default()
        }],
        exclude: vec![],
    }
}

impl Fixture {
    async fn set_policy(&self, enabled: bool, expected_version: u64) {
        let policy = Policy {
            enabled,
            senders: all_known(),
            readers: all_known(),
            ..Default::default()
        };
        self.receiver
            .act(
                action(
                    &self.owner,
                    &format!("custody:policy-{expected_version}"),
                    Command::Custody {
                        request: Custody::SetPolicy {
                            policy,
                            expected_version,
                        },
                    },
                ),
                Some(self.owner.uid.clone()),
            )
            .await
            .unwrap();
    }
    fn offer(&self, artifact: &Artifact) -> Offer {
        let mut offer = Offer {
            nonce: nucleus::new_uid("cust"),
            sender: self.sender.clone(),
            custodian: self.receiver_organ.clone(),
            artifact: artifact.hash().unwrap(),
            source_kind: artifact.kind(),
            bytes: artifact.bytes().unwrap().len() as u32,
            retained_until: self.now + 1800,
            deliver_by: self.now + 300,
            readers: all_known(),
            redistribute: true,
            key_id: self.sender_root.key_id.clone(),
            public_key: self.sender_root.public_key_b64(),
            signature: String::new(),
        };
        offer.signature = self.sender_root.sign_bytes(&offer.signing_bytes().unwrap());
        offer
    }
    async fn wire(&self, request: PublicRequest) -> Result<Value, engine::EngineError> {
        self.receiver
            .social_public_frame(
                &self.sender_endpoint,
                &self.receiver_endpoint,
                &serde_json::to_vec(&request).unwrap(),
                self.now,
            )
            .await
    }
    async fn begin(&self, offer: &Offer) -> String {
        self.wire(PublicRequest::CustodyBegin {
            offer: Box::new(offer.clone()),
        })
        .await
        .unwrap()["upload"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn send(&self, id: &str, artifact: &Artifact) {
        for (index, chunk) in artifact
            .bytes()
            .unwrap()
            .chunks(nucleus::trust_analysis::serving::CHUNK_BYTES)
            .enumerate()
        {
            self.wire(PublicRequest::CustodyChunk {
                upload: id.into(),
                offset: (index * nucleus::trust_analysis::serving::CHUNK_BYTES) as u32,
                bytes: STANDARD.encode(chunk),
            })
            .await
            .unwrap();
        }
    }
    async fn count(&self, table: &str) -> i64 {
        store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&self.receiver.store.pool)
            .await
            .unwrap()
    }
}

#[test]
fn custody_wire_commits_one_authenticated_original_and_signature_bound_ack_without_accounting_credit()
 {
    support::run_async_test("custody-wire", || async {
        let f = fixture(true).await;
        let offer = f.offer(&f.artifact);
        let incomplete = f.begin(&offer).await;
        assert!(
            f.wire(PublicRequest::CustodyFinish { upload: incomplete })
                .await
                .is_err()
        );
        assert_eq!(f.count("evidence_custody_commitment").await, 0);
        let id = f.begin(&offer).await;
        f.send(&id, &f.artifact).await;
        let duplicate = f
            .wire(PublicRequest::CustodyChunk {
                upload: id.clone(),
                offset: 0,
                bytes: STANDARD.encode(f.artifact.bytes().unwrap()),
            })
            .await
            .unwrap();
        assert_eq!(duplicate["offset"], offer.bytes);
        let completed = f
            .wire(PublicRequest::CustodyFinish { upload: id })
            .await
            .unwrap();
        let ack: Acknowledgment =
            serde_json::from_value(completed["acknowledgment"].clone()).unwrap();
        assert!(ack.matches(&offer));
        let public: [u8; 32] = STANDARD
            .decode(&ack.public_key)
            .unwrap()
            .try_into()
            .unwrap();
        let signature =
            ed25519_dalek::Signature::from_slice(&STANDARD.decode(&ack.signature).unwrap())
                .unwrap();
        ed25519_dalek::VerifyingKey::from_bytes(&public)
            .unwrap()
            .verify_strict(&ack.signing_bytes().unwrap(), &signature)
            .unwrap();
        let facts = f.count("fact").await;
        let replay = f
            .wire(PublicRequest::CustodyBegin {
                offer: Box::new(offer.clone()),
            })
            .await
            .unwrap();
        assert_eq!(replay["acknowledgment"], completed["acknowledgment"]);
        assert_eq!(f.count("fact").await, facts);
        assert_eq!(f.count("evidence_custody_payload").await, 1);
        assert_eq!(f.count("transfer_receipt_record").await, 0);
        let fetched = f
            .wire(PublicRequest::FetchEvidence {
                hash: offer.artifact.clone(),
                offset: 0,
            })
            .await
            .unwrap();
        assert_eq!(
            STANDARD
                .decode(fetched["chunk"]["bytes"].as_str().unwrap())
                .unwrap(),
            f.artifact.bytes().unwrap()
        );
        assert!(fetched.get("offer").is_none());
        assert!(fetched.get("owner_person_uid").is_none());
        f.set_policy(false, 1).await;
        assert!(
            f.wire(PublicRequest::FetchEvidence {
                hash: offer.artifact,
                offset: 0
            })
            .await
            .is_err()
        );
        assert_eq!(f.count("evidence_custody_payload").await, 1);
    });
}

#[test]
fn custody_rejects_unknown_signers_private_originals_missing_redistribution_and_changed_policy() {
    support::run_async_test("custody-authorization", || async {
        for variant in 0..4 {
            let f = fixture(variant != 0).await;
            let mut artifact = f.artifact.clone();
            if variant == 1 {
                if let Artifact::Contact(document) = &mut artifact {
                    document.statement.audience = Audience::Selected {
                        viewers: vec![Identity::Person(f.author.uid.clone())],
                    };
                    engine::trust_analysis::sign_contact_document(document, &f.author.signer)
                        .unwrap();
                }
            }
            let mut offer = f.offer(&artifact);
            if variant == 2 {
                offer.redistribute = false;
                offer.signature = f.sender_root.sign_bytes(&offer.signing_bytes().unwrap());
                assert!(
                    f.wire(PublicRequest::CustodyBegin {
                        offer: Box::new(offer)
                    })
                    .await
                    .is_err()
                );
            } else {
                let id = f.begin(&offer).await;
                f.send(&id, &artifact).await;
                if variant == 3 {
                    f.set_policy(false, 1).await;
                }
                assert!(
                    f.wire(PublicRequest::CustodyFinish { upload: id })
                        .await
                        .is_err()
                );
            }
            assert_eq!(f.count("evidence_custody_payload").await, 0);
            assert_eq!(f.count("evidence_custody_commitment").await, 0);
            if variant == 0 {
                let known: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM identity_key WHERE actor_uid=?)",
                )
                .bind(&f.author.uid)
                .fetch_one(&f.receiver.store.pool)
                .await
                .unwrap();
                assert!(!known);
            }
        }
    });
}

#[test]
fn custody_uploads_bound_slots_chunks_hashes_cancel_and_source_identity() {
    support::run_async_test("custody-upload-bounds", || async {
        let f = fixture(true).await;
        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(f.begin(&f.offer(&f.artifact)).await);
        }
        assert!(
            f.wire(PublicRequest::CustodyBegin {
                offer: Box::new(f.offer(&f.artifact))
            })
            .await
            .is_err()
        );
        let id = &ids[0];
        assert!(
            f.wire(PublicRequest::CustodyChunk {
                upload: id.clone(),
                offset: 1,
                bytes: STANDARD.encode(b"gap")
            })
            .await
            .is_err()
        );
        assert!(
            f.wire(PublicRequest::CustodyChunk {
                upload: id.clone(),
                offset: 0,
                bytes: STANDARD.encode(vec![0; nucleus::trust_analysis::serving::CHUNK_BYTES + 1])
            })
            .await
            .is_err()
        );
        let other = iroh::SecretKey::from_bytes(&[123; 32]).public().to_string();
        assert!(
            f.receiver
                .social_public_frame(
                    &other,
                    &f.receiver_endpoint,
                    &serde_json::to_vec(&PublicRequest::CustodyChunk {
                        upload: id.clone(),
                        offset: 0,
                        bytes: STANDARD.encode(b"wrong source")
                    })
                    .unwrap(),
                    f.now
                )
                .await
                .is_err()
        );
        for id in ids {
            f.wire(PublicRequest::CustodyCancel { upload: id })
                .await
                .unwrap();
        }
        let mut wrong = f.offer(&f.artifact);
        wrong.artifact =
            nucleus::karma::canonical_hash("lince.test.wrong.original.v1", &"wrong").unwrap();
        wrong.signature = f.sender_root.sign_bytes(&wrong.signing_bytes().unwrap());
        let id = f.begin(&wrong).await;
        f.send(&id, &f.artifact).await;
        assert!(
            f.wire(PublicRequest::CustodyFinish { upload: id })
                .await
                .is_err()
        );
        assert_eq!(f.count("evidence_custody_commitment").await, 0);
        #[cfg(feature = "trust-test-hooks")]
        {
            let expired = f.begin(&f.offer(&f.artifact)).await;
            f.receiver.expire_custody_uploads_for_test().await;
            assert!(
                f.wire(PublicRequest::CustodyChunk {
                    upload: expired,
                    offset: 0,
                    bytes: STANDARD.encode(b"late")
                })
                .await
                .is_err()
            );
            let replacement = f.begin(&f.offer(&f.artifact)).await;
            f.wire(PublicRequest::CustodyCancel {
                upload: replacement,
            })
            .await
            .unwrap();
        }
    });
}
