use super::*;
use nucleus::transfer::{
    receipt::{Document, Event},
    receipt_fragment::{self, Resolution},
};
use serde_json::{Value, json};

mod artifact;
use artifact::Artifact;

pub(super) struct Collection {
    origin: String,
    occurrence: CanonicalHash,
    artifacts: BTreeMap<(String, u64), Artifact>,
    pub selected: BTreeSet<(String, u64)>,
    pub events: BTreeMap<CanonicalHash, Event>,
    scopes: BTreeMap<CanonicalHash, Scope>,
    chosen: BTreeMap<CanonicalHash, BTreeSet<CanonicalHash>>,
    tried: BTreeSet<(String, u64)>,
    bytes: usize,
    work: usize,
    scanned: usize,
    max_bytes: usize,
    max_work: usize,
    pub partial: bool,
    pub failures: Vec<Value>,
}

pub(super) async fn visible(
    engine: &Engine,
    rows: &[store::receipt_events::Candidate],
    actor: Option<&str>,
) -> Result<BTreeSet<String>, EngineError> {
    let query = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::Any(
            rows.iter()
                .take(64)
                .map(|row| protein::Predicate::UidEq(row.record.clone()))
                .collect(),
        )],
        fields: Some(vec!["uid".into()]),
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: Some(64),
    };
    Ok(protein::execute_for(&engine.store, &query, actor)
        .await?
        .into_iter()
        .filter_map(|row| row["uid"].as_str().map(str::to_owned))
        .collect())
}

impl Collection {
    pub async fn scope_context(
        engine: &Engine,
        scope: &Scope,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Self, EngineError> {
        let mut collection = Self::new(&scope.source.origin, &scope.source.occurrence);
        let id = scope.id().map_err(invalid)?;
        let mut before: Option<(String, String)> = None;
        loop {
            let rows = store::receipt_references::scope_candidates(
                &engine.store.pool,
                &id,
                before
                    .as_ref()
                    .map(|(date, uid)| (date.as_str(), uid.as_str())),
            )
            .await?;
            let allowed = visible(engine, &rows, actor).await?;
            let more = rows.len() > 64;
            for row in rows.into_iter().take(64) {
                collection.scanned += 1;
                if collection.scanned > 5000 {
                    collection.partial = true;
                    return Ok(collection);
                }
                before = Some((row.created_at.clone(), row.record.clone()));
                if allowed.contains(&row.record)
                    && !collection.load(engine, &row, false, now).await?
                {
                    return Ok(collection);
                }
            }
            if !more {
                return Ok(collection);
            }
        }
    }

    pub fn extend_bundle(&self, bundle: &mut Bundle) -> Result<(), EngineError> {
        let scope = bundle.publication.scope.id().map_err(invalid)?;
        let mut seen = bundle
            .publication
            .events
            .iter()
            .map(|signed| signed.event.id().map_err(invalid))
            .collect::<Result<BTreeSet<_>, _>>()?;
        for artifact in self.artifacts.values() {
            let other = artifact.saved().document.public_bundle().unwrap();
            nucleus::performance::add("receipt-artifact-identities-reused", 1);
            if artifact.scope() != &scope {
                return Err(invalid("Proof context contains a different approved scope"));
            }
            nucleus::performance::add(
                "receipt-artifact-identities-reused",
                artifact.events().len() as u64,
            );
            for (event, id) in other.publication.events.iter().zip(artifact.events()) {
                if seen.insert(id.clone()) {
                    bundle.publication.events.push(event.clone());
                }
            }
            for authority in &other.authorities {
                if !bundle.authorities.contains(authority) {
                    bundle.authorities.push(authority.clone());
                }
            }
        }
        Ok(())
    }

    pub fn has_fragments(&self) -> bool {
        self.artifacts
            .values()
            .any(|artifact| matches!(artifact.saved().document, Document::ProofFragment(_)))
    }

    pub fn new(origin: &str, occurrence: &CanonicalHash) -> Self {
        Self {
            origin: origin.into(),
            occurrence: occurrence.clone(),
            artifacts: BTreeMap::new(),
            selected: BTreeSet::new(),
            events: BTreeMap::new(),
            scopes: BTreeMap::new(),
            chosen: BTreeMap::new(),
            tried: BTreeSet::new(),
            bytes: 0,
            work: 0,
            scanned: 0,
            max_bytes: 8 * 1024 * 1024,
            max_work: 5000,
            partial: false,
            failures: vec![],
        }
    }

    pub fn restrict_budget(&mut self, bytes: usize, events: usize) {
        self.max_bytes = self.max_bytes.min(bytes);
        self.max_work = self.max_work.min(events);
    }

    pub fn used_budget(&self) -> (usize, usize) {
        (self.bytes, self.work)
    }

    pub fn artifacts(&self) -> impl Iterator<Item = &store::transfer_receipts::Saved> {
        self.artifacts.values().map(Artifact::saved)
    }

    pub fn artifact(&self, key: &(String, u64)) -> &Artifact {
        &self.artifacts[key]
    }

    fn choose(&mut self, key: &(String, u64)) {
        let artifact = &self.artifacts[key];
        let chosen = self.chosen.entry(artifact.scope().clone()).or_default();
        chosen.extend(artifact.events().iter().cloned());
        nucleus::performance::add(
            "receipt-artifact-identities-reused",
            (artifact.events().len() + 1) as u64,
        );
        self.selected.insert(key.clone());
    }

    pub async fn load(
        &mut self,
        engine: &Engine,
        candidate: &store::receipt_events::Candidate,
        selected: bool,
        now: DateTime<Utc>,
    ) -> Result<bool, EngineError> {
        let key = (candidate.record.clone(), candidate.version as u64);
        if self.tried.contains(&key) {
            if selected && self.artifacts.contains_key(&key) {
                self.choose(&key);
            }
            return Ok(true);
        }
        let loaded = async {
            let saved = store::transfer_receipts::get_version(
                &engine.store.pool,
                &candidate.record,
                candidate.version as u64,
            )
            .await?;
            let bundle = saved
                .document
                .public_bundle()
                .ok_or_else(|| invalid("Only original public proof artifacts enter history"))?;
            let size = nucleus::json::encoded_len(&saved.document)?;
            if self.bytes.saturating_add(size) > self.max_bytes
                || self.work.saturating_add(bundle.publication.events.len()) > self.max_work
            {
                return Ok(None);
            }
            self.bytes += size;
            self.work += bundle.publication.events.len();
            if bundle.publication.scope.source.origin != self.origin
                || bundle.publication.scope.source.occurrence != self.occurrence
            {
                return Err(invalid("Proof source differs from its indexed occurrence"));
            }
            engine
                .verify_transfer_receipt_document(&saved.document, now)
                .await?;
            store::receipt_references::checked(&engine.store.pool, &saved).await?;
            if matches!(saved.document, Document::Publication(_)) {
                store::receipt_events::checked(&engine.store.pool, &saved).await?;
            }
            Ok::<_, EngineError>(Some(saved))
        }
        .await;
        match loaded {
            Ok(Some(saved)) => {
                let artifact = Artifact::new(saved)?;
                let bundle = artifact.saved().document.public_bundle().unwrap();
                let scope = &bundle.publication.scope;
                if self.scopes.get(artifact.scope()) != Some(scope) {
                    self.scopes.insert(artifact.scope().clone(), scope.clone());
                }
                for (signed, id) in bundle.publication.events.iter().zip(artifact.events()) {
                    if let Some(previous) = self.events.get(id) {
                        if previous != &signed.event {
                            return Err(invalid(
                                "One original proof identifier has conflicting event bytes",
                            ));
                        }
                        nucleus::performance::add("receipt-identical-event-copies-avoided", 1);
                    } else {
                        self.events.insert(id.clone(), signed.event.clone());
                    }
                }
                self.artifacts.insert(key.clone(), artifact);
                self.tried.insert(key.clone());
                if selected {
                    self.choose(&key);
                }
                Ok(true)
            }
            Ok(None) => {
                self.partial = true;
                Ok(false)
            }
            Err(error) => {
                self.partial = true;
                self.tried.insert(key);
                self.failures.push(json!({"record":candidate.record,"state":error.code().unwrap_or("invalid"),"message":error.to_string()}));
                Ok(true)
            }
        }
    }

    pub fn resolve(
        &self,
        now: DateTime<Utc>,
    ) -> Result<BTreeMap<CanonicalHash, Resolution>, EngineError> {
        self.chosen
            .iter()
            .map(|(id, selected)| {
                Ok((
                    id.clone(),
                    receipt_fragment::resolve(
                        &self.scopes[id],
                        &self.events,
                        selected,
                        now.timestamp(),
                    )
                    .map_err(invalid)?,
                ))
            })
            .collect()
    }

    pub async fn complete(
        &mut self,
        engine: &Engine,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<BTreeMap<CanonicalHash, Resolution>, EngineError> {
        let mut attempted = BTreeSet::new();
        loop {
            let resolutions = self.resolve(now)?;
            let wanted = resolutions
                .iter()
                .map(|(scope, result)| {
                    (
                        scope.clone(),
                        result
                            .missing
                            .iter()
                            .filter(|event| !attempted.contains(&(scope.clone(), (*event).clone())))
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                })
                .filter(|(_, events)| !events.is_empty())
                .collect::<Vec<_>>();
            if wanted.is_empty() {
                return Ok(resolutions);
            }
            let previous = self.events.len();
            for (scope, events) in wanted {
                for batch in events.chunks(128) {
                    for event in batch {
                        attempted.insert((scope.clone(), event.clone()));
                    }
                    let mut before: Option<(String, String)> = None;
                    loop {
                        if self.scanned >= 5000 {
                            self.partial = true;
                            return self.resolve(now);
                        }
                        let rows = store::receipt_references::lookup(
                            &engine.store.pool,
                            &scope,
                            batch,
                            before
                                .as_ref()
                                .map(|(date, uid)| (date.as_str(), uid.as_str())),
                        )
                        .await?;
                        let allowed = visible(engine, &rows, actor).await?;
                        let more = rows.len() > 64;
                        for row in rows.into_iter().take(64) {
                            self.scanned += 1;
                            if self.scanned > 5000 {
                                self.partial = true;
                                return self.resolve(now);
                            }
                            before = Some((row.created_at.clone(), row.record.clone()));
                            if allowed.contains(&row.record)
                                && !self.load(engine, &row, false, now).await?
                            {
                                return self.resolve(now);
                            }
                        }
                        if !more {
                            break;
                        }
                    }
                }
            }
            if self.events.len() == previous {
                return self.resolve(now);
            }
        }
    }
}
