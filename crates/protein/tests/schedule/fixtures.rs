use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use serde_json::Value;

pub(super) async fn task(engine: &Engine, slug: &str, work: Value, quantity: f64) -> String {
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity,
            },
            None,
        )
        .await
        .expect("create fixture task")
        .created
        .expect("created fixture task UID");
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: work,
            },
            None,
        )
        .await
        .expect("set fixture work extension");
    uid
}
