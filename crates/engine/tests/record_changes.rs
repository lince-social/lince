use engine::Engine;
use engine::sync::Delivery;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::record_changes::{self, Cause};
use store::records::NewRecord;

async fn cell() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
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

async fn paired() -> (Engine, String, Engine, String) {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, false)
        .await
        .unwrap();
    (a, a_organ, b, b_organ)
}

async fn sync(from: &Engine, to: &Engine) {
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => to.import_grant_batch(&root, &batch).await,
            None => to.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain");
}

async fn remote_entry(e: &Engine, uid: &str, field: &str) -> record_changes::Change {
    record_changes::recent(&e.store.pool, uid, 50)
        .await
        .expect("changes")
        .into_iter()
        .find(|c| c.cause == Cause::Remote && c.field == field)
        .expect("an overwrite by a remote op left an entry")
}

#[tokio::test]
async fn a_losing_local_edit_leaves_an_entry_naming_the_winning_organ() {
    let (a, a_organ, b, _b_organ) = paired().await;

    let uid = plain(&a, "apples").await;
    sync(&a, &b).await;

    store::records::set_slug(&b.store.pool, &uid, Some("what-b-typed"))
        .await
        .expect("B edits");

    store::records::set_slug(&a.store.pool, &uid, Some("what-a-typed"))
        .await
        .expect("A edits");
    sync(&a, &b).await;

    assert_eq!(
        store::records::get(&b.store.pool, &uid)
            .await
            .expect("get")
            .expect("record")
            .slug
            .as_deref(),
        Some("what-a-typed"),
        "A's later HLC won, which is the merge behaving correctly"
    );

    let lost = remote_entry(&b, &uid, "slug").await;
    assert_eq!(
        lost.winner_organ.as_deref(),
        Some(a_organ.as_str()),
        "the entry names the Organ whose op won"
    );
    assert_eq!(
        lost.displaced.as_deref(),
        Some("what-b-typed"),
        "and says what was displaced, or it cannot be recovered by hand"
    );
    assert!(
        lost.displaced_local,
        "what was overwritten was authored HERE, which is what makes it worth \
         telling this person about"
    );
}

#[tokio::test]
async fn an_overwrite_of_a_value_we_never_authored_is_not_marked_local() {
    let (a, _a_organ, b, _b_organ) = paired().await;

    let uid = plain(&a, "pears").await;
    sync(&a, &b).await;

    store::records::set_slug(&a.store.pool, &uid, Some("a-again"))
        .await
        .expect("A edits");
    sync(&a, &b).await;

    let entry = remote_entry(&b, &uid, "slug").await;
    assert!(
        !entry.displaced_local,
        "B never typed into this field, so nothing of B's was lost"
    );
}

#[tokio::test]
async fn an_arriving_op_that_loses_leaves_no_entry() {
    let (a, _a_organ, b, _b_organ) = paired().await;

    let uid = plain(&a, "plums").await;
    sync(&a, &b).await;

    store::records::set_slug(&a.store.pool, &uid, Some("a-first"))
        .await
        .expect("A edits");
    sync(&a, &b).await;

    store::records::set_slug(&b.store.pool, &uid, Some("b-later"))
        .await
        .expect("B edits later");

    let before = record_changes::recent(&b.store.pool, &uid, 50)
        .await
        .expect("changes")
        .len();

    sync(&a, &b).await;

    assert_eq!(
        store::records::get(&b.store.pool, &uid)
            .await
            .expect("get")
            .expect("record")
            .slug
            .as_deref(),
        Some("b-later"),
        "B's edit was later and stands"
    );
    assert_eq!(
        record_changes::recent(&b.store.pool, &uid, 50)
            .await
            .expect("changes")
            .len(),
        before,
        "a stale arriving op changed nothing, so it reported nothing"
    );
}
