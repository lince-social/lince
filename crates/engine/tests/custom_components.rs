use engine::{
    actions::Action as Backend,
    custom_component::{FORMAT, MAX_BYTES},
};
use serde_json::json;

fn query(target: Option<&str>) -> protein::Protein {
    serde_json::from_value(match target {
        Some(uid) => json!({"source":"record","where":[{"uid_eq":uid}],"limit":1}),
        None => json!({"source":"record","where":[{"kind_eq":"sand"},{"text_contains":FORMAT}],"fields":["uid","head"],"limit":null}),
    }).unwrap()
}

#[tokio::test]
async fn component_records_support_rename_replacement_and_deletion() {
    let engine = engine::Engine::open_memory().await.unwrap();
    let body =
        json!({"format":FORMAT,"castle":{"name":"Board","parts":[{"label":"Before"}]}}).to_string();
    let uid = engine
        .act(
            Backend::CreateCustomComponent {
                head: "Board".into(),
                body,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let updated = json!({"format":FORMAT,"castle":{"name":"Updated","parts":[{"label":"After"}]}})
        .to_string();
    engine
        .act(
            Backend::EditRecordText {
                target: uid.clone(),
                head: Some("Updated".into()),
                body: Some(updated.clone()),
            },
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute(&engine.store, &query(Some(&uid)))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["head"], "Updated");
    assert_eq!(rows[0]["body"], updated);
    engine
        .act(Backend::DeleteRecord { target: uid }, None)
        .await
        .unwrap();
    assert!(
        protein::execute(&engine.store, &query(None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn component_mutations_respect_existing_organ_permissions() {
    let engine = engine::Engine::open_memory().await.unwrap();
    engine
        .act(
            Backend::CreateRole {
                name: "component-reader".into(),
            },
            None,
        )
        .await
        .unwrap();
    let user = engine
        .act(
            Backend::CreateUser {
                username: "reader".into(),
                name: "Reader".into(),
                password: "reader-password".into(),
                role: "component-reader".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let create = Backend::CreateCustomComponent {
        head: "Component".into(),
        body: json!({"format":FORMAT,"castle":{"name":"Component","parts":[{}]}}).to_string(),
    };
    assert!(
        engine
            .act(create.clone(), Some(user.clone()))
            .await
            .is_err()
    );
    let uid = engine.act(create, None).await.unwrap().created.unwrap();
    engine
        .act(
            Backend::GrantPermission {
                role: "component-reader".into(),
                permission: "record:read".into(),
            },
            None,
        )
        .await
        .unwrap();
    let visible = protein::execute_for(&engine.store, &query(None), Some(&user))
        .await
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0]["uid"], uid);
    for action in [
        Backend::EditRecordText {
            target: uid.clone(),
            head: Some("Changed".into()),
            body: None,
        },
        Backend::DeleteRecord {
            target: uid.clone(),
        },
    ] {
        assert!(engine.act(action, Some(user.clone())).await.is_err());
    }
    let rows = protein::execute(&engine.store, &query(None)).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["head"], "Component");
    engine
        .act(
            Backend::SetPersonReadFilter {
                person: user.clone(),
                filter: Some(protein::Predicate::Any(vec![])),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        protein::execute_for(&engine.store, &query(None), Some(&user))
            .await
            .unwrap()
            .is_empty()
    );
    for body in ["{}".to_string(), "x".repeat(MAX_BYTES + 1)] {
        assert!(
            engine
                .act(
                    Backend::CreateCustomComponent {
                        head: "Invalid".into(),
                        body
                    },
                    None
                )
                .await
                .is_err()
        );
    }
    assert_eq!(
        protein::execute(&engine.store, &query(None))
            .await
            .unwrap()
            .len(),
        1
    );
}
