use super::*;
use serde_json::{Value, json};

struct LocalService {
    engine: Arc<engine::Engine>,
    endpoint: String,
    source: String,
}

#[async_trait::async_trait]
impl engine::social::Network for LocalService {
    async fn request(
        &self,
        destination: &str,
        request: nucleus::social::PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        if destination != self.endpoint {
            return Err(engine::EngineError::Forbidden(
                "This prepared service has no external destinations.".into(),
            ));
        }
        self.engine
            .social_public_request(
                &self.source,
                destination,
                request,
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) receiver: Option<Mutex<mpsc::Receiver<Result<Prepared, String>>>>,
    pub(super) data: Option<Value>,
    pub(super) failed: bool,
}

pub(super) struct Prepared {
    source: String,
    runtime: cell::CellRuntime,
    servers: Vec<crate::practice_cells::Worker>,
    primary_servers: Vec<crate::practice_cells::Worker>,
    data: Value,
}

fn subjects(subject: &str) -> bool {
    matches!(
        subject,
        "learn-organ"
            | "contacts"
            | "access-control"
            | "organ-sync"
            | "devices"
            | "mail"
            | "discovery"
            | "conversations"
            | "calls"
            | "learn-transfer"
            | "transfer-automation"
            | "instinct-import"
            | "learn-sync"
            | "blob-sync"
            | "backup"
    )
}

pub(super) fn prepare(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if !subjects(practice.runner.lesson.subject)
        || practice.setup.is_some()
        || practice.records.is_empty()
    {
        return;
    }
    let result = practice
        .community
        .receiver
        .as_ref()
        .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
    if let Some(result) = result {
        world.get_mut::<Practice>(root).unwrap().community.receiver = None;
        match result {
            Ok(prepared) => {
                let source = world.get::<Practice>(root).unwrap().source.clone();
                let mut cells = world.resource_mut::<crate::practice_cells::PracticeCells>();
                cells
                    .cells
                    .insert(prepared.source.clone(), prepared.runtime);
                cells
                    .servers
                    .insert(prepared.source.clone(), prepared.servers);
                cells.servers.insert(source, prepared.primary_servers);
                cells
                    .records
                    .entry(prepared.source.clone())
                    .or_default()
                    .insert(prepared.data["peer"].as_str().unwrap().into());
                let primary = world.get::<Practice>(root).unwrap().source.clone();
                let mut cells = world.resource_mut::<crate::practice_cells::PracticeCells>();
                for key in [
                    "record",
                    "conversation",
                    "thread",
                    "transfer",
                    "person",
                    "stock",
                    "promise",
                    "rule",
                    "protein",
                ] {
                    if let Some(uid) = prepared.data[key].as_str() {
                        cells
                            .records
                            .entry(primary.clone())
                            .or_default()
                            .insert(uid.into());
                        cells
                            .records
                            .entry(prepared.source.clone())
                            .or_default()
                            .insert(uid.into());
                    }
                }
                crate::protein_area::ensure_auxiliary(
                    world,
                    &crate::protein_area::Source::Organ(prepared.source),
                );
                world.get_mut::<Practice>(root).unwrap().community.data = Some(prepared.data);
            }
            Err(error) => {
                let mut practice = world.get_mut::<Practice>(root).unwrap();
                practice.community.failed = true;
                practice.runner.phase = Phase::Unavailable(error);
                input::release(world, root);
                render(world, root);
            }
        }
    }
    let practice = world.get::<Practice>(root).unwrap();
    if practice.community.data.is_none() {
        if practice.community.receiver.is_none() && !practice.community.failed {
            begin(world, root);
        }
        return;
    }
    if find(world, root, Role::Feature).is_none() {
        let _ = view(world, root);
    }
    if world.get::<Practice>(root).unwrap().pending.is_some()
        && let Some(operation) = world
            .get::<Practice>(root)
            .unwrap()
            .runner
            .current()
            .and_then(|step| step.operation)
    {
        let _ = execute(world, root, operation);
    }
}

fn begin(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    let source = practice.source.clone();
    let Some(runtime) = world
        .resource::<crate::practice_cells::PracticeCells>()
        .cells
        .get(&source)
        .cloned()
    else {
        return;
    };
    let primary_path = world
        .resource::<crate::practice_cells::PracticeCells>()
        .directories[&source]
        .clone();
    let peer_source = nucleus::new_uid("g");
    let peer_path = match crate::practice_cells::persistence::directory(
        primary_path.parent().unwrap().parent().unwrap(),
        &peer_source,
    ) {
        Ok(path) => path,
        Err(error) => {
            world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(error);
            return;
        }
    };
    world
        .resource_mut::<crate::practice_cells::PracticeCells>()
        .directories
        .insert(peer_source.clone(), peer_path.clone());
    let subject = practice_subject(world, root);
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let (sender, receiver) = mpsc::channel();
    world
        .get_mut::<Practice>(root)
        .unwrap()
        .extra_sources
        .push(peer_source.clone());
    let task = handle.spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let peer_engine =
                engine::Engine::open(&format!("sqlite://{}", peer_path.join("cell.db").display()))
                    .await
                    .map_err(|error| error.to_string())?;
            let peer = crate::practice_cells::persistence::runtime(peer_engine, &peer_path)?;
            let primary_uid = identity(&runtime, &primary_path, "Practice personal Organ").await?;
            let peer_uid = identity(&peer, &peer_path, "Practice shared Organ").await?;
            let mut data = json!({"primary":primary_uid,"peer":peer_uid,"source":peer_source});
            let mut primary_servers = Vec::new();
            let mut servers = Vec::new();
            if matches!(subject, "contacts" | "organ-sync" | "devices" | "mail" | "discovery" | "conversations" | "calls" | "blob-sync") {
                primary_servers = serve(&runtime, &primary_path).await?;
                servers = serve(&peer, &peer_path).await?;
                let wire = peer
                    .wire
                    .read()
                    .await
                    .clone()
                    .ok_or("The prepared peer is unavailable.")?;
                data["invite"] = json!(
                    wire.pairing_invite()
                        .await
                        .map_err(|error| error.to_string())?
                        .encode()
                );
                data["contact"] = json!(format!("o-{}", wire.node_id()));
                if matches!(subject, "organ-sync" | "mail" | "discovery" | "conversations" | "calls" | "blob-sync") {
                    let primary_wire = runtime
                        .wire
                        .read()
                        .await
                        .clone()
                        .ok_or("The practice connection is unavailable.")?;
                    primary_wire
                        .pair_with(
                            &wire
                                .pairing_invite()
                                .await
                                .map_err(|error| error.to_string())?,
                            "Practice shared Organ",
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    wire.pair_with(
                        &primary_wire
                            .pairing_invite()
                            .await
                            .map_err(|error| error.to_string())?,
                        "Practice personal Organ",
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    primary_wire.reconnect_contact(&peer_uid).await.map_err(|error| error.to_string())?;
                    let record = runtime
                        .engine
                        .act(
                            engine::actions::Action::CreateRecordDraft {
                                draft: engine::record_creation::Draft {
                                    head: "A shared practice note".into(),
                                    body: "Only this prepared note is selected for sharing.".into(),
                                    ..default()
                                },
                            },
                            None,
                        )
                        .await
                        .map_err(|error| error.to_string())?
                        .created
                        .ok_or("The shared note was not confirmed.")?;
                    runtime
                        .engine
                        .act(
                            engine::actions::Action::SetSyncPolicy {
                                target: peer_uid.clone(),
                                sync_out: true,
                                sync_in: false,
                            },
                            None,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    runtime
                        .engine
                        .act(
                            engine::actions::Action::SetContactShare {
                                target: peer_uid.clone(),
                                protein: Some(
                                    json!({"source":"record","where":[{"uid_eq":record}]}),
                                ),
                            },
                            None,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    if !store::contact_share::picked(&runtime.store.pool, &peer_uid)
                        .await.map_err(|error| error.to_string())?.contains(&record)
                    {
                        return Err("The prepared sharing selection did not include its note.".into());
                    }
                    runtime.engine.act(engine::actions::Action::SetSyncPolicy {
                        target: peer_uid.clone(), sync_out: false, sync_in: false,
                    }, None).await.map_err(|error| error.to_string())?;
                    peer.engine
                        .act(
                            engine::actions::Action::SetSyncPolicy {
                                target: primary_uid.clone(),
                                sync_out: false,
                                sync_in: true,
                            },
                            None,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    data["record"] = json!(record);
                    if subject == "mail" {
                        let (ops, _) = runtime
                            .engine
                            .ops_after(0, 1000)
                            .await
                            .map_err(|error| error.to_string())?;
                        let batch = engine::sync::OpBatch {
                            from_organ: primary_uid.clone(),
                            ops: ops.into_iter().filter(|op| op.uid == record).collect(),
                        };
                        let envelope = runtime
                            .engine
                            .prepare_outgoing_mail(&peer_uid, Some(&record), &batch)
                            .await
                            .map_err(|error| error.to_string())?;
                        data["envelope"] = json!(envelope.uid);
                    }
                }
            }
            if subject == "discovery" {
                use nucleus::social::{Command, PostDraft, PostState, ServiceSettings, Snippet};
                let endpoint = peer.wire.read().await.as_ref().unwrap().node_id().to_string();
                let source = runtime.wire.read().await.as_ref().unwrap().node_id().to_string();
                let act = |request| engine::actions::Action::Social { request };
                peer.engine.act(act(Command::ConfigureServices { settings: ServiceSettings { directory: true, mailbox: true, ..default() } }), None).await.map_err(|error| error.to_string())?;
                let saved = peer.engine.act(act(Command::SaveDraft { record: None, source: None, draft: PostDraft { title: "Practice bicycle help".into(), text: "A prepared Contribution. No real announcement leaves this computer.".into(), destinations: vec![endpoint.clone()], ..default() } }), None).await.map_err(|error| error.to_string())?.data.ok_or("The announcement draft was not confirmed.")?;
                let record = saved["record"].as_str().ok_or("The announcement Record is missing.")?.to_owned();
                peer.engine.act(act(Command::PrepareReplyKeys { record: record.clone(), services: vec![endpoint.clone()] }), None).await.map_err(|error| error.to_string())?;
                let preview = peer.engine.act(act(Command::Preview { record: record.clone(), state: PostState::Active }), None).await.map_err(|error| error.to_string())?.data.ok_or("The publication preview is missing.")?;
                let document: Snippet = serde_json::from_value(preview["document"].clone()).map_err(|error| error.to_string())?;
                peer.engine.act(act(Command::Publish { record, preview_hash: preview["preview_hash"].as_str().unwrap().into(), document: document.clone() }), None).await.map_err(|error| error.to_string())?;
                let local_service = Arc::new(LocalService { engine: peer.engine.clone(), endpoint: endpoint.clone(), source });
                peer.engine.attach_social_network(local_service.clone());
                peer.engine.social_reconcile_private_admissions().await.map_err(|error| error.to_string())?;
                for _ in 0..4 { peer.engine.social_publish_once().await.map_err(|error| error.to_string())?; }
                peer.engine.attach_social_network(peer.wire.read().await.as_ref().unwrap().clone());
                runtime.engine.act(act(Command::SaveServer { choice: nucleus::social::ServerChoice { endpoint: endpoint.clone(), label: "Prepared local discovery service".into(), query: true, mailbox: true, ..default() } }), None).await.map_err(|error| error.to_string())?;
                data["endpoint"] = json!(endpoint);
            }
            if matches!(subject, "conversations" | "calls") {
                let (conversation, thread) = runtime.engine.start_conversation(&peer_uid, "Practice conversation").await.map_err(|error| error.to_string())?;
                runtime.engine.send_message(&thread, "Practice peer", "Welcome to this disposable conversation.").await.map_err(|error| error.to_string())?;
                data["conversation"] = json!(conversation);
                data["thread"] = json!(thread);
            }
            if subject == "access-control" {
                let role = runtime
                    .engine
                    .act(
                        engine::actions::Action::CreateRole {
                            name: "practice-reader".into(),
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                role.created.ok_or("The Role was not confirmed.")?;
                store::auth::ensure_permission(&runtime.store.pool, "record", "read")
                    .await
                    .map_err(|error| error.to_string())?;
                runtime
                    .engine
                    .act(
                        engine::actions::Action::GrantPermission {
                            role: "practice-reader".into(),
                            permission: "record:read".into(),
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let user = runtime
                    .engine
                    .act(
                        engine::actions::Action::CreateUser {
                            username: "practice-reader".into(),
                            name: "Practice reader".into(),
                            password: nucleus::new_uid("practice-password"),
                            role: "practice-reader".into(),
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .created
                    .ok_or("The sample user was not confirmed.")?;
                data["user"] = json!(user);
                data["record"] = json!(
                    runtime
                        .engine
                        .act(
                            engine::actions::Action::CreateRecordDraft {
                                draft: engine::record_creation::Draft {
                                    head: "Practice reader's sample".into(),
                                    ..default()
                                }
                            },
                            None
                        )
                        .await
                        .map_err(|error| error.to_string())?
                        .created
                        .ok_or("The sample Record was not confirmed.")?
                );
                runtime
                    .engine
                    .act(
                        engine::actions::Action::GrantVisibility {
                            subject_kind: "actor".into(),
                            subject: data["user"].as_str().map(str::to_owned),
                            target: data["record"].as_str().unwrap().into(),
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
            }
            if matches!(subject, "learn-transfer" | "transfer-automation") {
                let prepared = commitments::fixture(&runtime, &primary_path, subject == "transfer-automation").await?;
                for (key, value) in prepared.as_object().unwrap() { data[key] = value.clone(); }
            }
            if subject == "instinct-import" {
                runtime.engine.act(engine::actions::Action::CreateRecord {
                    slug: Some("protein".into()), kind: nucleus::RecordKind::Plain,
                    head: "A deliberately conflicting practice Record".into(),
                    body: "Cancel this preview; importing must not replace this Record.".into(), quantity: 0.0,
                }, None).await.map_err(|error| error.to_string())?;
            }
            if subject == "learn-sync" {
                let prepared = disk::fixture(&runtime, &primary_path).await?;
                for (key, value) in prepared.as_object().unwrap() { data[key] = value.clone(); }
            }
            if subject == "blob-sync" {
                runtime.engine.initialize_blob_sync(&primary_path.join("blobs")).await.map_err(|error| error.to_string())?;
                peer.engine.initialize_blob_sync(&peer_path.join("blobs")).await.map_err(|error| error.to_string())?;
                let file = primary_path.join("blob-sample.txt");
                tokio::fs::write(&file, "A bundled practice file. These bytes are separate from Record text.").await.map_err(|error| error.to_string())?;
                tokio::fs::write(peer_path.join("blob-sample.txt"), "A bundled practice file.").await.map_err(|error| error.to_string())?;
                data["file"] = json!(file);
                data["endpoint"] = json!(peer.wire.read().await.as_ref().unwrap().node_id().to_string());
                primary_servers.push(crate::practice_cells::Worker(cell::blob_sync::spawn(runtime.clone())));
                servers.push(crate::practice_cells::Worker(cell::blob_sync::spawn(peer.clone())));
            }
            Ok::<_, String>(Prepared {
                source: peer_source,
                runtime: peer,
                servers,
                primary_servers,
                data,
            })
        })
        .await
        .unwrap_or_else(|_| {
            Err("Preparing the local peer timed out. Skip or Close remains available.".into())
        });
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut practice = world.get_mut::<Practice>(root).unwrap();
    practice.community.receiver = Some(Mutex::new(receiver));
    practice.tasks.push(task);
}

fn practice_subject(world: &World, root: Entity) -> &'static str {
    world.get::<Practice>(root).unwrap().runner.lesson.subject
}

async fn identity(
    runtime: &cell::CellRuntime,
    path: &std::path::Path,
    name: &str,
) -> Result<String, String> {
    let engine = &runtime.engine;
    let organ = store::organs::local(&engine.store.pool)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("The sample Organ is missing.")?;
    engine.set_root_key_path(path.join("root.key"));
    engine.set_sealing_keyring_path(path.join("sealing.json"));
    let signer = engine
        .operational_key_for(&organ.uid)
        .await
        .map_err(|error| error.to_string())?;
    engine
        .set_signer(signer.clone())
        .await
        .map_err(|error| error.to_string())?;
    engine
        .set_organ_signer(signer)
        .await
        .map_err(|error| error.to_string())?;
    engine
        .renew_local_roster()
        .await
        .map_err(|error| error.to_string())?;
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: organ.uid.clone(),
                head: Some(name.into()),
                body: None,
            },
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(organ.uid)
}

async fn serve(
    runtime: &cell::CellRuntime,
    path: &std::path::Path,
) -> Result<Vec<crate::practice_cells::Worker>, String> {
    let worker = crate::practice_cells::Worker(
        runtime
            .start_loopback_peer(&path.join("network.key"))
            .await
            .map_err(|error| error.to_string())?,
    );
    runtime
        .engine
        .create_organ_identity()
        .await
        .map_err(|error| error.to_string())?;
    store::cells::set_config(
        &runtime.store.pool,
        "lince.discovery",
        &json!({"accept_unknown":true,"local":false,"reach":"local"}),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(vec![worker])
}

fn view(world: &mut World, root: Entity) -> Result<Entity, String> {
    let practice = world.get::<Practice>(root).unwrap();
    let workspace = practice.workspace;
    let source = practice.source.clone();
    let subject = practice.runner.lesson.subject;
    if subject == "instinct-import" {
        let owner = super::super::spawn(world, root, workspace, DVec2::new(800.0, 0.0), default());
        own(world, root, owner, Role::Feature);
        return Ok(owner);
    }
    if subject == "learn-sync" {
        let data = practice.community.data.clone().unwrap();
        return disk::view(world, root, &data);
    }
    if subject == "blob-sync" {
        let data = practice.community.data.clone().unwrap();
        let owner = crate::sand_store::spawn_scoped_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Sync,
            DVec2::new(800.0, 0.0),
            source,
        );
        own(world, root, owner, Role::Feature);
        crate::sync_castle::prepare_copy(
            world,
            owner,
            std::path::PathBuf::from(data["file"].as_str().unwrap()),
            data["endpoint"].as_str().unwrap(),
        );
        let peer = crate::sand_store::spawn_scoped_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Sync,
            DVec2::new(1580.0, 0.0),
            data["source"].as_str().unwrap().into(),
        );
        own(world, root, peer, Role::Auxiliary);
        return Ok(owner);
    }
    if matches!(subject, "learn-transfer" | "transfer-automation") {
        let data = practice.community.data.clone().unwrap();
        return Ok(commitments::view(world, root, &data));
    }
    if matches!(subject, "conversations" | "calls") {
        let uid = practice.community.data.as_ref().unwrap()["conversation"]
            .as_str()
            .unwrap()
            .to_owned();
        let view = crate::full_record::open(
            world,
            root,
            &uid,
            crate::protein_area::Source::Organ(source),
        )
        .ok_or("The conversation view is unavailable.")?;
        own(world, root, view, Role::Feature);
        return Ok(view);
    }
    let kind = if subject == "backup" {
        crate::sand_store::SandKind::Configuration
    } else if subject == "access-control" {
        crate::sand_store::SandKind::AccessControl
    } else {
        crate::sand_store::SandKind::Organ
    };
    let owner = crate::sand_store::spawn_scoped_sand(
        world,
        root,
        workspace,
        kind,
        DVec2::new(800.0, 0.0),
        source,
    );
    own(world, root, owner, Role::Feature);
    if subject == "discovery" {
        crate::organ_castle::Command::Page(6).apply(world, owner);
    }
    if subject == "contacts" {
        let data = world
            .get::<Practice>(root)
            .unwrap()
            .community
            .data
            .clone()
            .unwrap();
        crate::organ_castle::prepare_pairing(
            world,
            owner,
            data["invite"].as_str().unwrap(),
            "Practice shared Organ",
        );
    }
    Ok(owner)
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    if !matches!(
        operation,
        Operation::InspectOrgan
            | Operation::SwitchOrgan
            | Operation::PairContact
            | Operation::InspectAccess
            | Operation::ShareRecords
            | Operation::InspectDevices
            | Operation::InspectMail
            | Operation::SearchDiscovery
            | Operation::PrepareAnnouncement
            | Operation::OpenDiscoveryRequest
            | Operation::SendPracticeMessage
            | Operation::InspectCalls
            | Operation::CheckTransfer
            | Operation::AgreeTransfer
            | Operation::ActivateTransfer
            | Operation::PauseTransferRule
            | Operation::PreviewImportConflict
            | Operation::CancelImportPreview
            | Operation::PreviewCleanImport
            | Operation::ImportPreparedRecords
            | Operation::ExportPracticeFiles
            | Operation::EditPracticeFile
            | Operation::StopPracticeSync
            | Operation::SendPracticeCopy
            | Operation::AcceptPracticeCopy
            | Operation::InspectBackup
    ) {
        return Err("This example is unavailable. Skip or Close to keep reading.".into());
    }
    let Some(data) = world.get::<Practice>(root).unwrap().community.data.clone() else {
        return Ok(());
    };
    let owner = find(world, root, Role::Feature).ok_or("Wait for the sample view, then Retry.")?;
    match operation {
        Operation::SendPracticeCopy => {
            let started = world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true);
            if !started {
                crate::sync_castle::send_copy(world, owner);
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::AcceptPracticeCopy => {
            let peer_source = data["source"].as_str().unwrap();
            let peer =
                if crate::practice_cells::source(world, owner).as_deref() == Some(peer_source) {
                    owner
                } else {
                    let peer = find(world, root, Role::Auxiliary)
                        .ok_or("The prepared receiving view is unavailable.")?;
                    own(world, root, owner, Role::Auxiliary);
                    own(world, root, peer, Role::Feature);
                    peer
                };
            crate::sync_castle::accept_copy(world, peer);
        }
        Operation::InspectBackup => crate::configuration::inspect_storage(world, owner),
        Operation::ExportPracticeFiles
        | Operation::EditPracticeFile
        | Operation::StopPracticeSync => {
            disk::execute(world, root, owner, &data, operation)?;
        }
        Operation::PreviewImportConflict | Operation::PreviewCleanImport => {
            let owner = if operation == Operation::PreviewCleanImport {
                let source = data["source"].as_str().unwrap();
                if crate::practice_cells::source(world, owner).as_deref() != Some(source) {
                    own(world, root, owner, Role::Auxiliary);
                    let workspace = world.get::<Practice>(root).unwrap().workspace;
                    let clean = super::super::spawn(
                        world,
                        root,
                        workspace,
                        DVec2::new(1580.0, 0.0),
                        default(),
                    );
                    world
                        .entity_mut(clean)
                        .insert(crate::practice_cells::PracticeSource(source.into()));
                    own(world, root, clean, Role::Feature);
                    clean
                } else {
                    owner
                }
            } else {
                owner
            };
            let conflict = operation == Operation::PreviewImportConflict;
            if !super::super::import_ui::preview_ready(world, owner, conflict)
                && !world
                    .get::<Practice>(root)
                    .unwrap()
                    .results
                    .get(&operation)
                    .is_some_and(|result| result["started"] == true)
            {
                super::super::import_ui::ImportAction::Preview.apply(world, owner);
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::CancelImportPreview => {
            super::super::import_ui::ImportAction::Cancel.apply(world, owner);
        }
        Operation::ImportPreparedRecords => {
            if !super::super::import_ui::imported(world, owner)
                && !world
                    .get::<Practice>(root)
                    .unwrap()
                    .results
                    .get(&operation)
                    .is_some_and(|result| result["started"] == true)
            {
                super::super::import_ui::ImportAction::Commit.apply(world, owner);
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::CheckTransfer
        | Operation::AgreeTransfer
        | Operation::ActivateTransfer
        | Operation::PauseTransferRule => commitments::execute(world, owner, &data, operation),
        Operation::SearchDiscovery
        | Operation::PrepareAnnouncement
        | Operation::OpenDiscoveryRequest => {
            crate::organ_castle::Command::Page(6).apply(world, owner);
            let (command, fields) = match operation {
                Operation::SearchDiscovery => (
                    "search",
                    vec![
                        ("/request/query/text", json!("bicycle")),
                        ("/request/services", json!([data["endpoint"]])),
                    ],
                ),
                Operation::PrepareAnnouncement => (
                    "save-draft",
                    vec![
                        ("/request/draft/title", json!("My practice Contribution")),
                        ("/request/draft/direction", json!("contribution")),
                        (
                            "/request/draft/text",
                            json!("This draft stays in the isolated practice Cell."),
                        ),
                    ],
                ),
                _ => (
                    "open-request",
                    vec![
                        (
                            "/request/text",
                            json!("Hello from the prepared practice Cell."),
                        ),
                        ("/request/services", json!([data["endpoint"]])),
                    ],
                ),
            };
            if crate::organ_castle::social_result(world, owner, command).is_none()
                && !world
                    .get::<Practice>(root)
                    .unwrap()
                    .results
                    .get(&operation)
                    .is_some_and(|result| result["started"] == true)
                && crate::organ_castle::social_form(world, owner, command, &fields, true)
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::SendPracticeMessage => {
            let uid = data["conversation"].as_str().unwrap();
            let source = crate::protein_area::Source::Organ(
                world.get::<Practice>(root).unwrap().source.clone(),
            );
            if !crate::thread_castle::message_visible(
                world,
                uid,
                &source,
                "Hello from Instinct practice.",
            ) && !world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true)
                && crate::thread_castle::prepare_message(
                    world,
                    uid,
                    &source,
                    "Hello from Instinct practice.",
                    true,
                )
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::InspectCalls => {}
        Operation::InspectOrgan => crate::organ_castle::Command::Page(0).apply(world, owner),
        Operation::InspectDevices => crate::organ_castle::Command::Page(4).apply(world, owner),
        Operation::InspectMail => {
            crate::organ_castle::Command::Page(5).apply(world, owner);
            if crate::organ_castle::form_result(world, owner, "mailbox-outbound").is_none()
                && !world
                    .get::<Practice>(root)
                    .unwrap()
                    .results
                    .get(&operation)
                    .is_some_and(|result| result["started"] == true)
                && crate::organ_castle::submit_form(world, owner, "mailbox-outbound", &[])
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::PairContact => {
            if !world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true)
            {
                crate::organ_castle::submit_pairing(world, owner);
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::InspectAccess => {
            if !crate::access_control::user_visible(world, owner, data["user"].as_str().unwrap()) {
                crate::access_control::inspect_user(world, owner, data["user"].as_str().unwrap());
            }
            if !world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true)
                && crate::access_control::preview_authority(
                    world,
                    owner,
                    data["record"].as_str().unwrap(),
                )
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
        }
        Operation::ShareRecords => {
            let peer = data["peer"].as_str().unwrap();
            if !crate::organ_castle::select_contact(world, owner, peer) {
                return Ok(());
            }
            let started = world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true);
            if !started
                && crate::organ_castle::submit_form(
                    world,
                    owner,
                    "set-sync-policy",
                    &[("/sync_out", json!(true)), ("/sync_in", json!(false))],
                )
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .insert(operation, json!({"started":true}));
            }
            let synced = world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["synced"] == true);
            if crate::organ_castle::feed_enabled(world, owner, peer)
                && !synced
                && crate::organ_castle::submit_form(world, owner, "sync-now", &[])
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .entry(operation)
                    .or_default()["synced"] = json!(true);
            }
            if crate::organ_castle::form_result(world, owner, "sync-now").is_some()
                && find(world, root, Role::Record).is_none()
            {
                let source = data["source"].as_str().unwrap().to_owned();
                let uid = data["record"].as_str().unwrap();
                let view = crate::full_record::open(
                    world,
                    root,
                    uid,
                    crate::protein_area::Source::Organ(source.clone()),
                );
                if let Some(view) = view {
                    world
                        .entity_mut(view)
                        .insert(crate::practice_cells::PracticeSource(source));
                    own(world, root, view, Role::Record);
                }
            }
        }
        Operation::SwitchOrgan => {
            let source = data["source"].as_str().unwrap();
            if crate::practice_cells::source(world, owner).as_deref() != Some(source) {
                own(world, root, owner, Role::Auxiliary);
                let workspace = world.get::<Practice>(root).unwrap().workspace;
                let peer = crate::sand_store::spawn_scoped_sand(
                    world,
                    root,
                    workspace,
                    crate::sand_store::SandKind::Organ,
                    DVec2::new(1560.0, 0.0),
                    source.into(),
                );
                own(world, root, peer, Role::Feature);
                crate::organ_castle::Command::Page(0).apply(world, peer);
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    let Some(owner) = find(world, root, Role::Feature) else {
        return false;
    };
    let Some(data) = world.get::<Practice>(root).unwrap().community.data.clone() else {
        return false;
    };
    match operation {
        Operation::SendPracticeCopy => find(world, root, Role::Auxiliary).is_some_and(|peer| {
            crate::sync_castle::copy_state(world, peer, "incoming", "offered").is_some()
        }),
        Operation::AcceptPracticeCopy => {
            crate::sync_castle::copy_state(world, owner, "incoming", "completed").is_some()
        }
        Operation::InspectBackup => crate::configuration::storage_visible(world, owner),
        Operation::ExportPracticeFiles
        | Operation::EditPracticeFile
        | Operation::StopPracticeSync => disk::complete(world, owner, operation),
        Operation::PreviewImportConflict => {
            super::super::import_ui::preview_ready(world, owner, true)
        }
        Operation::PreviewCleanImport => {
            super::super::import_ui::preview_ready(world, owner, false)
        }
        Operation::CancelImportPreview => super::super::import_ui::cancelled(world, owner),
        Operation::ImportPreparedRecords => super::super::import_ui::imported(world, owner),
        Operation::CheckTransfer
        | Operation::AgreeTransfer
        | Operation::ActivateTransfer
        | Operation::PauseTransferRule => commitments::complete(world, owner, &data, operation),
        Operation::SearchDiscovery => crate::organ_castle::social_result(world, owner, "search")
            .is_some_and(|result| {
                result["results"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty())
            }),
        Operation::PrepareAnnouncement => {
            crate::organ_castle::social_result(world, owner, "save-draft")
                .is_some_and(|result| result["record"].is_string())
        }
        Operation::OpenDiscoveryRequest => {
            crate::organ_castle::social_result(world, owner, "open-request").is_some_and(|result| {
                result["conversation"].is_string() || result["record"].is_string()
            })
        }
        Operation::SendPracticeMessage => crate::thread_castle::message_visible(
            world,
            data["conversation"].as_str().unwrap(),
            &crate::protein_area::Source::Organ(
                world.get::<Practice>(root).unwrap().source.clone(),
            ),
            "Hello from Instinct practice.",
        ),
        Operation::InspectCalls => crate::thread_castle::thread_visible(
            world,
            data["conversation"].as_str().unwrap(),
            &crate::protein_area::Source::Organ(
                world.get::<Practice>(root).unwrap().source.clone(),
            ),
        ),
        Operation::InspectDevices => crate::organ_castle::roster_visible(world, owner),
        Operation::InspectMail => {
            crate::organ_castle::page_visible(world, owner, 5)
                && crate::organ_castle::form_result(world, owner, "mailbox-outbound").is_some_and(
                    |result| {
                        result["saved_outgoing"].as_array().is_some_and(|rows| {
                            rows.iter().any(|row| row["uid"] == data["envelope"])
                        })
                    },
                )
        }
        Operation::InspectOrgan => {
            crate::organ_castle::page_visible(world, owner, 0)
                && crate::configuration::loaded_in(world, owner)
        }
        Operation::SwitchOrgan => {
            crate::practice_cells::source(world, owner).as_deref() == data["source"].as_str()
                && crate::organ_castle::page_visible(world, owner, 0)
                && crate::configuration::loaded_in(world, owner)
        }
        Operation::PairContact => {
            crate::organ_castle::has_contact(world, owner, data["contact"].as_str().unwrap())
        }
        Operation::InspectAccess => {
            crate::access_control::user_visible(world, owner, data["user"].as_str().unwrap())
                && crate::access_control::authority_visible(world, owner)
        }
        Operation::ShareRecords => {
            let source =
                crate::protein_area::Source::Organ(data["source"].as_str().unwrap().into());
            let uid = data["record"].as_str().unwrap();
            world
                .query::<(&crate::protein_area::RecordBinding, &RecordProperties)>()
                .iter(world)
                .any(|(binding, properties)| {
                    binding.source == source
                        && binding.uid == uid
                        && properties.0["head"] == "A shared practice note"
                })
        }
        _ => false,
    }
}
