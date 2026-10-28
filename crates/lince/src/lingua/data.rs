use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nucleus::script::{Host, Value};
use protein::{Predicate, Protein, Source};
use store::Store;

pub fn print_output(text: &str) -> Result<(), String> {
    std::io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|error| format!("Cannot write output: {error}"))
}

pub struct DataHost {
    directory: Option<PathBuf>,
    database: Option<Database>,
    started: Instant,
    reads: usize,
    readings: BTreeMap<(String, Option<String>), Value>,
    cached_bytes: usize,
}

struct Database {
    runtime: tokio::runtime::Runtime,
    store: Store,
}

impl Drop for Database {
    fn drop(&mut self) {
        self.runtime.block_on(self.store.pool.close());
    }
}

impl DataHost {
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            database: None,
            started: Instant::now(),
            reads: 0,
            readings: BTreeMap::new(),
            cached_bytes: 0,
        }
    }

    fn database(&mut self) -> Result<&Database, String> {
        self.reads += 1;
        if self.reads > 256 || self.started.elapsed() > Duration::from_secs(30) {
            return Err("Script database read or time limit exceeded".into());
        }
        if self.database.is_none() {
            let directory = self.directory.clone()
                .or_else(|| std::env::var_os("LINCE_DATA_DIR").map(PathBuf::from))
                .or_else(utils::config::lince_data_dir)
                .ok_or("No Lince directory is available; select one with dir = \"...\" or --directory DIR")?;
            let path = directory.join("lince.db");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let store = runtime.block_on(Store::open_readonly(&path)).map_err(|error| {
                format!("Cannot query Lince database at {}: {error}. Select an existing Lince directory with dir = \"...\" or --directory DIR", path.display())
            })?;
            self.database = Some(Database { runtime, store });
        }
        Ok(self.database.as_ref().expect("opened database"))
    }

    fn query(&mut self, mut query: Protein, declarations: bool) -> Result<Value, String> {
        if protein::is_ephemeral(&query) {
            return Err("This query source needs a running Cell's live context and is unavailable in read-only Lingua".into());
        }
        if query.limit.is_some_and(|limit| limit == 0 || limit > 1024) {
            return Err("Use a query limit between 1 and 1024".into());
        }
        query.limit.get_or_insert(100);
        protein::validate(&query).map_err(|error| format!("Invalid Protein query: {error}"))?;
        let database = self.database()?;
        database.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                let rows = protein::execute_for(&database.store, &query, None)
                    .await
                    .map_err(|error| error.to_string())?;
                if rows.len() > 1024 {
                    return Err("Query result limit exceeded".into());
                }
                let bytes = serde_json::to_vec(&rows)
                    .map_err(|error| error.to_string())?
                    .len();
                if bytes > nucleus::script::MAX_BYTES {
                    return Err("Query result exceeds 4 MiB".into());
                }
                let mut output = Vec::new();
                for row in rows {
                    if declarations
                        && query.source == Source::Record
                        && query.aggregate.is_none()
                        && query.fields.is_none()
                    {
                        let uid = row["uid"].as_str().ok_or("Record query returned no UID")?;
                        output.push(Value::Declaration(
                            render_record(&database.store, uid).await?,
                        ));
                    } else {
                        output.push(Value::from_json(row)?);
                    }
                }
                Ok(Value::List(output))
            })
            .await
            .map_err(|_| "Query exceeded its five-second time limit".to_string())?
        })
    }

    fn reading(&mut self, target: &str, property: Option<&str>) -> Result<Value, String> {
        let key = (target.to_string(), property.map(str::to_string));
        if let Some(value) = self.readings.get(&key) {
            return Ok(value.clone());
        }
        let value = self.read_record(target, property)?;
        let bytes = target.len() + property.map(str::len).unwrap_or(0) + value.render().len();
        if self.cached_bytes.saturating_add(bytes) <= nucleus::script::MAX_BYTES {
            self.cached_bytes += bytes;
            self.readings.insert(key, value.clone());
        }
        Ok(value)
    }

    fn read_record(&mut self, target: &str, property: Option<&str>) -> Result<Value, String> {
        let filter = if nucleus::valid_uid(target, "r") {
            Predicate::UidEq(target.into())
        } else {
            Predicate::SlugEq(target.into())
        };
        let query = Protein {
            source: Source::Record,
            filter: vec![filter],
            fields: property.map(|field| vec![field.into()]),
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: Some(2),
        };
        let Value::List(mut rows) = self.query(query, property.is_none())? else {
            unreachable!()
        };
        if rows.len() != 1 {
            return Err(format!("No unique Record matches @{target}"));
        }
        let value = rows.remove(0);
        let Some(property) = property else {
            return Ok(value);
        };
        let Value::Object(mut row) = value else {
            return Err("Invalid Record projection".into());
        };
        let value = row
            .remove(property)
            .ok_or_else(|| format!("Record property `{property}` is unavailable"))?;
        if property == "quantity" {
            let Value::Text(value) = value else {
                return Err("Record quantity is not an exact decimal".into());
            };
            return nucleus::DecimalValue::parse_inferred(&value)
                .map(Value::Number)
                .map_err(|error| error.to_string());
        }
        Ok(value)
    }
}

impl Host for DataHost {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value, String> {
        match (name, arguments) {
            (
                "record" | "quantity" | "head" | "body" | "slug",
                [Value::Reference(target) | Value::Text(target)],
            ) => self.reading(target, if name == "record" { None } else { Some(name) }),
            (
                "property",
                [
                    Value::Reference(target) | Value::Text(target),
                    Value::Text(property),
                ],
            ) => self.reading(target, Some(property)),
            ("records", []) => self.query(record_query(Vec::new()), true),
            ("records", [Value::Concept(concept)]) => self.query(
                record_query(vec![Predicate::ConceptIn(concept.clone())]),
                true,
            ),
            ("records", [Value::Text(text)]) => self.query(
                record_query(vec![Predicate::TextContains(text.clone())]),
                true,
            ),
            ("query", [descriptor]) => {
                let json = match descriptor {
                    Value::Text(source) => serde_json::from_str(source)
                        .map_err(|error| format!("Invalid Protein JSON: {error}"))?,
                    value => query_json(value, None)?,
                };
                let query = serde_json::from_value(json)
                    .map_err(|error| format!("Invalid Protein query: {error}"))?;
                self.query(query, false)
            }
            ("frequency", [Value::Reference(target) | Value::Text(target)]) => {
                let database = self.database()?;
                database.runtime.block_on(async {
                    let record = store::records::resolve(&database.store.pool, target).await.map_err(|error| error.to_string())?.ok_or_else(|| format!("No Frequency matches @{target}"))?;
                    let handle = store::karma::frequencies::get_handle(&database.store.pool, &record.uid).await.map_err(|error| error.to_string())?.ok_or_else(|| format!("No Frequency matches @{target}"))?;
                    let revision = store::karma::frequencies::get_revision(&database.store.pool, &handle.head_revision_hash).await.map_err(|error| error.to_string())?.ok_or("Frequency revision is unavailable")?;
                    if handle.active_revision_hash.as_ref().is_some_and(|hash| hash != &handle.head_revision_hash) {
                        return Err("The active Frequency revision differs from its saved head; query the revisions explicitly".into());
                    }
                    if let Some(hash) = &handle.active_activation_hash {
                        let activation = store::karma::frequencies::get_activation(&database.store.pool, hash).await.map_err(|error| error.to_string())?.ok_or("Frequency activation is unavailable")?;
                        if activation.epoch.effective_parameters() != &revision.default_compiled.effective_parameters {
                            return Err("The Frequency has parameter overrides that its declaration cannot represent".into());
                        }
                    }
                    let title = serde_json::to_string(&revision.frequency.purpose).map_err(|error| error.to_string())?;
                    let definition = serde_json::to_string(&serde_json::to_string(&revision.frequency).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
                    let active = i32::from(handle.status == nucleus::karma::DefinitionStatus::Active);
                    let source = format!("Frequency {} {{\n title {title}\n quantity {active}\n definition {definition}\n}} ^{}\n", handle.slug, handle.record_uid);
                    anicca::canonicalize(&source).map(Value::Declaration).map_err(|error| error.to_string())
                })
            }
            (
                "extension",
                [
                    Value::Reference(target) | Value::Text(target),
                    Value::Text(namespace),
                ],
            ) => {
                let mut query = record_query(vec![if nucleus::valid_uid(target, "r") {
                    Predicate::UidEq(target.clone())
                } else {
                    Predicate::SlugEq(target.clone())
                }]);
                query.fields = Some(vec!["extension".into()]);
                query.include.extension = Some(protein::ExtensionInclude {
                    namespace: namespace.clone(),
                });
                protein::validate(&query).map_err(|error| error.to_string())?;
                let database = self.database()?;
                database.runtime.block_on(async {
                    let mut rows = protein::execute_for(&database.store, &query, None)
                        .await
                        .map_err(|error| error.to_string())?;
                    if rows.len() != 1 {
                        return Err("No unique Extension Record is available".into());
                    }
                    let row = rows.remove(0);
                    let uid = row["uid"].as_str().ok_or("Extension Record has no UID")?;
                    let fields = row
                        .get("extension")
                        .filter(|value| value.is_object())
                        .ok_or("Extension is unavailable")?;
                    anicca::extension::render(uid, namespace, fields)
                        .map(Value::Declaration)
                        .map_err(|error| error.to_string())
                })
            }
            ("rule", [Value::Reference(target) | Value::Text(target)]) => {
                let database = self.database()?;
                database.runtime.block_on(async {
                    let rule = store::recurrence::get(&database.store.pool, target)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| format!("No Rule matches {target}"))?;
                    let projected = anicca::ProjectedRule {
                        uid: rule.uid.clone(),
                        slug: rule.uid.to_ascii_lowercase().replace('_', "-"),
                        quantity: i64::from(!rule.is_paused()),
                        frequency_slug: String::new(),
                        frequency_uid: None,
                        record_slug: rule.record_uid.clone(),
                        record_uid: Some(rule.record_uid),
                        condition: rule
                            .condition
                            .as_ref()
                            .map(|condition| condition.source.clone()),
                        gate: rule
                            .condition
                            .as_ref()
                            .map(|condition| condition.gate.as_text()),
                        carry: rule
                            .condition
                            .as_ref()
                            .map(|condition| condition.carry.as_text()),
                        consequences_json: serde_json::to_string(&rule.consequences)
                            .map_err(|error| error.to_string())?,
                        cadence: Some(
                            serde_json::to_string(&rule.cadence)
                                .map_err(|error| error.to_string())?,
                        ),
                        anchor_at: Some(rule.anchor_at),
                        note: rule.note,
                    };
                    anicca::render_rule(&projected)
                        .map(Value::Declaration)
                        .map_err(|error| error.to_string())
                })
            }
            _ => Err(format!("Unknown function or invalid arguments: {name}()")),
        }
    }

    fn print(&mut self, text: &str) -> Result<(), String> {
        print_output(text)
    }

    fn cancelled(&self) -> bool {
        self.started.elapsed() > Duration::from_secs(30)
    }
}

fn record_query(filter: Vec<Predicate>) -> Protein {
    Protein {
        source: Source::Record,
        filter,
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(100),
    }
}

fn query_json(value: &Value, field: Option<&str>) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match value {
        Value::Number(value)
            if matches!(
                field,
                Some(
                    "quantity_lt" | "quantity_lte" | "quantity_gt" | "quantity_gte" | "quantity_eq"
                )
            ) =>
        {
            Ok(J::String(value.to_string()))
        }
        Value::Number(value) => {
            serde_json::from_str(&value.to_string()).map_err(|error| error.to_string())
        }
        Value::List(values) => values
            .iter()
            .map(|value| query_json(value, None))
            .collect::<Result<Vec<_>, _>>()
            .map(J::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), query_json(value, Some(key))?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(J::Object),
        value => value.to_json(),
    }
}

async fn render_record(store: &Store, uid: &str) -> Result<String, String> {
    let row = store::records::get(&store.pool, uid)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Record is unavailable")?;
    let mut assertions = Vec::new();
    for assertion in store::assertions::for_subjects(&store.pool, &[uid.into()])
        .await
        .map_err(|error| error.to_string())?
    {
        assertions.push(anicca::ProjectedAssertion {
            object_uri: assertion.object_uri,
            predicate: assertion.predicate,
            identity: row.identity_predicate_uid.as_deref() == Some(&assertion.predicate_uid),
            object_slug: assertion.object_uid.clone(),
            object_uid: assertion.object_uid,
            quantity: assertion.quantity.map(|value| value.to_string()),
            unit: assertion.unit_uid,
        });
    }
    let projected = anicca::ProjectedRecord {
        uid: row.uid,
        slug: row.slug,
        head: row.head,
        body: row.body,
        quantity: Some((row.quantity.to_string(), row.unit_uid)),
        assertions,
    };
    anicca::render_record(&projected)
        .map_err(|error| format!("Record cannot be printed as Lingua: {error}"))
}
