use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

#[derive(Default)]
struct Query {
    statement: String,
    summary: String,
    rows: u64,
    elapsed: f64,
}

impl Visit for Query {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "db.statement" => {
                self.statement = value.split_whitespace().collect::<Vec<_>>().join(" ")
            }
            "summary" => self.summary = value.into(),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "rows_returned" {
            self.rows = value;
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed = value;
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "summary" {
            self.summary = format!("{value:?}");
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Queries(Arc<Trace>);

#[derive(Default)]
struct Trace {
    active: AtomicBool,
    rows: Mutex<Vec<Query>>,
}

impl Queries {
    pub(super) fn start(&self) {
        self.0.rows.lock().expect("SQL event buffer").clear();
        self.0.active.store(true, Ordering::Release);
    }

    pub(super) fn finish(&self) -> BTreeMap<String, (usize, u64, f64)> {
        self.0.active.store(false, Ordering::Release);
        let mut statements = BTreeMap::<_, (usize, u64, f64)>::new();
        for query in self.0.rows.lock().expect("SQL event buffer").drain(..) {
            let key = if query.statement.is_empty() {
                query.summary
            } else {
                query.statement
            };
            let (calls, rows, elapsed) = statements.entry(key).or_default();
            *calls += 1;
            *rows += query.rows;
            *elapsed += query.elapsed;
        }
        statements
    }
}

impl<S: Subscriber> Layer<S> for Queries {
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.target() == "sqlx::query"
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if self.0.active.load(Ordering::Acquire) {
            let mut query = Query::default();
            event.record(&mut query);
            let mut rows = self.0.rows.lock().expect("SQL event buffer");
            assert!(rows.len() < 512, "Private SQL probe exceeded its bound");
            rows.push(query);
        }
    }
}
