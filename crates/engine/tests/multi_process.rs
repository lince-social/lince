//! The multi-process harness (Ontology §11, decision 7 / cluster C0).
//!
//! Every other test in this workspace runs its Cells inside ONE process, and
//! `nucleus::hlc` is a process-wide atomic by design — "one clock per Cell".
//! So in-process tests share a counter between Cells that would not share one
//! in deployment, and they therefore prove a weaker statement than the one the
//! op log relies on. This file spawns real processes.
//!
//! It exists before the enrolment client for that reason: enrolment is what
//! makes a second Cell possible, and shipping it with no way to test what it
//! arms would undo the whole point of doing the Organ/Cell split first.

use std::path::PathBuf;
use std::process::Command;

fn worker(db: &std::path::Path, prefix: &str, writes: usize) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cell_worker"));
    command.arg(db).arg(prefix).arg(writes.to_string());
    command
}

fn temp_db(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "lince-harness-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    path
}

/// Removes the database when it goes out of scope — INCLUDING on panic, which
/// is exactly when a test database is most likely to be left behind and least
/// likely to be noticed. `/tmp` is a tmpfs here, so a suite that leaks one file
/// per failing run eventually fills it and every later test dies with "database
/// or disk is full" — which looks like anything except the actual cause.
struct TempDb(PathBuf);

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

/// One process at a time, against the same database — the sanity case that
/// proves the harness itself works before it is used to make claims.
#[test]
fn a_worker_process_writes_and_the_next_one_sees_it() {
    let guard = TempDb(temp_db("sequential"));
    let db = &guard.0;
    for prefix in ["first", "second"] {
        let status = worker(db, prefix, 5).status().expect("spawn");
        assert!(status.success(), "{prefix} worker failed");
    }

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let engine = engine::Engine::open(&format!("sqlite://{}", db.display()))
            .await
            .expect("open");
        let ops = store::sync_ops::all_by_hlc(&engine.store.pool)
            .await
            .expect("ops");
        for prefix in ["first", "second"] {
            assert!(
                store::records::resolve(&engine.store.pool, &format!("{prefix}-4"))
                    .await
                    .expect("resolve")
                    .is_some(),
                "{prefix} worker's last write is present"
            );
        }
        assert!(!ops.is_empty());
    });
}

/// TWO processes writing the same database at once.
///
/// This is not a contrived topology: it is what happens when the CLI touches
/// the database while the web Cell is running, which SQLite in WAL mode
/// permits. Both processes resolve to the SAME Cell Record — one database, one
/// fixed `local-cell` slug — while each runs its own `nucleus::hlc` static.
/// Op identity is `(actor_cell, hlc)`, so two independent clocks under one
/// actor is precisely the collision the Organ/Cell split exists to prevent,
/// arriving by a door the split does not cover.
///
/// The assertion is about DATA, not about mechanism: every write either lands
/// or the process fails. A write that is silently swallowed by
/// `INSERT OR IGNORE` on a duplicate identity is the failure this harness was
/// built to make visible.
#[test]
fn two_processes_sharing_one_database_lose_no_writes() {
    let guard = TempDb(temp_db("concurrent"));
    let db = &guard.0;
    // Create the database first, so both workers race on writes rather than on
    // running migrations.
    assert!(
        worker(db, "seed", 1).status().expect("spawn").success(),
        "seed worker failed"
    );

    let writes = 40;
    let children: Vec<_> = ["alpha", "beta"]
        .into_iter()
        .map(|prefix| {
            (
                prefix,
                worker(db, prefix, writes).spawn().expect("spawn"),
            )
        })
        .collect();
    for (prefix, mut child) in children {
        let status = child.wait().expect("wait");
        assert!(status.success(), "{prefix} worker failed");
    }

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let engine = engine::Engine::open(&format!("sqlite://{}", db.display()))
            .await
            .expect("open");
        let mut missing_rows = Vec::new();
        let mut missing_ops = Vec::new();
        for prefix in ["alpha", "beta"] {
            for n in 0..writes {
                let slug = format!("{prefix}-{n}");
                let Some(row) = store::records::resolve(&engine.store.pool, &slug)
                    .await
                    .expect("resolve")
                else {
                    missing_rows.push(slug);
                    continue;
                };
                // The ROW is written by an ordinary INSERT and would survive a
                // lost op, so checking rows alone proves nothing. The op is
                // what syncs, and a dropped op is a write that exists here and
                // reaches nobody — silent, and permanent.
                if store::sync_ops::for_field(&engine.store.pool, "record", &row.uid, "head")
                    .await
                    .expect("ops")
                    .is_empty()
                {
                    missing_ops.push(slug);
                }
            }
        }
        assert!(
            missing_rows.is_empty(),
            "records vanished across the process boundary: {missing_rows:?}"
        );
        assert!(
            missing_ops.is_empty(),
            "{} writes exist locally but produced NO op, so they sync to \
             nobody — two processes minted colliding `(actor_cell, hlc)` \
             identities and `INSERT OR IGNORE` swallowed the loser: {:?}",
            missing_ops.len(),
            &missing_ops[..missing_ops.len().min(5)]
        );

        // And the log agrees with the read model afterwards.
        let audit = engine.audit_read_model().await.expect("audit");
        assert!(
            audit.is_clean(),
            "read model drifted from the log: {:?}",
            audit.diverged
        );
    });
}

/// Sibling sync ACROSS OS PROCESSES — the box C3 opened and C0's harness was
/// built for.
///
/// Everything else about siblings is tested in one process with two Engines
/// and two real endpoints, which is honest as far as it goes and stops exactly
/// where it matters: `nucleus::hlc` is a process-wide atomic, so those two
/// Cells share the clock that deployment gives them separately. Here they do
/// not. Two databases, two endpoints, two processes, two clocks.
#[test]
fn a_second_process_enrols_and_converges() {
    let host_db = temp_db("sibling-host");
    let join_db = temp_db("sibling-join");
    let _host_guard = TempDb(host_db.clone());
    let _join_guard = TempDb(join_db.clone());
    let root_key = temp_db("sibling-root");
    let _root_guard = TempDb(root_key.clone());
    let code_file = temp_db("sibling-code");
    let _code_guard = TempDb(code_file.clone());

    let mut host = Command::new(env!("CARGO_BIN_EXE_cell_worker"))
        .arg("host")
        .arg(&host_db)
        .arg(&root_key)
        .arg(&code_file)
        .spawn()
        .expect("the host process starts");
    // The code file appears only after the endpoint is serving, which is what
    // makes this a wait for readiness rather than a sleep and a hope.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !code_file.exists() {
        if std::time::Instant::now() > deadline {
            let _ = host.kill();
            panic!("the host process never published an enrolment code");
        }
        if let Ok(Some(status)) = host.try_wait() {
            panic!("the host process exited early: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let joined = Command::new(env!("CARGO_BIN_EXE_cell_worker"))
        .arg("join")
        .arg(&join_db)
        .arg(&code_file)
        .status()
        .expect("the joining process runs");
    let _ = host.kill();
    let _ = host.wait();
    assert!(joined.success(), "the second process failed to join and sync");

    // What the second process now holds, read from its own database.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let engine = engine::Engine::open(&format!("sqlite://{}", join_db.display()))
            .await
            .expect("the joined database opens");
        let landed = store::records::resolve(&engine.store.pool, "written-on-the-host")
            .await
            .expect("resolve");
        assert!(
            landed.is_some(),
            "a Record written by the host BEFORE this Cell existed must arrive \
             through sibling sync"
        );
        let organ = store::organs::local(&engine.store.pool)
            .await
            .expect("organ")
            .expect("local organ");
        assert_eq!(
            landed.expect("record").organ_uid,
            Some(organ.uid),
            "and belong to the identity both processes now share"
        );
    });
}
