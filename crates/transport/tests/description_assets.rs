use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    description_asset::{Kind, Request, Response},
    drawing::Drawing,
};
use std::sync::Arc;
use transport::{ClientMessage, LaneHub, ServerMessage, Session};

#[tokio::test]
async fn transclusion_subscription_follows_edits_and_revocation_and_assets_follow_the_same_access()
{
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let uid = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some("included"),
            kind: RecordKind::Plain,
            head: "Included",
            body: "First",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some("reader"),
            kind: RecordKind::Person,
            head: "Reader",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read").await.unwrap();
    store::auth::grant(&engine.store.pool, role, permission).await.unwrap();
    store::auth::create_credential(&engine.store.pool, &person, "reader", "hash", role)
        .await
        .unwrap();
    engine.act(Action::GrantVisibility { subject_kind: "actor".into(), subject: Some(person.clone()), target: uid.clone() }, None).await.unwrap();
    let bytes = serde_json::to_vec(&Drawing::default()).unwrap();
    let Response::Stored { asset } = engine
        .description_asset(
            None,
            Request::Put {
                record: uid.clone(),
                kind: Kind::Drawing,
                data_base64: STANDARD.encode(bytes),
            },
        )
        .await
        .unwrap()
    else {
        panic!("asset")
    };
    let mut session = Session::new(
        engine.clone(),
        Arc::new(LaneHub::new()),
        "description-reader",
        Some(person.clone()),
    );
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::UidEq(uid.clone())],
        fields: Some(vec!["uid".into(), "head".into(), "body".into()]),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(1),
    };
    assert!(
        matches!(session.handle(ClientMessage::Subscribe { id: "included".into(), protein: query }).await.as_slice(), [ServerMessage::Snapshot { rows, .. }] if rows[0]["body"] == "First")
    );
    engine
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: None,
                body: Some("Second".into()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(session.refresh().await.iter().any(|message| matches!(message, ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } if rows.first().is_some_and(|row| row["body"] == "Second"))));
    let get = ClientMessage::DescriptionAsset {
        id: "asset".into(),
        request: Request::Get { record: uid, asset },
    };
    assert!(matches!(
        session.handle(get.clone()).await.as_slice(),
        [ServerMessage::DescriptionAsset { .. }]
    ));
    engine
        .set_read_filter(&person, Some(&protein::Predicate::UidEq(person.clone())))
        .await
        .unwrap();
    assert!(session.refresh().await.iter().any(|message| matches!(message, ServerMessage::Snapshot { rows, .. } | ServerMessage::Update { rows, .. } if rows.is_empty())));
    assert!(matches!(
        session.handle(get).await.as_slice(),
        [ServerMessage::Error { .. }]
    ));
}
