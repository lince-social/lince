//! The shared half of C7: which Cell does a Record's recurring work.
//!
//! These cover `runs_here`, the predicate every outward-reaching scheduler asks
//! before doing anything. The Karma side of the same designation is tested in
//! `karma_runs.rs` against the freeze query; this file tests the helper itself,
//! because transfer delivery retries reach it through no SQL of their own.
//!
//! The multi-Cell half — that exactly one of three Cells actually sends, and
//! that an unreachable holder does not hand the work to whoever cannot see it —
//! is `engine/tests/dst_deferred.rs` and needs Resenha.

use store::Store;

async fn record(store: &Store, slug: &str) -> String {
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    let uid = nucleus::new_uid("r");
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, 'thing', ?, '', '1', 0, ?, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
    )
    .bind(&uid)
    .bind(slug)
    .bind(slug)
    .bind(&organ.uid)
    .execute(&store.pool)
    .await
    .unwrap();
    uid
}

/// Absence means yes. An Organ that never designates anything keeps behaving
/// exactly as it did before the mechanism existed — the same direction the
/// local axis fails in, and for the same reason: work that quietly stopped
/// everywhere is visible nowhere.
#[tokio::test]
async fn an_undesignated_record_runs_here() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "undesignated").await;
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

/// The filter tests EQUALITY, not the mere presence of a designation —
/// designating the Cell you are sitting at must not stop it working.
#[tokio::test]
async fn a_record_designated_to_this_cell_runs_here() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    let cell = store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "mine").await;

    store::executor::designate(&store.pool, &uid, Some(&cell.uid))
        .await
        .unwrap();
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

/// The whole point: designated elsewhere, this Cell stands down.
#[tokio::test]
async fn a_record_designated_to_another_cell_does_not_run_here() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "theirs").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    assert!(!store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

/// Clearing hands the work back to every Cell, and clearing is the ONLY way
/// back — there is no expiry, because the lease is a value rather than a claim.
#[tokio::test]
async fn clearing_a_designation_returns_the_work_to_every_cell() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "handed-back").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    store::executor::designate(&store.pool, &uid, None)
        .await
        .unwrap();

    assert_eq!(
        store::executor::designated(&store.pool, &uid)
            .await
            .unwrap(),
        None
    );
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

/// A designation exists and this Cell cannot say who it is: it is NOT the one.
///
/// This is the one place the mechanism fails toward silence rather than toward
/// duplication, and it is deliberate. Designating means "exactly one Cell", and
/// a Cell guessing it might be that one is precisely how the duplicate gets
/// made. The state should not occur — `cells::ensure_local` runs at startup —
/// so answering it strictly costs nothing real.
#[tokio::test]
async fn a_cell_that_cannot_identify_itself_is_not_the_designated_one() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "unidentified").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    assert!(!store::executor::runs_here(&store.pool, &uid).await.unwrap());
}
