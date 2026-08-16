//! One Cell, in its own OS process, for the multi-process harness
//! (Ontology §11, decision 7 / cluster C0).
//!
//! Exists because the properties that matter most cannot be tested in one
//! process. `nucleus::hlc` is a process-wide atomic — that is its documented
//! design, "one clock per Cell" — so two Cells sharing a test process also
//! share a counter, which is exactly what masks the collisions the op log is
//! supposed to survive. Every in-process test therefore proves a WEAKER
//! statement than the one deployment relies on.
//!
//! Three modes:
//!
//! - `cell_worker <db-path> <slug-prefix> <writes>` — write N Records and
//!   exit. The original mode, and the one that found the data-loss bug.
//! - `cell_worker host <db> <root-key> <code-out>` — become an Organ, write
//!   one Record, publish an enrolment code to `<code-out>`, then serve until
//!   killed.
//! - `cell_worker join <db> <code-file>` — enrol into that Organ from the
//!   code, run ONE sync pass, and exit.
//!
//! The last two are what make sibling sync testable for real: two databases,
//! two endpoints, two `hlc` clocks that genuinely do not share an atomic.

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

/// Loopback addresses for a bound endpoint. `bound_sockets()` reports the
/// wildcard bind, so the port is pointed at 127.0.0.1 explicitly — the same
/// thing the in-process tests do, and what makes this work with no discovery.
fn loopback_addrs(wire: &engine::wire::Wire) -> Vec<String> {
    wire.endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| format!("127.0.0.1:{}", addr.port()))
        .collect()
}

/// Become an Organ of one Cell, write something worth syncing, and publish an
/// enrolment code. Serves until killed.
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
                // This worker exercises convergence, not mail.
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .map_err(|e| e.to_string())?;

    // Written BEFORE the sibling joins, so what the test proves is that a
    // second process converges on history it was not present for.
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
    // Written LAST and atomically-ish: the harness waits for this file to
    // exist, so it must not appear before the endpoint is actually serving.
    let temporary = format!("{code_out}.partial");
    std::fs::write(&temporary, invite.encode()).map_err(|e| e.to_string())?;
    let serving = tokio::spawn(async move { wire.serve().await });
    std::fs::rename(&temporary, code_out).map_err(|e| e.to_string())?;
    let _ = serving.await;
    Ok(())
}

/// Enrol into the Organ named by the code, then run one sync pass.
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
    // The point of the exercise: a sibling pass, across a process boundary,
    // pulling ops written by a Cell whose `hlc` really is a separate atomic.
    wire.sync_once().await.map_err(|e| e.to_string())?;
    Ok(())
}
