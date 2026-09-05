use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let result = match args.get(1).map(String::as_str) {
        Some("host") => match args.as_slice() {
            [_, _, db, root_key, code_out] => runtime.block_on(host(db, root_key, code_out)),
            _ => usage(),
        },
        Some("join") => match args.as_slice() {
            [_, _, db, code_file] => runtime.block_on(join(db, code_file)),
            _ => usage(),
        },
        _ => match args.as_slice() {
            [_, db, prefix, writes] => match writes.parse() {
                Ok(writes) => runtime.block_on(run(db, prefix, writes)),
                Err(_) => usage(),
            },
            _ => usage(),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cell_worker: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> Result<(), String> {
    Err(
        "usage: cell_worker <db> <prefix> <writes> | host <db> <root-key> <code-out> \
         | join <db> <code-file>"
            .into(),
    )
}

async fn run(db: &str, prefix: &str, writes: usize) -> Result<(), String> {
    let engine = engine::Engine::open(&format!("sqlite://{db}"))
        .await
        .map_err(|e| e.to_string())?;
    for n in 0..writes {
        store::records::create(
            &engine.store.pool,
            store::records::NewRecord {
                slug: Some(&format!("{prefix}-{n}")),
                kind: nucleus::RecordKind::Plain,
                head: &format!("{prefix} {n}"),
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .map_err(|e| format!("write {n}: {e}"))?;
    }
    Ok(())
}

fn loopback_addrs(wire: &engine::wire::Wire) -> Vec<String> {
    wire.endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| format!("127.0.0.1:{}", addr.port()))
        .collect()
}

async fn host(db: &str, root_key: &str, code_out: &str) -> Result<(), String> {
    let engine = Arc::new(
        engine::Engine::open(&format!("sqlite://{db}"))
            .await
            .map_err(|e| e.to_string())?,
    );
    let organ = store::organs::ensure_local(&engine.store.pool, "http://host.test")
        .await
        .map_err(|e| e.to_string())?
        .uid;
    let root = engine::trust::Signer::load_or_create(
        std::path::Path::new(root_key),
        &organ,
        engine::roster::ROOT_KEY_ID,
    )
    .map_err(|e| e.to_string())?;
    engine.set_root_key_path(std::path::PathBuf::from(root_key));
    engine
        .publish_root_key(&root)
        .await
        .map_err(|e| e.to_string())?;

    let secret = iroh::SecretKey::from_bytes(&[7u8; 32]);
    let wire = Arc::new(
        engine::wire::Wire::bind(engine.clone(), secret, engine::wire::Reach::Local)
            .await
            .map_err(|e| e.to_string())?,
    );
    let this_cell = store::cells::local(&engine.store.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no Cell Record")?;
    engine
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: this_cell.uid,
                node_id: wire.node_id().to_string(),
                label: "the host".into(),
                operational_key: "k-host".into(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .map_err(|e| e.to_string())?;

    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some("written-on-the-host"),
            kind: nucleus::RecordKind::Plain,
            head: "From the host process",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let token = engine
        .issue_enrolment_token()
        .await
        .map_err(|e| e.to_string())?;
    let invite = engine::pairing::EnrolmentInvite {
        node_id: wire.node_id().to_string(),
        organ_uid: organ.clone(),
        root_key: root.public_key_b64(),
        token,
        addrs: loopback_addrs(&wire),
    };
    let temporary = format!("{code_out}.partial");
    std::fs::write(&temporary, invite.encode()).map_err(|e| e.to_string())?;
    let serving = tokio::spawn(async move { wire.serve().await });
    std::fs::rename(&temporary, code_out).map_err(|e| e.to_string())?;
    let _ = serving.await;
    Ok(())
}

async fn join(db: &str, code_file: &str) -> Result<(), String> {
    let engine = Arc::new(
        engine::Engine::open(&format!("sqlite://{db}"))
            .await
            .map_err(|e| e.to_string())?,
    );
    store::organs::ensure_local(&engine.store.pool, "http://joiner.test")
        .await
        .map_err(|e| e.to_string())?;
    let code = std::fs::read_to_string(code_file).map_err(|e| e.to_string())?;
    let invite =
        engine::pairing::EnrolmentInvite::decode(code.trim()).map_err(|e| e.to_string())?;

    let secret = iroh::SecretKey::from_bytes(&[8u8; 32]);
    let wire = engine::wire::Wire::bind(engine.clone(), secret, engine::wire::Reach::Local)
        .await
        .map_err(|e| e.to_string())?;
    wire.enrol(&invite).await.map_err(|e| e.to_string())?;
    wire.sync_once().await.map_err(|e| e.to_string())?;
    Ok(())
}
