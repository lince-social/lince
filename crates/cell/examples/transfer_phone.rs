use cell::{Cell, CellOptions};
use engine::{
    Engine,
    actions::Action,
    location::Network,
    wire::{PeerAddr, Reach, Wire},
};
use nucleus::{
    location::{Command, PeerRequest, Settings, SourceKind, View},
    visibility::{Condition, Data, Policy},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Command as Process,
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Serialize, Deserialize)]
struct Identity {
    node: String,
    organ: String,
    root_key: String,
}

#[derive(Serialize, Deserialize)]
struct Fixture {
    authority: Identity,
    record: String,
    address: String,
    transfer: String,
    controller: String,
    contacts: BTreeMap<String, String>,
    logins: BTreeMap<String, Login>,
}

#[derive(Serialize, Deserialize)]
struct Login {
    person: String,
    username: String,
    password: String,
}

fn write_private(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|error| {
        std::io::Error::other(format!("Read fixture {}: {error}", path.display()))
    })?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn act(engine: &Engine, action: Action) -> Result<engine::actions::ActionOutcome> {
    let name = serde_json::to_value(&action)?["action"]
        .as_str()
        .unwrap_or("action")
        .to_owned();
    Ok(engine
        .act(action, None)
        .await
        .map_err(|error| std::io::Error::other(format!("Seed {name}: {error}")))?)
}

async fn record(
    engine: &Engine,
    slug: &str,
    head: &str,
    kind: nucleus::RecordKind,
    quantity: f64,
) -> Result<String> {
    act(
        engine,
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind,
            head: head.into(),
            body: String::new(),
            quantity,
        },
    )
    .await?
    .created
    .ok_or_else(|| "Record creation returned no identity".into())
}

async fn policy(engine: &Engine, record: &str, data: Data, policy: Policy) -> Result<()> {
    let revision = store::data_visibility::policy(&engine.store.pool, record, data)
        .await?
        .map_or(0, |saved| saved.revision);
    act(
        engine,
        Action::Visibility {
            request: nucleus::visibility::Command::Save {
                record_uid: record.into(),
                data,
                policy,
                expected_revision: revision,
            },
        },
    )
    .await?;
    Ok(())
}

fn audience(contacts: &BTreeMap<String, String>, radius: u32) -> Policy {
    Policy {
        include: vec![
            Condition {
                organs: vec![contacts["b"].clone()],
                ..Default::default()
            },
            Condition {
                organs: vec![contacts["c"].clone()],
                radius_metres: Some(radius),
                ..Default::default()
            },
            Condition {
                organs: vec![contacts["d"].clone()],
                ..Default::default()
            },
        ],
        exclude: vec![Condition {
            organs: vec![contacts["d"].clone()],
            ..Default::default()
        }],
    }
}

async fn seed(root: &Path, name: &str) -> Result<()> {
    if !["a", "b", "c", "d"].contains(&name) {
        return Err("Choose one of a, b, c, d".into());
    }
    let directory = root.join(name);
    fs::create_dir(&directory)?;
    let cell = Cell::open_mobile(CellOptions {
        data_dir: Some(directory.clone()),
        peer_port: Some(if name == "a" { 6175 } else { 0 }),
        ..Default::default()
    })
    .await
    .map_err(|error| std::io::Error::other(format!("Open {name} profile: {error}")))?;
    let runtime = cell.runtime();
    let engine = &runtime.engine;
    act(engine, Action::RosterCreateOrgan).await?;
    act(
        engine,
        Action::SetCellConfig {
            namespace: "lince.discovery".into(),
            fds: json!({"internet": false, "direct": true, "local": false}),
        },
    )
    .await?;
    let wire = runtime
        .wire
        .read()
        .await
        .clone()
        .ok_or("Peer endpoint unavailable")?;
    let identity = Identity {
        node: wire.node_id().to_string(),
        organ: store::organs::local(&runtime.store.pool)
            .await?
            .ok_or("Local Organ missing")?
            .uid,
        root_key: engine
            .root_signer()
            .await?
            .ok_or("Organ root key missing")?
            .public_key_b64(),
    };
    if name != "a" {
        write_private(&root.join(format!("{name}.json")), &identity)?;
        cell.shutdown().await;
        return Ok(());
    }
    let mut contacts = BTreeMap::new();
    for name in ["b", "c", "d"] {
        let other: Identity = read(&root.join(format!("{name}.json")))?;
        store::organs::add_contact(
            &runtime.store.pool,
            &other.organ,
            None,
            &format!("Validation {name}"),
            "",
            if name == "c" { 6 } else { 1 },
        )
        .await?;
        store::organs::set_node_id(&runtime.store.pool, &other.organ, Some(&other.node)).await?;
        store::organs::set_trust(&runtime.store.pool, &other.organ, "known").await?;
        engine::trust::adopt_key(
            &runtime.store,
            &other.organ,
            engine::roster::ROOT_KEY_ID,
            &other.root_key,
        )
        .await?;
        contacts.insert(name.into(), other.organ);
    }
    let controller = record(
        engine,
        "validation-driver",
        "Validation driver",
        nucleus::RecordKind::Person,
        1.0,
    )
    .await?;
    let passenger = record(
        engine,
        "validation-passenger",
        "Validation passenger",
        nucleus::RecordKind::Person,
        1.0,
    )
    .await?;
    let role = store::auth::ensure_role(&runtime.store.pool, store::auth::ADMIN_ROLE).await?;
    let mut logins = BTreeMap::new();
    for (name, person) in [("driver", &controller), ("passenger", &passenger)] {
        let username = format!("validation-{name}");
        let password = nucleus::new_uid("validation-password");
        let password_hash = utils::auth::hash_password(&password)?;
        store::auth::create_credential(
            &runtime.store.pool,
            person,
            &username,
            &password_hash,
            role,
        )
        .await?;
        logins.insert(
            name.into(),
            Login {
                person: person.clone(),
                username,
                password,
            },
        );
    }
    let source = record(
        engine,
        "validation-seat",
        "Validation transport seat",
        nucleus::RecordKind::Plain,
        1.0,
    )
    .await?;
    record(
        engine,
        "validation-need",
        "Validation passenger Need",
        nucleus::RecordKind::Plain,
        -1.0,
    )
    .await?;
    let address = record(
        engine,
        "validation-address",
        "Public validation meeting place",
        nucleus::RecordKind::Plain,
        0.0,
    )
    .await?;
    let destination = record(
        engine,
        "validation-destination",
        "Validation destination",
        nucleus::RecordKind::Plain,
        0.0,
    )
    .await?;
    act(
        engine,
        Action::SetPlace {
            target: address.clone(),
            lat: 51.5007,
            lon: -0.1246,
            address: Some("Public validation meeting place".into()),
        },
    )
    .await?;
    act(
        engine,
        Action::SetPlace {
            target: destination.clone(),
            lat: 51.5033,
            lon: -0.1195,
            address: Some("Public validation destination".into()),
        },
    )
    .await?;
    let assertion = engine
        .change_record(
            engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: source.clone(),
                mutation: engine::record_change::Mutation::Assertion {
                    predicate: "address".into(),
                    object: Some(address.clone()),
                    quantity: None,
                    unit: None,
                },
            },
            None,
        )
        .await?
        .created
        .ok_or("Address link missing")?;
    let saved = engine.saved_location_of(&source, None).await?;
    act(
        engine,
        Action::SavedLocation {
            request: nucleus::vocabulary::SavedLocationCommand::Select {
                record: source.clone(),
                assertion: Some(assertion),
                expected_revision: saved.revision,
            },
        },
    )
    .await?;
    let signer = engine::trust::Signer::load_or_create(
        &directory.join("keys/validation-person.key"),
        &controller,
        "validation-person",
    )?;
    engine.set_signer(signer).await?;
    let now = chrono::Utc::now();
    let draft: Action = serde_json::from_value(json!({
        "action": "create-transfer-draft",
        "request_id": "phone-transfer-validation",
        "creator": controller,
        "slug": "validation-transport",
        "head": "Phone validation Transport",
        "agreement": "full",
        "agreement_pct": null,
        "satiation": "none",
        "parent": null,
        "source": source,
        "visibility": "hidden",
        "max_proximity": null,
        "reserve_default": "agreed",
        "require_confirmation": true,
        "invitees": [passenger],
        "promises": [{
            "uid": "validation-seat-promise", "record": source, "party": controller, "delta": -1.0,
            "window_start": (now - chrono::Duration::minutes(5)).to_rfc3339(),
            "window_end": (now + chrono::Duration::hours(4)).to_rfc3339(),
            "item": {"title": "Transport seat", "transport": {"activity": "ride", "direction": "offer", "origin": address, "destination": destination, "count": 1, "capacity": 2}}
        }]
    }))?;
    let transfer = act(engine, draft)
        .await?
        .created
        .ok_or("Transfer draft missing")?;
    for record in [&source, &address] {
        policy(
            engine,
            record,
            Data::Record,
            Policy {
                include: vec![Condition::default()],
                exclude: vec![],
            },
        )
        .await?;
    }
    policy(engine, &source, Data::Place, audience(&contacts, 3000)).await?;
    policy(engine, &address, Data::Place, audience(&contacts, 5000)).await?;
    policy(
        engine,
        &source,
        Data::LiveLocation,
        audience(&contacts, 3000),
    )
    .await?;
    let this_cell = store::cells::local(&runtime.store.pool)
        .await?
        .ok_or("Local Cell missing")?;
    act(
        engine,
        Action::Location {
            request: Command::Configure {
                settings: Settings {
                    record_uid: source.clone(),
                    controller_uid: controller.clone(),
                    source_cell_uid: this_cell.uid,
                    source_node_id: identity.node.clone(),
                    source_kind: SourceKind::Device,
                    duration_seconds: 3600,
                    recipients: vec![],
                    transfer_uid: Some(transfer.clone()),
                },
            },
        },
    )
    .await?;
    write_private(
        &root.join("fixture.json"),
        &Fixture {
            authority: identity,
            record: source,
            address,
            transfer,
            controller,
            contacts,
            logins,
        },
    )?;
    cell.shutdown().await;
    Ok(())
}

struct Observer {
    engine: Arc<Engine>,
    wire: Arc<Wire>,
    task: tokio::task::JoinHandle<()>,
}

async fn query(
    observer: &Observer,
    fixture: &Fixture,
    record: &str,
    data: Data,
) -> std::result::Result<Value, engine::EngineError> {
    observer
        .wire
        .location_request(
            &fixture.authority.node,
            PeerRequest::Read {
                record_uid: record.into(),
                data,
            },
        )
        .await
}

fn summarize(result: &std::result::Result<Value, engine::EngineError>, started_ms: i64) -> Value {
    match result {
        Ok(value) => {
            if let Ok(view) = serde_json::from_value::<View>(value.clone()) {
                json!({"allowed": true, "remote_response": true, "status": view.status, "source_kind": view.source_kind, "has_fix": view.fix.is_some(), "age_ms": view.age_ms, "radius_metres": view.radius_metres, "sequence": view.fix.as_ref().map(|fix| fix.sequence), "captured_after_observer_start": view.fix.as_ref().map(|fix| fix.captured_at_ms >= started_ms), "capture_age_ms": view.fix.as_ref().map(|fix| fix.capture_age_ms)})
            } else {
                json!({"allowed": true, "remote_response": true, "has_place": !value["place"].is_null(), "radius_metres": value["place"]["radius_metres"], "has_address_text": value["place"]["address"].is_string()})
            }
        }
        Err(engine::EngineError::Forbidden(_)) => {
            json!({"allowed": false, "remote_response": true, "forbidden": true})
        }
        Err(_) => json!({"allowed": false, "transport_or_protocol_error": true}),
    }
}

async fn observe(root: &Path, phone: IpAddr, seconds: u64) -> Result<()> {
    let fixture: Fixture = read(&root.join("fixture.json"))?;
    let mut observers = BTreeMap::new();
    for name in ["b", "c", "d"] {
        let directory = root.join(name);
        let engine = Arc::new(
            Engine::open(&format!(
                "sqlite://{}",
                directory.join("lince.db").display()
            ))
            .await?,
        );
        let secret = engine::wire::node_secret(&directory.join("keys/node-ed25519-v1.key"))?;
        let wire = Arc::new(
            Wire::bind_on_port(engine.clone(), secret, Reach::Local, None, false, 0).await?,
        );
        wire.remember_addr(
            PeerAddr::new(fixture.authority.node.parse()?)
                .with_ip_addr(SocketAddr::new(phone, 6175)),
        );
        engine.attach_location_network(wire.clone());
        let server = wire.clone();
        let task = tokio::spawn(async move {
            server.serve().await;
        });
        observers.insert(name, Observer { engine, wire, task });
    }
    let started = Instant::now();
    let started_ms = chrono::Utc::now().timestamp_millis();
    while started.elapsed() < Duration::from_secs(seconds) {
        let mut raw = BTreeMap::new();
        let mut report = BTreeMap::new();
        for (name, observer) in &observers {
            let (live, saved, address) = tokio::join!(
                query(observer, &fixture, &fixture.record, Data::LiveLocation),
                query(observer, &fixture, &fixture.record, Data::Place),
                query(observer, &fixture, &fixture.address, Data::Place),
            );
            report.insert(*name, json!({"live": summarize(&live, started_ms), "saved": summarize(&saved, started_ms), "referenced_address": summarize(&address, started_ms)}));
            raw.insert(
                *name,
                live.ok()
                    .and_then(|value| serde_json::from_value::<View>(value).ok()),
            );
        }
        let compared = raw["b"]
            .as_ref()
            .and_then(|view| view.fix.as_ref())
            .zip(raw["c"].as_ref().and_then(|view| view.fix.as_ref()))
            .filter(|(exact, coarse)| exact.sequence == coarse.sequence)
            .map(|(exact, coarse)| {
                exact.latitude != coarse.latitude || exact.longitude != coarse.longitude
            });
        println!(
            "{}",
            json!({"at_ms": chrono::Utc::now().timestamp_millis(), "elapsed_ms": started.elapsed().as_millis(), "transport": "real-wire", "viewers": report, "coarse_differs_from_exact_same_sequence": compared})
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    for (_, observer) in observers {
        observer.wire.endpoint().close().await;
        observer.task.abort();
        let _ = observer.task.await;
        observer.engine.store.pool.close().await;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, directory] if mode == "prepare" => {
            let root = PathBuf::from(directory);
            fs::create_dir(&root)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
            }
            let root = fs::canonicalize(root)?;
            for name in ["b", "c", "d", "a"] {
                if !Process::new(std::env::current_exe()?)
                    .arg("seed")
                    .arg(&root)
                    .arg(name)
                    .status()?
                    .success()
                {
                    return Err(format!("Disposable profile {name} could not be prepared").into());
                }
            }
            println!(
                "Prepared four disposable profiles. Copy only profile a into the new debug application's files/lince; launch it and explicitly start/approve device location. Observe with: transfer_phone observe ROOT PHONE_WIFI_IP SECONDS"
            );
            Ok(())
        }
        [mode, directory, name] if mode == "seed" => seed(Path::new(directory), name).await,
        [mode, directory, phone, seconds] if mode == "observe" => {
            observe(Path::new(directory), phone.parse()?, seconds.parse()?).await
        }
        _ => Err("Use prepare NEW_DIRECTORY or observe DIRECTORY PHONE_WIFI_IP SECONDS".into()),
    }
}
