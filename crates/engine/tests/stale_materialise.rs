use engine::Engine;
use engine::sync::{Materialise, OpBatch, WireOp};
use engine::trust::Signer;
use store::sync_ops::OpKind;

async fn cell(url: &str) -> (Engine, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, url)
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (engine, organ)
}

fn op(uid: &str, field: &str, value: &str, hlc: i64, organ: &str, cell: &str) -> WireOp {
    WireOp {
        tbl: "record".into(),
        uid: uid.into(),
        field: field.into(),
        kind: "set".into(),
        value: Some(serde_json::json!(value).to_string()),
        hlc,
        actor_cell: cell.into(),
        organ_uid: organ.into(),
        fact: None,
    }
}

async fn head_of(engine: &Engine, uid: &str) -> String {
    store::records::get(&engine.store.pool, uid)
        .await
        .expect("read")
        .expect("record")
        .head
}

#[tokio::test]
async fn a_materialise_of_an_op_the_log_has_already_superseded_is_refused() {
    let (engine, _local) = cell("http://guard").await;
    let them = "organ_them";
    store::organs::add_contact(&engine.store.pool, them, None, "Them", "", 0)
        .await
        .expect("contact");

    let uid = "rec_guarded";
    let loser = op(uid, "head", "older", 10, them, "cell_them");
    let winner = op(uid, "head", "newer", 20, them, "cell_them");

    engine
        .import_op_batch(&OpBatch {
            from_organ: them.to_string(),
            ops: vec![loser.clone(), winner.clone()],
        })
        .await
        .expect("import");
    assert_eq!(head_of(&engine, uid).await, "newer");

    let outcome = engine
        .materialise(Materialise {
            op: &loser,
            kind: OpKind::Set,
            replica_root: None,
            undelete: false,
        })
        .await
        .expect("materialise runs");
    assert!(outcome.is_some(), "the pair is one the read model owns");

    assert_eq!(
        head_of(&engine, uid).await,
        "newer",
        "the database refused the stale write rather than the process remembering not to make it"
    );

    let report = engine.audit_read_model().await.expect("audit");
    assert!(
        report.is_clean(),
        "the read model still agrees with the log: {:?}",
        report.diverged
    );
}

#[tokio::test]
async fn the_winner_still_applies_when_nothing_supersedes_it() {
    let (engine, _local) = cell("http://guard-b").await;
    let them = "organ_them";
    store::organs::add_contact(&engine.store.pool, them, None, "Them", "", 0)
        .await
        .expect("contact");

    let uid = "rec_plain";
    engine
        .import_op_batch(&OpBatch {
            from_organ: them.to_string(),
            ops: vec![op(uid, "head", "only", 10, them, "cell_them")],
        })
        .await
        .expect("import");

    assert_eq!(head_of(&engine, uid).await, "only");
}
