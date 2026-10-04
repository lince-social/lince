use engine::{Engine, EngineError, actions::Action, enrolment::CellTransport, trust::Signer};
use nucleus::sand_package::{
    self as model, Command, Identity, Kind, License, Package, Query, Response,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

async fn owner() -> (Engine, Signer, String) {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = Signer::generate(&organ, "packages-key");
    engine.set_organ_signer(signer.clone()).await.unwrap();
    let body = json!({"format":nucleus::component::composition::FORMAT,"castle":{"name":"Board","parts":[{"label":"Original"}]}}).to_string();
    let record = engine
        .act(
            Action::CreateCustomComponent {
                head: "Board".into(),
                body,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    (engine, signer, record)
}

fn licenses() -> Vec<License> {
    vec![
        License {
            name: "Author license".into(),
            text: "Retain author and copyright\nFull license text".into(),
        },
        License {
            name: "Dependency license".into(),
            text: "Dependency license text".into(),
        },
    ]
}

async fn command(engine: &Engine, request: Command) -> Result<Response, EngineError> {
    let outcome = engine.act(Action::SandPackage { request }, None).await?;
    Ok(serde_json::from_value(outcome.data.unwrap()).unwrap())
}

async fn save(engine: &Engine, record: &str) -> Identity {
    let Response::Saved { identity } = command(
        engine,
        Command::Save {
            record: record.into(),
            kind: Kind::Castle,
            licenses: licenses(),
            credits: vec!["Original Author and dependency authors".into()],
        },
    )
    .await
    .unwrap() else {
        panic!("Expected a package identity");
    };
    identity
}

async fn contact(engine: &Engine) -> String {
    let uid = nucleus::new_uid("r");
    store::organs::add_contact(&engine.store.pool, &uid, None, "Contact", "", 0)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &uid, "known")
        .await
        .unwrap();
    uid
}

#[tokio::test]
async fn private_snapshots_keep_identity_metadata_and_immutable_versions() {
    let (engine, _, record) = owner().await;
    let peer = contact(&engine).await;
    let identity = save(&engine, &record).await;
    let stored = store::sand_packages::get(&engine.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.public);
    assert_eq!(stored.package.manifest.licenses, licenses());
    assert_eq!(stored.package.manifest.identity.id, record);
    assert_eq!(
        stored.package.manifest.author,
        stored.package.manifest.identity.origin
    );
    assert_eq!(stored.package.manifest.permissions, ["canvas:place"]);
    assert_eq!(
        stored.package.manifest.credits,
        ["Original Author and dependency authors"]
    );
    let Response::Catalogue { entries, .. } = engine
        .public_sand_packages(&peer, Query::List { offset: 0 })
        .await
        .unwrap()
    else {
        panic!("Expected catalogue");
    };
    assert!(entries.is_empty());
    assert!(
        engine
            .public_sand_packages(
                &peer,
                Query::Inspect {
                    identity: identity.clone()
                }
            )
            .await
            .is_err()
    );
    command(
        &engine,
        Command::SetPublic {
            identity: identity.clone(),
            public: true,
        },
    )
    .await
    .unwrap();
    let Response::Package { package, .. } = engine
        .public_sand_packages(
            &peer,
            Query::Inspect {
                identity: identity.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("Expected package");
    };
    assert_eq!(package, stored.package);
    engine.act(Action::EditRecordText { target: record.clone(), head: Some("Changed".into()), body: Some(json!({"format":nucleus::component::composition::FORMAT,"castle":{"name":"Changed","parts":[{}]}}).to_string()) }, None).await.unwrap();
    let second = save(&engine, &record).await;
    assert_eq!(second.id, identity.id);
    assert_eq!(second.origin, identity.origin);
    assert_eq!(second.version, 2);
    let old = store::sand_packages::get(&engine.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old.package, package);
    assert!(old.public);
    assert!(
        !store::sand_packages::get(&engine.store.pool, &second, false)
            .await
            .unwrap()
            .unwrap()
            .public
    );
    command(
        &engine,
        Command::SetPublic {
            identity: identity.clone(),
            public: false,
        },
    )
    .await
    .unwrap();
    assert!(
        engine
            .public_sand_packages(&peer, Query::Inspect { identity })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn catalogue_requires_known_contacts_and_respects_hidden_records() {
    let (engine, _, record) = owner().await;
    let identity = save(&engine, &record).await;
    command(
        &engine,
        Command::SetPublic {
            identity: identity.clone(),
            public: true,
        },
    )
    .await
    .unwrap();
    assert!(
        engine
            .public_sand_packages(&nucleus::new_uid("r"), Query::List { offset: 0 })
            .await
            .is_err()
    );
    let peer = contact(&engine).await;
    store::visibility::set_hidden_from_organ(&engine.store.pool, &peer, &record, true)
        .await
        .unwrap();
    let Response::Catalogue { entries, .. } = engine
        .public_sand_packages(&peer, Query::List { offset: 0 })
        .await
        .unwrap()
    else {
        panic!("Expected catalogue");
    };
    assert!(entries.is_empty());
    assert!(
        engine
            .public_sand_packages(
                &peer,
                Query::Inspect {
                    identity: identity.clone()
                }
            )
            .await
            .is_err()
    );
    store::organs::set_trust(&engine.store.pool, &peer, "blocked")
        .await
        .unwrap();
    assert!(
        engine
            .public_sand_packages(&peer, Query::List { offset: 0 })
            .await
            .is_err()
    );
    assert!(
        engine
            .public_sand_packages(&peer, Query::Inspect { identity })
            .await
            .is_err()
    );
}

struct Peer(Mutex<Package>);

#[async_trait::async_trait]
impl CellTransport for Peer {
    async fn sand_packages(&self, _: &str, query: Query) -> Result<Response, EngineError> {
        match query {
            Query::Inspect { .. } => Ok(Response::Package {
                package: self.0.lock().unwrap().clone(),
                public: true,
                origin_verified: true,
            }),
            Query::List { .. } => panic!("Unexpected list"),
        }
    }
    async fn enrol(
        &self,
        _: &engine::pairing::EnrolmentInvite,
    ) -> Result<engine::roster::SignedRoster, EngineError> {
        panic!("Unexpected enrolment")
    }
    async fn audit_against(
        &self,
        _: &str,
    ) -> Result<Option<engine::wire::AuditAgreement>, EngineError> {
        panic!("Unexpected audit")
    }
    async fn carrier_probe(&self, _: &str) -> engine::wire::CarrierProbe {
        panic!("Unexpected probe")
    }
    async fn collect_mail_now(&self) -> Result<usize, EngineError> {
        panic!("Unexpected mailbox")
    }
    async fn sync_now(&self) -> Result<usize, EngineError> {
        panic!("Unexpected sync")
    }
    async fn ask_to_be_carried(&self, _: &str) -> Result<(), EngineError> {
        panic!("Unexpected carry")
    }
    async fn redeem_mailbox_invite(&self, _: &str, _: &str) -> Result<(String, i64), EngineError> {
        panic!("Unexpected invite")
    }
}

fn resign(package: &mut Package, signer: &Signer) {
    package.digest = package.content_digest().unwrap();
    package.signature = signer.sign_bytes(&package.signing_bytes().unwrap());
}

async fn receiver(package: Package) -> (Engine, Arc<Peer>) {
    let engine = Engine::open_memory().await.unwrap();
    let origin = package.manifest.identity.origin.clone();
    store::organs::add_contact(&engine.store.pool, &origin, None, "Author Organ", "", 0)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &origin, "known")
        .await
        .unwrap();
    engine::trust::adopt_key(
        &engine.store,
        &origin,
        &package.manifest.key_id,
        &package.manifest.public_key,
    )
    .await
    .unwrap();
    let transport = Arc::new(Peer(Mutex::new(package)));
    let endpoint: Arc<dyn CellTransport> = transport.clone();
    engine.set_enroller(Arc::downgrade(&endpoint));
    (engine, transport)
}

#[tokio::test]
async fn receiving_unsupported_packages_preserves_everything_without_enabling() {
    let (author, signer, record) = owner().await;
    let identity = save(&author, &record).await;
    let mut package = store::sand_packages::get(&author.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap()
        .package;
    package.manifest.execution = "future.native.v8".into();
    package.manifest.permissions = vec!["camera".into(), "filesystem:read".into()];
    package.payload = "opaque unsupported executable format".into();
    resign(&mut package, &signer);
    let (receiver, _peer) = receiver(package.clone()).await;
    command(
        &receiver,
        Command::Receive {
            organ: identity.origin.clone(),
            identity: identity.clone(),
        },
    )
    .await
    .unwrap();
    let stored = store::sand_packages::get(&receiver.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.package, package);
    assert!(!stored.public);
    assert!(stored.origin_verified);
    assert!(
        store::records::get(&receiver.store.pool, &identity.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        command(
            &receiver,
            Command::Enable {
                identity: identity.clone()
            }
        )
        .await
        .is_err()
    );
    let Response::Package {
        package: inspected, ..
    } = command(
        &receiver,
        Command::Inspect {
            organ: None,
            identity,
        },
    )
    .await
    .unwrap()
    else {
        panic!("Expected metadata");
    };
    assert_eq!(inspected, package);
}

#[tokio::test]
async fn receiving_rejects_tampering_and_conflicting_versions() {
    let (author, signer, record) = owner().await;
    let identity = save(&author, &record).await;
    let original = store::sand_packages::get(&author.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap()
        .package;
    let mut altered = original.clone();
    altered.manifest.credits.clear();
    altered.digest = altered.content_digest().unwrap();
    let (receiver, peer) = receiver(altered).await;
    let receive = Command::Receive {
        organ: identity.origin.clone(),
        identity: identity.clone(),
    };
    assert!(command(&receiver, receive.clone()).await.is_err());
    assert!(
        store::sand_packages::get(&receiver.store.pool, &identity, false)
            .await
            .unwrap()
            .is_none()
    );
    *peer.0.lock().unwrap() = original.clone();
    command(&receiver, receive.clone()).await.unwrap();
    command(&receiver, receive.clone()).await.unwrap();
    let mut replacement = original.clone();
    replacement.payload = "Changed source under the same version".into();
    resign(&mut replacement, &signer);
    *peer.0.lock().unwrap() = replacement;
    assert!(command(&receiver, receive).await.is_err());
    assert_eq!(
        store::sand_packages::get(&receiver.store.pool, &identity, false)
            .await
            .unwrap()
            .unwrap()
            .package,
        original
    );
}

#[tokio::test]
async fn enabling_rechecks_original_key_revocation_and_missing_packages() {
    let (engine, signer, record) = owner().await;
    let identity = save(&engine, &record).await;
    command(
        &engine,
        Command::Enable {
            identity: identity.clone(),
        },
    )
    .await
    .unwrap();
    store::roster::record_revocation(
        &engine.store.pool,
        &identity.origin,
        &signer.public_key_b64(),
        "revoked",
    )
    .await
    .unwrap();
    assert!(
        command(
            &engine,
            Command::Enable {
                identity: identity.clone()
            }
        )
        .await
        .is_err()
    );
    assert!(
        command(
            &engine,
            Command::SetPublic {
                identity: identity.clone(),
                public: true
            }
        )
        .await
        .is_err()
    );
    assert!(
        command(
            &engine,
            Command::Inspect {
                organ: None,
                identity
            }
        )
        .await
        .is_ok()
    );
    assert!(
        command(
            &engine,
            Command::Enable {
                identity: Identity {
                    origin: nucleus::new_uid("r"),
                    id: nucleus::new_uid("r"),
                    version: 1
                }
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn packages_require_licenses_and_catalogues_have_bounded_pages() {
    let (engine, signer, record) = owner().await;
    assert!(
        command(
            &engine,
            Command::Save {
                record: record.clone(),
                kind: Kind::Sand,
                licenses: vec![],
                credits: vec![]
            }
        )
        .await
        .is_err()
    );
    let identity = save(&engine, &record).await;
    let original = store::sand_packages::get(&engine.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap()
        .package;
    for version in 2..=model::PAGE_SIZE + 1 {
        let mut package = original.clone();
        package.manifest.identity.version = version;
        resign(&mut package, &signer);
        store::sand_packages::save(&engine.store.pool, &package, true, None)
            .await
            .unwrap();
    }
    let Response::Catalogue { entries, next } = command(
        &engine,
        Command::List {
            organ: None,
            offset: 0,
        },
    )
    .await
    .unwrap() else {
        panic!("Expected catalogue");
    };
    assert_eq!(entries.len(), model::PAGE_SIZE as usize);
    assert_eq!(next, Some(model::PAGE_SIZE));
    let Response::Catalogue { entries, next } = command(
        &engine,
        Command::List {
            organ: None,
            offset: model::PAGE_SIZE,
        },
    )
    .await
    .unwrap() else {
        panic!("Expected catalogue");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(next, None);
    assert!(
        command(
            &engine,
            Command::List {
                organ: None,
                offset: u32::MAX
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_known_person_key_cannot_attest_a_package_as_an_organ() {
    let (engine, _, record) = owner().await;
    let identity = save(&engine, &record).await;
    let mut package = store::sand_packages::get(&engine.store.pool, &identity, false)
        .await
        .unwrap()
        .unwrap()
        .package;
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Person",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let signer = Signer::generate(&person.uid, "person-key");
    engine.set_signer(signer.clone()).await.unwrap();
    package.manifest.identity.origin = person.uid;
    package.manifest.key_id = signer.key_id.clone();
    package.manifest.public_key = signer.public_key_b64();
    resign(&mut package, &signer);
    package.validate().unwrap();
    assert!(!engine.package_origin_verified(&package).await.unwrap());
}
