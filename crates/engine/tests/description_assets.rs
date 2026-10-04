use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{Engine, actions::Action};
use nucleus::{
    RecordKind,
    description_asset::{Kind, Request, Response},
    drawing::Drawing,
};

async fn record(engine: &Engine, kind: RecordKind, slug: &str) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind,
            head: slug,
            body: "original",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

fn put(record: &str, kind: Kind, bytes: &[u8]) -> Request {
    Request::Put {
        record: record.into(),
        kind,
        data_base64: STANDARD.encode(bytes),
    }
}

#[tokio::test]
async fn assets_are_separate_deduplicated_and_scoped_to_the_record() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine, RecordKind::Plain, "drawing-owner").await;
    let other = record(&engine, RecordKind::Plain, "other").await;
    let bytes = serde_json::to_vec(&Drawing::default()).unwrap();
    let Response::Stored { asset } = engine
        .description_asset(None, put(&uid, Kind::Drawing, &bytes))
        .await
        .unwrap()
    else {
        panic!("asset")
    };
    let Response::Stored { asset: again } = engine
        .description_asset(None, put(&uid, Kind::Drawing, &bytes))
        .await
        .unwrap()
    else {
        panic!("asset")
    };
    assert_eq!(asset, again);
    let count: i64 = store::sqlx::query_scalar("SELECT count(*) FROM description_assets")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        store::records::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .body,
        "original"
    );
    let Response::Data { data_base64, kind } = engine
        .description_asset(
            None,
            Request::Get {
                record: uid,
                asset: asset.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("bytes")
    };
    assert_eq!(kind, Kind::Drawing);
    assert_eq!(STANDARD.decode(data_base64).unwrap(), bytes);
    assert!(
        engine
            .description_asset(
                None,
                Request::Get {
                    record: other,
                    asset
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn reads_use_current_permissions_and_locked_descriptions_block_assets() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine, RecordKind::Plain, "picture").await;
    let person = record(&engine, RecordKind::Person, "viewer").await;
    let role = store::auth::ensure_role(&engine.store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    for action in ["read", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", action).await.unwrap();
        store::auth::grant(&engine.store.pool, role, permission).await.unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person, "viewer", "hash", role)
        .await
        .unwrap();
    engine.act(Action::GrantVisibility { subject_kind: "actor".into(), subject: Some(person.clone()), target: uid.clone() }, None).await.unwrap();
    let bytes = serde_json::to_vec(&Drawing::default()).unwrap();
    let Response::Stored { asset } = engine
        .description_asset(None, put(&uid, Kind::Drawing, &bytes))
        .await
        .unwrap()
    else {
        panic!("asset")
    };
    let get = Request::Get {
        record: uid.clone(),
        asset,
    };
    engine.description_asset(Some(&person), get.clone()).await.unwrap();
    engine
        .set_read_filter(&person, Some(&protein::Predicate::UidEq(person.clone())))
        .await
        .unwrap();
    assert!(
        engine
            .description_asset(Some(&person), get.clone())
            .await
            .is_err()
    );
    assert!(
        engine
            .description_asset(Some(&person), put(&uid, Kind::Drawing, &bytes))
            .await
            .is_err()
    );
    engine.set_read_filter(&person, None).await.unwrap();
    assert!(
        engine
            .description_asset(Some(&person), get.clone())
            .await
            .is_ok()
    );
    let locked = utils::vault::lock(&uid, "a long password", "secret").unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: uid,
                head: None,
                body: Some(locked),
            },
            None,
        )
        .await
        .unwrap();
    assert!(engine.description_asset(None, get).await.is_err());
}

#[tokio::test]
async fn field_write_policy_is_required_even_when_the_record_is_readable() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine, RecordKind::Plain, "policy-owner").await;
    let person = record(&engine, RecordKind::Person, "restricted-editor").await;
    let role = store::auth::ensure_role(&engine.store.pool, "description-test")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(
        &engine.store.pool,
        &person,
        "restricted-editor",
        "hash",
        role,
    )
    .await
    .unwrap();
    let policy = protein::authority::RolePolicy {
        read: protein::Predicate::UidEq(uid.clone()),
        grants: Vec::new(),
    };
    store::role_policies::set(
        &engine.store.pool,
        role,
        &serde_json::to_value(policy).unwrap(),
        0,
    )
    .await
    .unwrap();
    assert!(engine.may_read_record(Some(&person), &uid).await.unwrap());
    let bytes = serde_json::to_vec(&Drawing::default()).unwrap();
    assert!(
        engine
            .description_asset(Some(&person), put(&uid, Kind::Drawing, &bytes))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn asset_validation_rejects_bad_formats_coordinates_and_sizes() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine, RecordKind::Plain, "validation").await;
    for kind in [Kind::Drawing, Kind::Png, Kind::Webp] {
        assert!(
            engine
                .description_asset(None, put(&uid, kind, b"bad bytes"))
                .await
                .is_err()
        );
    }
    let mut drawing = Drawing::default();
    drawing.width = 4096;
    assert!(
        engine
            .description_asset(
                None,
                put(&uid, Kind::Drawing, &serde_json::to_vec(&drawing).unwrap())
            )
            .await
            .is_err()
    );
    let request = Request::Put {
        record: uid.clone(),
        kind: Kind::Drawing,
        data_base64: "A".repeat(nucleus::description_asset::MAX_BYTES.div_ceil(3) * 4 + 1),
    };
    assert!(engine.description_asset(None, request).await.is_err());
    for (kind, format) in [
        (Kind::Png, image::ImageFormat::Png),
        (Kind::Webp, image::ImageFormat::WebP),
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(64, 64))
            .write_to(&mut bytes, format)
            .unwrap();
        assert!(
            engine
                .description_asset(None, put(&uid, kind, bytes.get_ref()))
                .await
                .is_ok()
        );
        let wrong = if kind == Kind::Png {
            Kind::Webp
        } else {
            Kind::Png
        };
        assert!(
            engine
                .description_asset(None, put(&uid, wrong, bytes.get_ref()))
                .await
                .is_err()
        );
    }
}
