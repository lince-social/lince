use nucleus::RecordKind;
use store::{
    Store, communication,
    records::{self, NewRecord},
};

async fn tag_record(store: &Store, slug: &str) -> String {
    communication::ensure_tag_record(&store.pool, slug)
        .await
        .unwrap()
        .uid
}

async fn conversation(store: &Store, head: &str, tag_uid: &str, participants: &[&str]) -> String {
    let conv = records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    communication::tag(&store.pool, &conv.uid, tag_uid)
        .await
        .unwrap();
    for person in participants {
        let p = records::create(
            &store.pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Person,
                head: person,
                body: "",
                quantity: store::exact::one(),
            },
        )
        .await
        .unwrap();
        communication::add_participant(&store.pool, &conv.uid, &p.uid)
            .await
            .unwrap();
    }
    conv.uid
}

async fn post_message(store: &Store, conversation_uid: &str, body: &str) -> String {
    let thread_of = store::concepts::ensure(&store.pool, "thread-of")
        .await
        .unwrap();
    let message_in = store::concepts::ensure(&store.pool, "message-in")
        .await
        .unwrap();

    let thread = records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Thread,
            head: "General",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &thread.uid,
            predicate_uid: &thread_of,
            object_uid: Some(conversation_uid),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();

    let message = records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Message,
            head: "",
            body,
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();
    store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &message.uid,
            predicate_uid: &message_in,
            object_uid: Some(&thread.uid),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    message.uid
}

#[tokio::test]
async fn tag_listing_orders_by_newest_activity_with_preview_and_participants() {
    let store = Store::open_memory().await.unwrap();
    let tag = tag_record(&store, "communication").await;

    let alpha = conversation(&store, "Alpha", &tag, &["Ana", "Bea"]).await;
    post_message(&store, &alpha, "first in alpha").await;
    let beta = conversation(&store, "Beta", &tag, &["Cid"]).await;
    post_message(&store, &beta, "hello from beta").await;

    let list = communication::conversations_by_tag(&store.pool, &tag)
        .await
        .unwrap();
    assert_eq!(list.len(), 2, "both tagged conversations are listed");

    assert_eq!(list[0].record.uid, beta, "newest activity sorts first");
    assert_eq!(list[1].record.uid, alpha);

    assert_eq!(
        list[0].last_message.as_ref().map(|m| m.body.as_str()),
        Some("hello from beta")
    );
    assert_eq!(
        list[1].last_message.as_ref().map(|m| m.body.as_str()),
        Some("first in alpha")
    );

    assert_eq!(list[0].participants.len(), 1);
    assert_eq!(list[1].participants.len(), 2);
}

#[tokio::test]
async fn untagged_and_other_tag_conversations_are_excluded() {
    let store = Store::open_memory().await.unwrap();
    let comms = tag_record(&store, "communication").await;
    let other = tag_record(&store, "project-x").await;

    let mine = conversation(&store, "Mine", &comms, &[]).await;
    conversation(&store, "Theirs", &other, &[]).await;
    records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Loose",
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .unwrap();

    let list = communication::conversations_by_tag(&store.pool, &comms)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].record.uid, mine);
}

#[tokio::test]
async fn deactivated_conversation_drops_out_of_the_list() {
    let store = Store::open_memory().await.unwrap();
    let tag = tag_record(&store, "communication").await;
    let conv = conversation(&store, "Gone", &tag, &[]).await;

    assert_eq!(
        communication::conversations_by_tag(&store.pool, &tag)
            .await
            .unwrap()
            .len(),
        1
    );

    sqlx::query("UPDATE record SET quantity_mantissa = '0', quantity_scale = 0 WHERE uid = ?")
        .bind(&conv.as_str())
        .execute(&store.pool)
        .await
        .unwrap();

    assert!(
        communication::conversations_by_tag(&store.pool, &tag)
            .await
            .unwrap()
            .is_empty(),
        "deactivated conversation is not listed"
    );
}

#[tokio::test]
async fn extension_roundtrips_with_defaults() {
    let store = Store::open_memory().await.unwrap();
    let conv = conversation(
        &store,
        "Ext",
        &tag_record(&store, "communication").await,
        &[],
    )
    .await;

    assert!(
        communication::get_ext(&store.pool, &conv)
            .await
            .unwrap()
            .is_none()
    );

    let mut ext = communication::CommunicationExt::default();
    ext.room_id = "room-42".to_string();
    ext.recording_policy = "disabled".to_string();
    communication::set_ext(&store.pool, &conv, &ext)
        .await
        .unwrap();

    let loaded = communication::get_ext(&store.pool, &conv)
        .await
        .unwrap()
        .expect("extension present");
    assert_eq!(loaded, ext);
    assert_eq!(loaded.provider, "native-webrtc", "default provider");
    assert_eq!(loaded.room.state, "idle", "default room state");
}

#[tokio::test]
async fn session_open_flips_room_active_and_writes_sidecar_and_link() {
    let store = Store::open_memory().await.unwrap();
    let conv = conversation(
        &store,
        "Call",
        &tag_record(&store, "communication").await,
        &[],
    )
    .await;

    let session = communication::open_session(&store.pool, &conv, "audio+video")
        .await
        .unwrap();
    assert_eq!(session.kind, "call_session");
    assert!(session.head.starts_with("Call · "));

    let ext = communication::get_ext(&store.pool, &conv)
        .await
        .unwrap()
        .expect("ext written on open");
    assert_eq!(ext.room.state, "active");
    assert_eq!(ext.room.media, "audio+video");
    assert_eq!(
        ext.room.session_record_id.as_deref(),
        Some(session.uid.as_str())
    );

    let sidecar = communication::get_session(&store.pool, &session.uid)
        .await
        .unwrap()
        .expect("session sidecar written");
    assert_eq!(sidecar.media, "audio+video");
    assert!(sidecar.ended_at.is_none());
    assert_eq!(sidecar.recording.state, "idle");

    let sessions = communication::sessions_of(&store.pool, &conv)
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].uid, session.uid);
}

#[tokio::test]
async fn session_close_stamps_end_and_flips_room_idle() {
    let store = Store::open_memory().await.unwrap();
    let conv = conversation(
        &store,
        "Call",
        &tag_record(&store, "communication").await,
        &[],
    )
    .await;
    let session = communication::open_session(&store.pool, &conv, "audio")
        .await
        .unwrap();

    communication::close_session(&store.pool, &conv, &session.uid, 3)
        .await
        .unwrap();

    let ext = communication::get_ext(&store.pool, &conv)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ext.room.state, "idle");
    assert!(ext.room.session_record_id.is_none());
    assert!(ext.room.occupants.is_empty());

    let sidecar = communication::get_session(&store.pool, &session.uid)
        .await
        .unwrap()
        .unwrap();
    assert!(sidecar.ended_at.is_some(), "close stamps ended_at");
    assert_eq!(sidecar.peak_participants, 3);
}
