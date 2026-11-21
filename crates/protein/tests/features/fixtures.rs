use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use protein::{Predicate, Protein, Source, ThreadsInclude};

pub(super) async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

pub(super) async fn make(e: &Engine, slug: &str, kind: RecordKind, quantity: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind,
            head: slug.into(),
            body: String::new(),
            quantity,
        },
        None,
    )
    .await
    .expect("create fixture Record")
    .created
    .expect("creation returns the Record UID")
}

pub(super) fn base(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

pub(super) fn thread_query(subject: String, messages_limit: usize) -> Protein {
    let mut query = base(Source::Record, vec![Predicate::UidEq(subject)]);
    query.include.threads = Some(ThreadsInclude {
        messages_limit,
        ..Default::default()
    });
    query
}
