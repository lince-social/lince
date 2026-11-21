use nucleus::{DecimalValue, RecordKind};
use serde_json::json;
use std::{env::var, path::PathBuf};
use store::{
    Store,
    records::{NewRecord, create, set_extension},
};

pub(super) struct Settings {
    pub output: PathBuf,
    pub database: PathBuf,
    pub count: usize,
    pub bytes: usize,
    pub logs: usize,
    pub traced: bool,
}

impl Settings {
    pub(super) fn from_env() -> Self {
        let path = |key| PathBuf::from(var(key).unwrap_or_else(|error| panic!("{key}: {error}")));
        let number = |key| {
            var(key)
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or_else(|| panic!("{key} must contain a nonnegative integer"))
        };
        let output = path("LINCE_QUERY_PROBE_OUTPUT");
        let database = path("LINCE_QUERY_PROBE_DB");
        assert!(!database.exists(), "probe database must be new");
        let count = number("LINCE_QUERY_PROBE_RECORDS");
        let bytes = number("LINCE_QUERY_PROBE_BYTES");
        let logs = number("LINCE_QUERY_PROBE_LOGS");
        assert!((1..=64).contains(&count) && bytes <= 1_048_576 && logs <= 512);
        let traced = var("LINCE_QUERY_PROBE_TRACE").as_deref() == Ok("1");
        Self {
            output,
            database,
            count,
            bytes,
            logs,
            traced,
        }
    }
}

pub(super) async fn seed(
    store: &Store,
    count: usize,
    bytes: usize,
    logs: usize,
) -> (String, Vec<String>) {
    let line = "é👩‍💻 paragraph\n";
    let body = line.repeat(bytes / line.len());
    let work = json!({
        "start":"2026-09-01", "due":"2026-09-30", "estimate_min":90,
        "logs": vec![json!({"start":"2026-09-01T10:00:00Z","end":"2026-09-01T10:15:00Z"}); logs],
    });
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let record = create(
            &store.pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: &format!("Private query {index}"),
                body: &body,
                quantity: DecimalValue::parse_inferred("9007199254740993.125")
                    .expect("fixture quantity"),
            },
        )
        .await
        .expect("create probe Record");
        set_extension(&store.pool, &record.uid, "work", &work)
            .await
            .expect("write probe work logs");
        ids.push(record.uid);
    }
    (body, ids)
}
