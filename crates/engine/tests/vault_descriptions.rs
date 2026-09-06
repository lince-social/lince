use {
    engine::{Engine, actions::Action},
    nucleus::RecordKind,
    store::records::NewRecord,
    utils::vault,
};

const PASSWORD: &str = "correct horse battery staple";
const SECRET: &str = "sk-a-provider-token-nobody-else-may-read";

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
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
    .expect("record")
    .uid
}

#[tokio::test]
async fn a_locked_description_is_stored_and_synced_as_ciphertext_only() {
    let e = engine().await;
    let uid = plain(&e, "vault").await;
    let locked = vault::lock(&uid, PASSWORD, SECRET).expect("lock");

    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: None,
            body: Some(locked.clone()),
        },
        None,
    )
    .await
    .expect("edit");

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert!(vault::is_locked(&row.body));
    assert!(!row.body.contains(SECRET));
    assert_eq!(vault::unlock(&uid, PASSWORD, &row.body).unwrap(), SECRET);

    let ops = store::sync_ops::all_by_hlc(&e.store.pool)
        .await
        .expect("ops");
    for op in &ops {
        let value = op.value.clone().unwrap_or_default();
        assert!(!value.contains(SECRET), "an op carried the plaintext");
    }
    let facts = store::facts::for_record(&e.store.pool, &uid, 100)
        .await
        .expect("facts");
    for fact in &facts {
        let payload = fact.payload.clone().unwrap_or_default();
        assert!(!payload.contains(SECRET), "a Fact carried the plaintext");
    }
}

#[tokio::test]
async fn a_new_password_protects_the_new_description_and_not_the_old_one() {
    let e = engine().await;
    let uid = plain(&e, "vault-rotated").await;
    let first = vault::lock(&uid, PASSWORD, SECRET).expect("lock");
    let second = vault::lock(&uid, "a different password", "a rewritten secret").expect("relock");

    for body in [first.clone(), second.clone()] {
        e.act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some(body),
            },
            None,
        )
        .await
        .expect("edit");
    }

    assert_eq!(
        vault::unlock(&uid, "a different password", &second).unwrap(),
        "a rewritten secret"
    );
    assert_eq!(
        vault::unlock(&uid, PASSWORD, &first).unwrap(),
        SECRET,
        "an older revision keeps the password it was saved with"
    );
    assert!(vault::unlock(&uid, PASSWORD, &second).is_err());
}
