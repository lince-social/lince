use super::serving::{all, approve, peer, review};
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{EngineError, social::Network};
use nucleus::{
    social::{Command as Social, PublicRequest, ask::ContactConsent},
    trust_analysis::{
        Condition,
        remote::{self as model, Command as Request},
        serving::{Artifact, Chunk, SourceKind},
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};

struct ReaderNetwork {
    source: String,
    peers: Mutex<BTreeMap<String, Arc<Engine>>>,
    traffic: Mutex<Vec<Value>>,
    mode: AtomicU8,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    active: AtomicUsize,
    high: AtomicUsize,
    forged: Mutex<Option<Artifact>>,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl Network for ReaderNetwork {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        self.traffic
            .lock()
            .unwrap()
            .push(json!({"destination":destination,"message":request}));
        let count = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.high.fetch_max(count, Ordering::SeqCst);
        let _active = Active(&self.active);
        let mode = self.mode.load(Ordering::SeqCst);
        if mode == 1 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        let forged = self.forged.lock().unwrap().clone();
        let mut value = if mode == 3 {
            let artifact = forged.unwrap();
            let hash = artifact.hash().unwrap();
            let bytes = artifact.bytes().unwrap();
            match request {
                PublicRequest::DiscoverEvidence { document } => {
                    json!({"service":destination,"page":nucleus::trust_analysis::discovery::Page {id:document.id,question:document.question.hash().unwrap(),headers:if document.question.matches(&artifact){vec![nucleus::trust_analysis::discovery::Header {hash,kind:artifact.kind(),bytes:bytes.len() as u32,expires_at:artifact.expires_at().unwrap()}]}else{Vec::new()},next:None,partial:false}})
                }
                PublicRequest::FetchEvidence { offset, .. } => {
                    json!({"service":destination,"chunk":Chunk {hash,offset,total:bytes.len() as u32,bytes:STANDARD.encode(&bytes[offset as usize..bytes.len().min(offset as usize+65536)])}})
                }
                _ => panic!("Unexpected protocol"),
            }
        } else {
            let peer = self.peers.lock().unwrap()[destination].clone();
            peer.social_public_request(
                &self.source,
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await?
        };
        if mode == 2 {
            value["service"] = json!("another-endpoint");
        }
        if mode == 4 {
            value["padding"] = json!("x".repeat(20000));
        }
        Ok(value)
    }
}

struct Fixture {
    reader: Arc<Engine>,
    source: Arc<Engine>,
    viewer: support::Person,
    author: support::Person,
    contact: support::Person,
    network: Arc<ReaderNetwork>,
    draft: model::Draft,
    artifact: Artifact,
    reader_root: engine::trust::Signer,
}

async fn run(
    node: &Engine,
    person: &support::Person,
    request: Request,
    id: &str,
) -> Result<Value, EngineError> {
    Ok(node
        .act(
            action(person, id, Command::Remote { request }),
            Some(person.uid.clone()),
        )
        .await?
        .data
        .unwrap()["trust"]
        .clone())
}
async fn counts(node: &Engine) -> (i64, i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap()
}

impl Fixture {
    async fn new(recognize_key: bool) -> Self {
        let source = Arc::new(support::engine().await);
        let reader = Arc::new(support::engine().await);
        let author = support::person(&source, "remote.author").await;
        let contact = support::person(&source, "remote.contact").await;
        let viewer = support::person(&reader, "remote.viewer").await;
        authorize(&source, &author).await;
        authorize(&reader, &viewer).await;
        let (_, reader_endpoint, reader_root) = peer(&source, &reader, &author, 202).await;
        let (organ, endpoint, _) = peer(&reader, &source, &viewer, 201).await;
        reader
            .social_command(
                Social::SetAskContact {
                    choice: ContactConsent {
                        organ,
                        ask: true,
                        answer: false,
                        forward: false,
                    },
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap();
        if recognize_key {
            recognize(&reader, &author).await;
        } else {
            let local = store::organs::local(&reader.store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            let mut tx = store::write_tx(&reader.store.pool).await.unwrap();
            store::records::create_with_uid_on(
                &mut tx,
                &author.uid,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Person,
                    head: "Unverified contributor profile",
                    body: "",
                    quantity: store::exact::integer(1),
                },
                &local,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let document = preview(
            &source,
            &author,
            draft(&author, &contact, 0, false),
            "remote:source-preview",
        )
        .await;
        let record = publish(&source, &author, &document, "remote:source-publish").await;
        let grant = review(&source, &author, &record, SourceKind::Contact, all()).await;
        approve(&source, &author, &grant, "remote:source-grant")
            .await
            .unwrap();
        let artifact = serde_json::from_value::<nucleus::trust_analysis::serving::Review>(
            grant["review"].clone(),
        )
        .unwrap()
        .artifact;
        let network = Arc::new(ReaderNetwork {
            source: reader_endpoint,
            peers: Mutex::new(BTreeMap::from([(endpoint, source.clone())])),
            traffic: Mutex::new(Vec::new()),
            mode: AtomicU8::new(0),
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            active: AtomicUsize::new(0),
            high: AtomicUsize::new(0),
            forged: Mutex::new(None),
        });
        reader.attach_social_network(network.clone());
        let draft = model::Draft {
            query: nucleus::trust_analysis::query::Query::for_subject(Identity::Person(
                author.uid.clone(),
            )),
            as_organ: false,
            linked_claims: Vec::new(),
            include_contacts: true,
            receipt_sources: Vec::new(),
            contact_subjects: Vec::new(),
        };
        Self {
            reader,
            source,
            viewer,
            author,
            contact,
            network,
            draft,
            artifact,
            reader_root,
        }
    }
    async fn preview(&self) -> Value {
        run(
            &self.reader,
            &self.viewer,
            Request::Preview {
                draft: self.draft.clone(),
            },
            &nucleus::new_uid("remote-test"),
        )
        .await
        .unwrap()
    }
    async fn start(&self, review: &Value, id: &str) -> Result<Value, EngineError> {
        run(
            &self.reader,
            &self.viewer,
            Request::Start {
                review: Box::new(serde_json::from_value(review["review"].clone()).unwrap()),
                expected_review: serde_json::from_value(review["hash"].clone()).unwrap(),
            },
            id,
        )
        .await
    }
    async fn poll(&self, id: &str) -> Value {
        run(
            &self.reader,
            &self.viewer,
            Request::Poll { job: id.into() },
            &nucleus::new_uid("remote-test"),
        )
        .await
        .unwrap()
    }
    async fn add_proxy(&self) -> String {
        let proxy = Arc::new(support::engine().await);
        let owner = support::person(&proxy, "remote.proxy").await;
        authorize(&proxy, &owner).await;
        recognize(&proxy, &self.author).await;
        recognize(&proxy, &self.contact).await;
        let reader_organ = store::organs::local(&self.reader.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        store::organs::add_contact(&proxy.store.pool, &reader_organ, None, "Reader", "", 1)
            .await
            .unwrap();
        store::organs::set_trust(&proxy.store.pool, &reader_organ, "known")
            .await
            .unwrap();
        store::organs::set_node_id(&proxy.store.pool, &reader_organ, Some(&self.network.source))
            .await
            .unwrap();
        store::replica::make_own_root(&proxy.store.pool, &reader_organ)
            .await
            .unwrap();
        store::visibility::grant(&proxy.store.pool, "actor", Some(&owner.uid), &reader_organ)
            .await
            .unwrap();
        proxy.publish_root_key(&self.reader_root).await.unwrap();
        proxy
            .adopt_roster(&self.reader.roster_of(&reader_organ).await.unwrap().unwrap())
            .await
            .unwrap();
        for request in [
            Social::ConfigureAsk { enabled: true },
            Social::SetAskContact {
                choice: ContactConsent {
                    organ: reader_organ,
                    ask: false,
                    answer: true,
                    forward: false,
                },
            },
        ] {
            proxy
                .social_command(request, None, nucleus::execution::now())
                .await
                .unwrap();
        }
        let (organ, endpoint, _) = peer(&self.reader, &proxy, &self.viewer, 203).await;
        self.reader
            .social_command(
                Social::SetAskContact {
                    choice: ContactConsent {
                        organ: organ.clone(),
                        ask: true,
                        answer: false,
                        forward: false,
                    },
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap();
        let Artifact::Contact(document) = &self.artifact else {
            panic!("Contact fixture")
        };
        let record = proxy
            .act(
                action(
                    &owner,
                    "proxy:import",
                    Command::ImportContact {
                        document: serde_json::to_string(document).unwrap(),
                        as_organ: false,
                    },
                ),
                Some(owner.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let grant = review(&proxy, &owner, &record, SourceKind::Contact, all()).await;
        approve(&proxy, &owner, &grant, "proxy:grant")
            .await
            .unwrap();
        self.network.peers.lock().unwrap().insert(endpoint, proxy);
        organ
    }
}

#[test]
fn remote_queries_review_locally_keep_reader_policy_private_filter_endpoints_and_deduplicate_sources()
 {
    support::run_async_test("remote-private-dispatch", || async {
        let mut fixture = Fixture::new(true).await;
        let excluded = fixture.add_proxy().await;
        fixture.draft.query.endpoints.exclude.push(Condition {
            identities: vec![Identity::Organ(excluded.clone())],
            ..Default::default()
        });
        let before = counts(&fixture.reader).await;
        let review = fixture.preview().await;
        assert_eq!(review["review"]["endpoints"].as_array().unwrap().len(), 1);
        assert!(review["local"].is_object());
        assert!(fixture.network.traffic.lock().unwrap().is_empty());
        assert_eq!(
            fixture.reader.refresh_trust_requests_once().await.unwrap(),
            0
        );
        let started = fixture.start(&review, "remote:start").await.unwrap();
        let id = started["job"].as_str().unwrap();
        assert!(fixture.start(&review, "remote:start").await.unwrap()["replayed"] == true);
        fixture.reader.refresh_trust_requests_once().await.unwrap();
        let result = fixture.poll(id).await;
        assert_eq!(result["state"], "completed");
        assert_eq!(result["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(result["events"], 1);
        let traffic = serde_json::to_string(&*fixture.network.traffic.lock().unwrap()).unwrap();
        assert!(!traffic.contains(&fixture.viewer.uid));
        assert!(!traffic.contains("weights"));
        assert!(!traffic.contains("counterparties"));
        assert!(!traffic.contains(&excluded));
        fixture.draft.query.endpoints.exclude.clear();
        let review = fixture.preview().await;
        let started = fixture.start(&review, "remote:duplicates").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.reader.refresh_trust_requests_once().await.unwrap();
        let result = fixture.poll(id).await;
        assert_eq!(result["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(result["events"], 1);
        assert!(fixture.network.high.load(Ordering::SeqCst) <= 4);
        let opened = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Inspect {
                job: id.into(),
                artifact: fixture.artifact.hash().unwrap(),
            },
            "remote:open",
        )
        .await
        .unwrap();
        assert_eq!(opened["signature_state"], "accepted");
        assert_eq!(
            serde_json::from_value::<Artifact>(opened["artifact"].clone()).unwrap(),
            fixture.artifact
        );
        assert_eq!(counts(&fixture.reader).await, before);
    });
}

#[test]
fn direct_verified_experience_survives_optional_helper_enable_disable_outage_and_removal() {
    support::run_async_test("trust-without-helper", || async {
        let mut fixture = Fixture::new(true).await;
        fixture.add_proxy().await;
        recognize(&fixture.reader,&fixture.contact).await;
        let mut paths = nucleus::trust_analysis::paths::Policy::for_anchor(Identity::Person(fixture.author.uid.clone()));
        paths.allow_unilateral = true;
        fixture.draft.query.contacts = Some(paths);
        let mut helper = Some(support::engine().await);
        let (organ, endpoint, _) = peer(&fixture.reader,helper.as_ref().unwrap(),&fixture.viewer,204).await;
        let mut choice = nucleus::social::ServerChoice::institute_helper();
        choice.organ = Some(organ);
        choice.endpoint = endpoint.clone();
        let mut baseline = None;
        for (index, enabled) in [false,true,false,true,false].into_iter().enumerate() {
            if index == 3 {drop(helper.take());}
            choice.query = enabled;
            let command = if index == 4 {Social::RemoveServer {endpoint:endpoint.clone()}} else {Social::SaveServer {choice:choice.clone()}};
            fixture.reader.social_command(command,None,nucleus::execution::now()).await.unwrap();
            let before = counts(&fixture.reader).await;
            let review = fixture.preview().await;
            assert_eq!(review["review"]["endpoints"].as_array().unwrap().len(),2);
            let started = fixture.start(&review,&format!("helper-independent:{index}")).await.unwrap();
            let job = started["job"].as_str().unwrap();
            fixture.reader.refresh_trust_requests_once().await.unwrap();
            let result = run(&fixture.reader,&fixture.viewer,Request::Verify {job:job.into()},&format!("helper-verify:{index}")).await.unwrap();
            assert!(!result["paths"]["paths"].as_array().unwrap().is_empty());
            let semantic = json!({"analysis":result["analysis"],"paths":result["paths"]});
            if let Some(expected) = &baseline {assert_eq!(&semantic,expected);} else {baseline = Some(semantic);}
            assert_eq!(counts(&fixture.reader).await,before);
            assert!(fixture.network.traffic.lock().unwrap().iter().all(|packet|packet["destination"] != endpoint));
            run(&fixture.reader,&fixture.viewer,Request::Forget {job:job.into()},&format!("helper-forget:{index}")).await.unwrap();
        }
        let servers = fixture.reader.social_command(Social::Overview,None,nucleus::execution::now()).await.unwrap().data.unwrap();
        assert!(servers["servers"].as_array().unwrap().iter().all(|server|server["endpoint"] != endpoint));
    });
}

#[test]
fn remote_cancellation_consumes_an_unstarted_review_and_interrupts_waiting_io() {
    support::run_async_test("remote-cancellation", || async {
        let fixture = Fixture::new(true).await;
        let reviewed = fixture.preview().await;
        let nonce = reviewed["review"]["nonce"].as_str().unwrap();
        run(
            &fixture.reader,
            &fixture.viewer,
            Request::Cancel { job: nonce.into() },
            "remote:cancel-review",
        )
        .await
        .unwrap();
        assert!(
            fixture
                .start(&reviewed, "remote:cancelled-start")
                .await
                .is_err()
        );
        assert!(fixture.network.traffic.lock().unwrap().is_empty());
        let reviewed = fixture.preview().await;
        let started = fixture.start(&reviewed, "remote:waiting").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.network.mode.store(1, Ordering::SeqCst);
        let (work, _) = tokio::join!(fixture.reader.refresh_trust_requests_once(), async {
            fixture.network.entered.notified().await;
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Cancel { job: id.into() },
                "remote:stop",
            )
            .await
            .unwrap();
        });
        work.unwrap();
        let result = fixture.poll(id).await;
        assert_eq!(result["state"], "cancelled");
        assert_eq!(result["bytes"], 0);
        assert!(result["artifacts"].as_array().unwrap().is_empty());
        assert_eq!(fixture.network.active.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn remote_answers_reject_changed_consent_forged_signatures_wrong_endpoints_and_oversized_frames() {
    support::run_async_test("remote-hostile-replies", || async {
        let fixture = Fixture::new(true).await;
        for mode in [2, 3, 4] {
            let reviewed = fixture.preview().await;
            let started = fixture
                .start(&reviewed, &format!("remote:hostile:{mode}"))
                .await
                .unwrap();
            let id = started["job"].as_str().unwrap();
            let mut forged = fixture.artifact.clone();
            if let Artifact::Contact(document) = &mut forged {
                document.signature = STANDARD.encode([0u8; 64]);
            }
            *fixture.network.forged.lock().unwrap() = Some(forged);
            fixture.network.mode.store(mode, Ordering::SeqCst);
            fixture.reader.refresh_trust_requests_once().await.unwrap();
            let result = fixture.poll(id).await;
            assert_eq!(result["partial"], true);
            assert!(result["artifacts"].as_array().unwrap().is_empty());
        }
        fixture.network.mode.store(1, Ordering::SeqCst);
        let reviewed = fixture.preview().await;
        let started = fixture.start(&reviewed, "remote:consent").await.unwrap();
        let id = started["job"].as_str().unwrap();
        tokio::join!(fixture.reader.refresh_trust_requests_once(), async {
            fixture.network.entered.notified().await;
            let organ = store::organs::local(&fixture.source.store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            fixture
                .reader
                .social_command(
                    Social::SetAskContact {
                        choice: ContactConsent {
                            organ,
                            ask: false,
                            answer: false,
                            forward: false,
                        },
                    },
                    None,
                    nucleus::execution::now(),
                )
                .await
                .unwrap();
            fixture.network.release.notify_one();
        })
        .0
        .unwrap();
        assert!(
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Poll { job: id.into() },
                "remote:withheld"
            )
            .await
            .is_err()
        );
        let organ = store::organs::local(&fixture.source.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        fixture
            .reader
            .social_command(
                Social::SetAskContact {
                    choice: ContactConsent {
                        organ,
                        ask: true,
                        answer: false,
                        forward: false,
                    },
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap();
        let reviewed = fixture.preview().await;
        let started = fixture
            .start(&reviewed, "remote:device-removed")
            .await
            .unwrap();
        let id = started["job"].as_str().unwrap();
        tokio::join!(fixture.reader.refresh_trust_requests_once(), async {
            fixture.network.entered.notified().await;
            fixture
                .reader
                .publish_roster(&fixture.reader_root, Vec::new())
                .await
                .unwrap();
            fixture.network.release.notify_one();
        })
        .0
        .unwrap();
        assert!(
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Poll { job: id.into() },
                "remote:removed-device-progress"
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn remote_unknown_identity_stays_unadopted_and_other_readers_or_hidden_profiles_cannot_open_results()
 {
    support::run_async_test("remote-identity-readers", || async {
        let fixture = Fixture::new(false).await;
        let other = support::person(&fixture.reader, "remote.other").await;
        authorize(&fixture.reader, &other).await;
        fixture
            .reader
            .set_signer(fixture.viewer.signer.clone())
            .await
            .unwrap();
        let before = counts(&fixture.reader).await;
        let reviewed = fixture.preview().await;
        let started = fixture.start(&reviewed, "remote:unknown").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.reader.refresh_trust_requests_once().await.unwrap();
        let result = fixture.poll(id).await;
        assert_eq!(
            result["artifacts"][0]["signature_snapshot"],
            "unknown_identity"
        );
        let opened = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Inspect {
                job: id.into(),
                artifact: fixture.artifact.hash().unwrap(),
            },
            "remote:unknown-open",
        )
        .await
        .unwrap();
        assert_eq!(opened["signature_state"], "unknown_identity");
        let merged = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Verify { job: id.into() },
            "remote:unknown-merge",
        )
        .await
        .unwrap();
        assert!(
            merged["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["signature"] == "unknown_identity")
        );
        assert!(merged["paths"]["paths"].as_array().unwrap().is_empty());
        assert!(merged["analysis"]["groups"].as_array().unwrap().is_empty());
        assert_eq!(counts(&fixture.reader).await, before);
        assert!(
            run(
                &fixture.reader,
                &other,
                Request::Poll { job: id.into() },
                "remote:other-reader"
            )
            .await
            .is_err()
        );
        assert_eq!(counts(&fixture.reader).await, before);
        fixture
            .reader
            .set_read_filter(
                &fixture.viewer.uid,
                Some(&protein::Predicate::UidEq(fixture.viewer.uid.clone())),
            )
            .await
            .unwrap();
        assert!(run(&fixture.reader, &fixture.viewer, Request::Verify { job:id.into() }, "remote:hidden-combined-analysis").await.is_err());
        assert!(
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Inspect {
                    job: id.into(),
                    artifact: fixture.artifact.hash().unwrap()
                },
                "remote:hidden-profile"
            )
            .await
            .is_err()
        );
    });
}

#[cfg(feature = "trust-test-hooks")]
#[test]
fn remote_deadline_is_monotonic_polling_cannot_renew_it_and_restart_discards_reviews() {
    support::run_async_test("remote-deadline", || async {
        let fixture = Fixture::new(true).await;
        let reviewed = fixture.preview().await;
        let restarted = Engine::new(fixture.reader.store.clone()).await.unwrap();
        assert!(
            run(
                &restarted,
                &fixture.viewer,
                Request::Start {
                    review: Box::new(serde_json::from_value(reviewed["review"].clone()).unwrap()),
                    expected_review: serde_json::from_value(reviewed["hash"].clone()).unwrap()
                },
                "remote:restart"
            )
            .await
            .is_err()
        );
        let started = fixture.start(&reviewed, "remote:deadline").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.network.mode.store(1, Ordering::SeqCst);
        let (_, result) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(fixture.reader.refresh_trust_requests_once(), async {
                fixture.network.entered.notified().await;
                for _ in 0..3 { assert_eq!(fixture.poll(id).await["state"], "running"); }
                fixture.reader.expire_trust_query_for_test(id).await.unwrap();
                let result = fixture.poll(id).await;
                assert_eq!(result["state"], "expired");
                result
            })
        })
        .await
        .unwrap();
        assert_eq!(result["state"], "expired");
        assert_eq!(fixture.network.active.load(Ordering::SeqCst), 0);
        assert!(result["artifacts"].as_array().unwrap().is_empty());
        assert_eq!(fixture.network.traffic.lock().unwrap().len(), 1);
    });
}

#[cfg(feature = "trust-test-hooks")]
#[test]
fn combined_results_recheck_permissions_authority_and_job_state_after_arithmetic() {
    support::run_async_test("combined-finalization-races", || async {
        for variant in 0..6 {
            let fixture = Fixture::new(true).await;
            let reviewed = fixture.preview().await;
            let started = fixture.start(&reviewed, &format!("finalize:start-{variant}")).await.unwrap();
            let id = started["job"].as_str().unwrap().to_owned();
            fixture.reader.refresh_trust_requests_once().await.unwrap();
            let checkpoint = fixture.reader.pause_trust_verification_for_test(&id).await.unwrap();
            let reader = fixture.reader.clone();
            let person = fixture.viewer.clone();
            let job = id.clone();
            let task = tokio::spawn(async move {
                run(&reader, &person, Request::Verify { job }, "finalize:verify").await
            });
            tokio::time::timeout(Duration::from_secs(5), checkpoint.reached()).await.unwrap();
            match variant {
                0 => fixture.reader.set_read_filter(&fixture.viewer.uid, Some(&protein::Predicate::UidEq(fixture.viewer.uid.clone()))).await.unwrap(),
                1 => {
                    let (key, signature) = fixture.reader.revocation_certificate(&fixture.author.signer);
                    fixture.reader.adopt_revocation(&fixture.author.uid, &key, &signature).await.unwrap();
                }
                2 => { run(&fixture.reader, &fixture.viewer, Request::Cancel { job:id.clone() }, "finalize:cancel").await.unwrap(); }
                3 => { run(&fixture.reader, &fixture.viewer, Request::Forget { job:id.clone() }, "finalize:forget").await.unwrap(); }
                4 => store::people::deactivate(&fixture.reader.store.pool, &fixture.viewer.uid, &nucleus::execution::now().to_rfc3339(), Some("Finalization test")).await.unwrap(),
                _ => {
                    let organ = reviewed["review"]["endpoints"][0]["organ"].as_str().unwrap();
                    fixture.reader.social_command(Social::SetAskContact { choice:ContactConsent { organ:organ.into(), ask:false, answer:false, forward:false } }, None, nucleus::execution::now()).await.unwrap();
                }
            }
            let before = counts(&fixture.reader).await;
            checkpoint.release();
            assert!(tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap().is_err());
            assert_eq!(counts(&fixture.reader).await, before);
        }
    });
}

#[test]
fn merged_remote_contacts_use_local_paths_hide_newer_private_controls_and_never_import() {
    support::run_async_test("remote-merged-contact-controls", || async {
        let mut fixture = Fixture::new(true).await;
        fixture.add_proxy().await;
        recognize(&fixture.reader, &fixture.contact).await;
        recognize(&fixture.source, &fixture.viewer).await;
        authorize(&fixture.source, &fixture.contact).await;
        let reader_organ = store::organs::local(&fixture.reader.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        store::visibility::grant(
            &fixture.source.store.pool,
            "actor",
            Some(&fixture.contact.uid),
            &reader_organ,
        )
        .await
        .unwrap();
        let local = preview(
            &fixture.reader,
            &fixture.viewer,
            draft(&fixture.viewer, &fixture.author, 0, false),
            "merge:local-preview",
        )
        .await;
        publish(
            &fixture.reader,
            &fixture.viewer,
            &local,
            "merge:local-contact",
        )
        .await;
        for (person, contact, label) in [
            (&fixture.author, &fixture.viewer, "reader"),
            (&fixture.contact, &fixture.author, "contact"),
        ] {
            let reviewed = preview(
                &fixture.source,
                person,
                draft(person, contact, 0, false),
                &format!("merge:{label}:preview"),
            )
            .await;
            let record = publish(
                &fixture.source,
                person,
                &reviewed,
                &format!("merge:{label}:publish"),
            )
            .await;
            let approved =
                review(&fixture.source, person, &record, SourceKind::Contact, all()).await;
            approve(
                &fixture.source,
                person,
                &approved,
                &format!("merge:{label}:serve"),
            )
            .await
            .unwrap();
        }
        fixture.draft.query.contacts = Some(nucleus::trust_analysis::paths::Policy::for_anchor(
            Identity::Person(fixture.viewer.uid.clone()),
        ));
        let reviewed = fixture.preview().await;
        let started = fixture.start(&reviewed, "merge:start").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.reader.refresh_trust_requests_once().await.unwrap();
        let before = counts(&fixture.reader).await;
        let traffic = fixture.network.traffic.lock().unwrap().len();
        let merged = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Verify { job: id.into() },
            "merge:verify",
        )
        .await
        .unwrap();
        assert_eq!(merged["kind"], "remote_verified");
        let paths = merged["paths"]["paths"].as_array().unwrap();
        assert!(
            paths
                .iter()
                .any(|path| path["identities"].as_array().unwrap().len() == 3
                    && path["identities"][2]["uid"] == fixture.contact.uid),
            "{merged}"
        );
        assert_eq!(counts(&fixture.reader).await, before);
        assert_eq!(fixture.network.traffic.lock().unwrap().len(), traffic);
        assert!(
            merged["contact_subjects"]
                .as_array()
                .unwrap()
                .iter()
                .all(|identity| identity["uid"] != fixture.viewer.uid)
        );
        let withdrawn = preview(
            &fixture.source,
            &fixture.author,
            draft(&fixture.author, &fixture.contact, 1, true),
            "merge:withdraw-preview",
        )
        .await;
        let record = publish(
            &fixture.source,
            &fixture.author,
            &withdrawn,
            "merge:withdraw",
        )
        .await;
        let encoded = export(
            &fixture.source,
            &fixture.author,
            &record,
            "merge:withdraw-export",
        )
        .await;
        fixture
            .reader
            .set_signer(fixture.viewer.signer.clone())
            .await
            .unwrap();
        let hidden = fixture
            .reader
            .act(
                action(
                    &fixture.viewer,
                    "merge:retain-withdrawal",
                    Command::ImportContact {
                        document: encoded,
                        as_organ: false,
                    },
                ),
                Some(fixture.viewer.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture
            .reader
            .act(
                Action::DeleteRecord {
                    target: hidden.clone(),
                },
                None,
            )
            .await
            .unwrap();
        let before = counts(&fixture.reader).await;
        let merged = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Verify { job: id.into() },
            "merge:verify-withdrawal",
        )
        .await
        .unwrap();
        assert!(
            merged["paths"]["paths"]
                .as_array()
                .unwrap()
                .iter()
                .all(|path| !serde_json::to_string(path)
                    .unwrap()
                    .contains(&fixture.contact.uid))
        );
        assert!(!serde_json::to_string(&merged).unwrap().contains(&hidden));
        assert_eq!(counts(&fixture.reader).await, before);
        run(
            &fixture.reader,
            &fixture.viewer,
            Request::Cancel { job: id.into() },
            "merge:cancel",
        )
        .await
        .unwrap();
        assert!(
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Verify { job: id.into() },
                "merge:after-cancel"
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn merged_claims_recheck_hidden_newer_heads_and_context_questions_need_review() {
    support::run_async_test("remote-merged-claim-controls", || async {
        let mut fixture = Fixture::new(true).await;
        let first = super::claims::reviewed(
            &fixture.source,
            &fixture.author,
            super::claims::draft(&fixture.author, true),
            "merge:claim-preview",
        )
        .await;
        let record = super::claims::publish(
            &fixture.source,
            &fixture.author,
            &first,
            "merge:claim-publish",
        )
        .await
        .unwrap()
        .created
        .unwrap();
        let approval = review(
            &fixture.source,
            &fixture.author,
            &record,
            SourceKind::Claim,
            all(),
        )
        .await;
        approve(
            &fixture.source,
            &fixture.author,
            &approval,
            "merge:claim-serve",
        )
        .await
        .unwrap();
        let document = store::profile_claims::get_on(
            &mut *fixture.source.store.pool.acquire().await.unwrap(),
            &record,
        )
        .await
        .unwrap()
        .document;
        fixture.draft.linked_claims.push(model::Series {
            issuer: document.statement.issuer.clone(),
            claim: document.statement.claim.clone(),
            snapshot: Some(document.clone()),
        });
        let reviewed = fixture.preview().await;
        let started = fixture.start(&reviewed, "merge:claim-start").await.unwrap();
        let id = started["job"].as_str().unwrap();
        fixture.reader.refresh_trust_requests_once().await.unwrap();
        let merged = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Verify { job: id.into() },
            "merge:claim-verify",
        )
        .await
        .unwrap();
        assert_eq!(merged["claims"][0]["state"], "current_observed");
        assert_eq!(merged["claim_checks"][0]["state"], "checked");
        assert_eq!(merged["claim_checks"][0]["document"], serde_json::to_value(&document).unwrap());
        assert_eq!(merged["claim_checks"][0]["support"]["state"], "unknown");
        let mut withdrawal = super::claims::draft(&fixture.author, true);
        withdrawal.claim = Some(document.statement.claim.clone());
        withdrawal.descriptor = document.statement.descriptor.clone();
        withdrawal.expected_revision = 1;
        withdrawal.withdrawn = true;
        withdrawal.amount = "0".into();
        let withdrawn = super::claims::reviewed(
            &fixture.source,
            &fixture.author,
            withdrawal,
            "merge:claim-withdraw-review",
        )
        .await;
        let record = super::claims::publish(
            &fixture.source,
            &fixture.author,
            &withdrawn,
            "merge:claim-withdraw",
        )
        .await
        .unwrap()
        .created
        .unwrap();
        let doc = store::profile_claims::get_on(
            &mut *fixture.source.store.pool.acquire().await.unwrap(),
            &record,
        )
        .await
        .unwrap();
        let hidden = fixture
            .reader
            .act(
                action(
                    &fixture.viewer,
                    "merge:claim-retain",
                    Command::Claims {
                        request: nucleus::trust_analysis::claims::commands::Command::Import {
                            document: serde_json::to_string(&doc.document).unwrap(),
                            as_organ: false,
                        },
                    },
                ),
                Some(fixture.viewer.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        fixture
            .reader
            .act(
                Action::DeleteRecord {
                    target: hidden.clone(),
                },
                None,
            )
            .await
            .unwrap();
        let before = counts(&fixture.reader).await;
        let merged = run(
            &fixture.reader,
            &fixture.viewer,
            Request::Verify { job: id.into() },
            "merge:claim-withheld",
        )
        .await
        .unwrap();
        assert_eq!(merged["claims"][0]["state"], "withheld");
        assert_eq!(merged["claim_checks"][0]["state"], "withheld");
        assert!(merged["claim_checks"][0].get("support").is_none());
        assert!(merged["claims"][0]["revision"].is_null());
        assert!(!serde_json::to_string(&merged).unwrap().contains(&hidden));
        assert_eq!(counts(&fixture.reader).await, before);
        fixture
            .draft
            .contact_subjects
            .push(Identity::Person(fixture.contact.uid.clone()));
        let mut changed: model::Review =
            serde_json::from_value(reviewed["review"].clone()).unwrap();
        changed.draft = fixture.draft.clone();
        changed.questions = changed.draft.questions().unwrap();
        assert!(
            run(
                &fixture.reader,
                &fixture.viewer,
                Request::Start {
                    expected_review: changed.hash().unwrap(),
                    review: Box::new(changed)
                },
                "merge:unreviewed-context"
            )
            .await
            .is_err()
        );
    });
}
