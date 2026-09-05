use nucleus::RecordKind;
use store::Store;
use store::record_changes::{self, Cause};
use store::records::NewRecord;

async fn cell() -> Store {
    Store::open_memory().await.expect("in-memory store")
}

async fn plain(store: &Store, slug: &str) -> String {
    store::records::create(
        &store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
}

#[tokio::test]
async fn a_local_edit_is_logged_as_local_and_names_no_winner() {
    let store = cell().await;
    let uid = plain(&store, "figs").await;

    store::records::set_slug(&store.pool, &uid, Some("typed-here"))
        .await
        .expect("edit");

    let changes = record_changes::recent(&store.pool, &uid, 20)
        .await
        .expect("changes");
    let entry = changes
        .iter()
        .find(|c| c.field == "slug")
        .expect("the local edit was logged");
    assert_eq!(entry.cause, Cause::Local);
    assert!(
        entry.winner_organ.is_none(),
        "nobody won anything: there was no race"
    );
}

#[tokio::test]
async fn entries_age_out() {
    let store = cell().await;
    let uid = plain(&store, "quinces").await;
    store::records::set_slug(&store.pool, &uid, Some("typed-here"))
        .await
        .expect("edit");

    assert!(
        !record_changes::recent(&store.pool, &uid, 20)
            .await
            .expect("changes")
            .is_empty()
    );

    let cutoff = chrono::Utc::now() + chrono::Duration::seconds(1);
    let removed = record_changes::prune_before(&store.pool, cutoff)
        .await
        .expect("prune");
    assert!(removed > 0, "the old entries were dropped");
    assert!(
        record_changes::recent(&store.pool, &uid, 20)
            .await
            .expect("changes")
            .is_empty(),
        "nothing older than the retention window survives"
    );
}

#[tokio::test]
async fn one_record_keeps_only_its_newest_entries() {
    let store = cell().await;
    let uid = plain(&store, "sloes").await;
    let other = plain(&store, "medlars").await;

    for n in 0..(record_changes::MAX_PER_RECORD + 10) {
        store::records::set_slug(&store.pool, &uid, Some(&format!("edit-{n}")))
            .await
            .expect("edit");
    }

    let changes = record_changes::recent(&store.pool, &uid, 500)
        .await
        .expect("changes");
    assert_eq!(changes.len() as i64, record_changes::MAX_PER_RECORD);
    assert_eq!(
        changes[0].field, "slug",
        "newest first, and the newest is what survives"
    );

    assert!(
        !record_changes::recent(&store.pool, &other, 20)
            .await
            .expect("changes")
            .is_empty(),
        "the cap is per Record: a hot Record must not evict a quiet one"
    );
}
