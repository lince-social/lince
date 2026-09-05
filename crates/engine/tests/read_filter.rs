use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, "http://filter")
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    engine
}

async fn record(engine: &Engine, slug: &str, head: &str, kind: RecordKind) -> String {
    store::records::create(
        &engine.store.pool,
        NewRecord {
            slug: Some(slug),
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|row| row.uid)
    .expect("record")
}

async fn person(engine: &Engine, slug: &str) -> String {
    let uid = record(engine, slug, slug, RecordKind::Person).await;
    let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
        .await
        .expect("role");
    store::auth::create_credential(&engine.store.pool, &uid, slug, "hash", role)
        .await
        .expect("credential");
    uid
}

fn kind_is(kind: &str) -> protein::Predicate {
    protein::Predicate::KindEq(kind.to_string())
}

#[tokio::test]
async fn no_filter_means_the_login_sees_what_it_always_saw() {
    let engine = cell().await;
    let who = person(&engine, "ana").await;
    assert!(
        engine.readable_by(&who).await.expect("readable").is_none(),
        "absent is unnarrowed, and must not be read as empty"
    );
}

#[tokio::test]
async fn a_filter_narrows_what_the_login_may_see() {
    let engine = cell().await;
    let who = person(&engine, "bea").await;
    let task = record(&engine, "a-task", "A task", RecordKind::Plain).await;
    record(&engine, "a-note", "A note", RecordKind::Organ).await;

    engine
        .act(
            Action::SetPersonReadFilter {
                person: who.clone(),
                filter: Some(kind_is("plain")),
            },
            None,
        )
        .await
        .expect("set the filter");

    let readable = engine
        .readable_by(&who)
        .await
        .expect("readable")
        .expect("a filter is set");
    assert!(readable.contains(&task));
    assert!(
        !readable.iter().any(|uid| uid != &task && uid == "a-note"),
        "a Record the filter does not admit is not readable"
    );
}

#[tokio::test]
async fn what_a_login_cannot_see_it_cannot_change_either() {
    let engine = cell().await;
    let who = person(&engine, "caio").await;
    let hidden = record(&engine, "hidden", "Hidden", RecordKind::Organ).await;

    engine
        .act(
            Action::SetPersonReadFilter {
                person: who.clone(),
                filter: Some(kind_is("plain")),
            },
            None,
        )
        .await
        .expect("set the filter");

    let refused = engine
        .act(
            Action::EditRecordText {
                target: hidden.clone(),
                head: Some("rewritten".into()),
                body: None,
            },
            Some(who.clone()),
        )
        .await;

    assert!(
        refused.is_err(),
        "a filter that only hid things on screen would be a display preference, not a boundary"
    );
    let row = store::records::get(&engine.store.pool, &hidden)
        .await
        .expect("read")
        .expect("record");
    assert_eq!(row.head, "Hidden");
}
