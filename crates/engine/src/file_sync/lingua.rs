mod automation;
mod habits;

use super::*;
use anicca::{ProjectedAssertion, ProjectedRecord, grammar::grammar as ast};
use automation::Automation;

#[derive(Debug, Default)]
pub(super) struct State {
    files: HashMap<PathBuf, Document>,
}

#[derive(Debug, Clone)]
struct Document {
    source: String,
    identified: String,
    records: Vec<ProjectedRecord>,
    extensions: Vec<anicca::ProjectedExtension>,
    automation: Automation,
    missing: u32,
}

fn invalid(error: impl std::fmt::Display) -> EngineError {
    EngineError::Consequence(error.to_string())
}

fn conflict(report: &mut FileSyncReport, path: &Path, error: impl std::fmt::Display) {
    report.conflicts.push(FileConflict {
        path: path.display().to_string(),
        reason: error.to_string(),
    });
}

fn normalized_decimal(text: &str) -> Result<String, EngineError> {
    let value = nucleus::DecimalValue::parse_inferred(text).map_err(invalid)?;
    Ok((0..=value.scale())
        .find_map(|scale| value.rescale(scale))
        .unwrap()
        .to_string())
}

fn normalize(record: &mut ProjectedRecord) -> Result<(), EngineError> {
    if !record.body.is_empty() && !record.body.ends_with('\n') {
        record.body.push('\n');
    }
    if let Some((value, _)) = &mut record.quantity {
        *value = normalized_decimal(value)?;
    }
    for assertion in &mut record.assertions {
        if let Some(value) = &mut assertion.quantity {
            *value = normalized_decimal(value)?;
        }
    }
    record.assertions.sort_by_key(|value| {
        format!(
            "{}:{:?}:{:?}",
            value.predicate, value.object_uid, value.object_uri
        )
    });
    Ok(())
}

impl Engine {
    async fn remember_lingua_source(
        &self,
        path: &Path,
        document: &Document,
    ) -> Result<(), EngineError> {
        let uri = lince_resources::Reference::path(path)
            .map_err(invalid)?
            .target;
        let hash = lince_resources::fingerprint(document.source.as_bytes());
        let mut tx = store::write_tx(&self.store.pool).await?;
        for record in &document.records {
            store::source_locations::remember_on(&mut tx, &record.uid, &record.uid, &uri, &hash)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub fn import_lingua_directory<'a>(
        &'a self,
        directory: &'a Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<FileSyncReport, EngineError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("Lingua import requires a local Organ"))?;
            self.lingua_sync_pass(directory, &organ.uid, None, &mut State::default())
                .await
        })
    }

    async fn read_lingua_document(
        &self,
        source: &str,
        known: Option<&Document>,
    ) -> Result<Document, EngineError> {
        let original = anicca::parse(source).map_err(invalid)?;
        let (identified, _) = anicca::ensure_uids(source).map_err(invalid)?;
        let mut document = anicca::parse(&identified).map_err(invalid)?;
        let original_count = original.declarations.len();
        for (index, declaration) in document.declarations.iter_mut().enumerate() {
            let ast::Declaration::Record(record) = declaration else {
                continue;
            };
            let ast::Declaration::Record(original) = &original.declarations[index] else {
                unreachable!()
            };
            if original.opening.uid().is_none() {
                let previous = known.and_then(|known| {
                    known.records.iter().find(|previous| {
                        previous.slug.as_deref() == record.header.subject.slug()
                            && (previous.slug.is_some() || previous.head == record.title.value())
                    })
                });
                let previous = previous.or_else(|| {
                    known
                        .filter(|known| known.records.len() == original_count)
                        .and_then(|known| known.records.get(index))
                });
                let existing = match record.header.subject.slug() {
                    Some(slug) => store::records::resolve(&self.store.pool, slug).await?,
                    None => None,
                };
                if let Some(uid) = previous
                    .map(|record| record.uid.clone())
                    .or_else(|| existing.map(|row| row.uid))
                {
                    record.opening = ast::RecordOpening::identified(uid.clone());
                    record.closing = ast::RecordClosing::identified(uid);
                }
            }
        }
        let mut projected = anicca::project(&document).map_err(invalid)?;
        projected.extensions.sort_by(|left, right| {
            (&left.target_slug, &left.namespace).cmp(&(&right.target_slug, &right.namespace))
        });
        for record in &mut projected.records {
            normalize(record)?;
            crate::record_creation::Draft {
                uid: record.uid.clone(),
                head: record.head.clone(),
                body: record.body.clone(),
                slug: record.slug.clone(),
                quantity: record.quantity.as_ref().unwrap().0.clone(),
                assertions: record
                    .assertions
                    .iter()
                    .map(|a| crate::record_creation::Assertion {
                        predicate: a.predicate.clone(),
                        object: a.object_uri.clone().or_else(|| a.object_slug.clone()),
                        quantity: a.quantity.clone(),
                        unit: a.unit.clone(),
                    })
                    .collect(),
                ..Default::default()
            }
            .validate()?;
        }
        let mut automation = Automation {
            frequencies: projected.frequencies,
            rules: projected.rules,
        };
        automation.normalize()?;
        Ok(Document {
            automation,
            source: source.into(),
            identified: anicca::format(&document),
            records: projected.records,
            extensions: projected.extensions,
            missing: 0,
        })
    }

    async fn lingua_record(&self, uid: &str) -> Result<Option<ProjectedRecord>, EngineError> {
        let Some(row) = store::records::get(&self.store.pool, uid).await? else {
            return Ok(None);
        };
        let concepts: HashMap<_, _> = store::concepts::list_all(&self.store.pool)
            .await?
            .into_iter()
            .map(|row| (row.uid, row.canonical_name))
            .collect();
        let mut assertions = Vec::new();
        for assertion in store::assertions::for_subjects(&self.store.pool, &[uid.into()]).await? {
            let object_slug = match &assertion.object_uid {
                Some(uid) => Some(
                    store::records::get(&self.store.pool, uid)
                        .await?
                        .and_then(|row| row.slug)
                        .ok_or_else(|| {
                            invalid(
                                "A linked Record needs a slug before it can be written in Lingua",
                            )
                        })?,
                ),
                None => None,
            };
            assertions.push(ProjectedAssertion {
                object_uri: assertion.object_uri,
                predicate: concepts
                    .get(&assertion.predicate_uid)
                    .cloned()
                    .unwrap_or(assertion.predicate),
                identity: row.identity_predicate_uid.as_deref() == Some(&assertion.predicate_uid),
                object_slug,
                object_uid: assertion.object_uid,
                quantity: assertion.quantity.map(|quantity| quantity.to_string()),
                unit: assertion
                    .unit_uid
                    .as_ref()
                    .and_then(|uid| concepts.get(uid).cloned()),
            });
        }
        let mut record = ProjectedRecord {
            uid: row.uid,
            slug: row.slug,
            head: row.head,
            body: row.body,
            quantity: Some((
                row.quantity.to_string(),
                row.unit_uid
                    .as_ref()
                    .and_then(|uid| concepts.get(uid).cloned()),
            )),
            assertions,
        };
        normalize(&mut record)?;
        Ok(Some(record))
    }

    async fn lingua_extensions(
        &self,
        records: &[ProjectedRecord],
    ) -> Result<Vec<anicca::ProjectedExtension>, EngineError> {
        let mut extensions = Vec::new();
        for record in records {
            let Some(slug) = &record.slug else {
                continue;
            };
            let values: Vec<(String, String)> = store::sqlx::query_as(
                "SELECT namespace,fds FROM record_extension WHERE record_uid=? ORDER BY namespace",
            )
            .bind(&record.uid)
            .fetch_all(&self.store.pool)
            .await?;
            for (namespace, raw) in values {
                if shared_extension(&namespace) {
                    extensions.push(anicca::ProjectedExtension {
                        target_slug: slug.clone(),
                        target_uid: Some(record.uid.clone()),
                        namespace,
                        fields: serde_json::from_str(&raw)?,
                    });
                }
            }
        }
        extensions.sort_by(|left, right| {
            (&left.target_slug, &left.namespace).cmp(&(&right.target_slug, &right.namespace))
        });
        Ok(extensions)
    }

    async fn apply_lingua_record(&self, record: &ProjectedRecord) -> Result<bool, EngineError> {
        if store::karma::frequencies::get_handle(&self.store.pool, &record.uid)
            .await?
            .is_some()
        {
            return Ok(false);
        }
        let previous = self.lingua_record(&record.uid).await?;
        if previous.as_ref() == Some(record) {
            return Ok(false);
        }
        let created = previous.is_none();
        let (quantity, unit) = record.quantity.as_ref().unwrap();
        let mut meanings = HashMap::new();
        for name in record
            .assertions
            .iter()
            .map(|a| &a.predicate)
            .chain(record.assertions.iter().filter_map(|a| a.unit.as_ref()))
            .chain(unit.iter())
        {
            let concept =
                match store::vocabulary::resolve_file_meaning(&self.store.pool, name, &record.uid)
                    .await?
                {
                    Some(concept) => concept,
                    None => self
                        .act(
                            Action::CreateConcept {
                                lingua: store::linguas::LOCAL_UID.into(),
                                name: name.clone(),
                                parents: vec![],
                            },
                            None,
                        )
                        .await?
                        .created
                        .ok_or_else(|| invalid("Unable to save the selected meaning"))?,
                };
            store::sqlx::query(
                "INSERT OR IGNORE INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)",
            )
            .bind(&concept)
            .bind(format!("concept:{concept}"))
            .bind(nucleus::execution::now().to_rfc3339())
            .execute(&self.store.pool)
            .await?;
            meanings.insert(name.clone(), concept);
        }
        if created {
            self.act(
                Action::CreateRecordDraft {
                    draft: crate::record_creation::Draft {
                        uid: record.uid.clone(),
                        head: record.head.clone(),
                        body: record.body.clone(),
                        slug: record.slug.clone(),
                        quantity: quantity.clone(),
                        ..Default::default()
                    },
                },
                None,
            )
            .await?;
        } else {
            let previous = previous.as_ref().unwrap();
            if previous.head != record.head || previous.body != record.body {
                self.act(
                    Action::EditRecordText {
                        target: record.uid.clone(),
                        head: Some(record.head.clone()),
                        body: Some(record.body.clone()),
                    },
                    None,
                )
                .await?;
            }
            if previous.slug != record.slug {
                self.act(
                    Action::SetSlug {
                        target: record.uid.clone(),
                        slug: record.slug.clone(),
                    },
                    None,
                )
                .await?;
            }
        }
        if previous
            .as_ref()
            .and_then(|p| p.quantity.as_ref())
            .map(|q| &q.1)
            != Some(unit)
        {
            self.act(
                Action::SetUnit {
                    target: record.uid.clone(),
                    unit: unit.as_ref().and_then(|name| meanings.get(name).cloned()),
                },
                None,
            )
            .await?;
        }
        if previous
            .as_ref()
            .and_then(|p| p.quantity.as_ref())
            .map(|q| &q.0)
            != Some(quantity)
        {
            self.act(
                Action::SetQuantityExact {
                    target: record.uid.clone(),
                    amount: quantity.clone(),
                },
                None,
            )
            .await?;
        }
        let old_assertions = previous
            .as_ref()
            .map(|r| r.assertions.as_slice())
            .unwrap_or_default();
        if old_assertions != record.assertions {
            self.act(
                Action::SetIdentity {
                    subject: record.uid.clone(),
                    predicate: None,
                },
                None,
            )
            .await?;
            for old in old_assertions
                .iter()
                .filter(|old| !record.assertions.contains(old))
            {
                self.act(
                    Action::RetractRecord {
                        subject: record.uid.clone(),
                        predicate: store::vocabulary::resolve_file_meaning(
                            &self.store.pool,
                            &old.predicate,
                            &record.uid,
                        )
                        .await?
                        .ok_or_else(|| invalid("The previously selected meaning is unavailable"))?,
                        object: old.object_uri.clone().or_else(|| old.object_uid.clone()),
                    },
                    None,
                )
                .await?;
            }
            for assertion in record
                .assertions
                .iter()
                .filter(|a| !old_assertions.contains(a))
            {
                self.act(
                    Action::AssertRecord {
                        subject: record.uid.clone(),
                        predicate: meanings[&assertion.predicate].clone(),
                        object: assertion
                            .object_uri
                            .clone()
                            .or_else(|| assertion.object_uid.clone()),
                        quantity: assertion.quantity.clone(),
                        unit: assertion
                            .unit
                            .as_ref()
                            .and_then(|name| meanings.get(name).cloned()),
                    },
                    None,
                )
                .await?;
            }
            if let Some(identity) = record.assertions.iter().find(|a| a.identity) {
                self.act(
                    Action::SetIdentity {
                        subject: record.uid.clone(),
                        predicate: Some(meanings[&identity.predicate].clone()),
                    },
                    None,
                )
                .await?;
            }
        }
        Ok(created)
    }

    pub(super) async fn lingua_sync_pass(
        &self,
        dir: &Path,
        organ: &str,
        config: Option<&serde_json::Value>,
        state: &mut State,
    ) -> Result<FileSyncReport, EngineError> {
        let mut report = FileSyncReport::default();
        let disk = scan_disk(dir, "lingua")?;
        let mut documents = std::collections::BTreeMap::new();
        let mut paths: Vec<_> = disk.keys().cloned().collect();
        paths.sort();
        for path in &paths {
            match self
                .read_lingua_document(&disk[path], state.files.get(path))
                .await
            {
                Ok(document) => {
                    documents.insert(path.clone(), document);
                }
                Err(error) => conflict(&mut report, path, error),
            }
        }
        let mut identities: HashMap<String, String> = store::records::list_all(&self.store.pool)
            .await?
            .into_iter()
            .filter_map(|r| r.slug.map(|slug| (slug, r.uid)))
            .collect();
        for row in store::records::list_all(&self.store.pool).await? {
            identities.insert(row.uid.clone(), row.uid);
        }
        let mut declared = HashMap::new();
        let mut duplicate = HashSet::new();
        let mut slugs = HashMap::new();
        for (path, document) in &documents {
            let entries = document
                .records
                .iter()
                .map(|r| (&r.uid, r.slug.as_ref()))
                .chain(
                    document
                        .automation
                        .frequencies
                        .iter()
                        .map(|f| (&f.uid, Some(&f.slug))),
                )
                .chain(
                    document
                        .automation
                        .rules
                        .iter()
                        .map(|r| (&r.uid, Some(&r.slug))),
                );
            for (uid, slug) in entries {
                if let Some(other) = declared.insert(uid.clone(), path.clone()) {
                    duplicate.insert(other);
                    duplicate.insert(path.clone());
                }
                let is_rule = document
                    .automation
                    .rules
                    .iter()
                    .any(|rule| &rule.uid == uid);
                if !is_rule {
                    identities.insert(uid.clone(), uid.clone());
                }
                if let Some(slug) = slug {
                    if let Some(other) = slugs.insert(slug.clone(), path.clone()) {
                        duplicate.insert(other);
                        duplicate.insert(path.clone());
                    }
                    if identities.get(slug).is_some_and(|old| old != uid) {
                        duplicate.insert(path.clone());
                    } else if !is_rule {
                        identities.insert(slug.clone(), uid.clone());
                    }
                }
            }
        }
        let frequency_uids: HashSet<_> = store::karma::frequencies::list_handles(&self.store.pool)
            .await?
            .into_iter()
            .map(|handle| handle.record_uid)
            .chain(documents.values().flat_map(|document| {
                document
                    .automation
                    .frequencies
                    .iter()
                    .map(|frequency| frequency.uid.clone())
            }))
            .collect();
        for path in &paths {
            let Some(document) = documents.get_mut(path) else {
                continue;
            };
            let validation = async {
                if duplicate.contains(path) {
                    return Err(invalid(
                        "Duplicate Record uid or slug; the file was kept unchanged",
                    ));
                }
                for extension in &mut document.extensions {
                    if !shared_extension(&extension.namespace){return Err(invalid("This metadata namespace is managed by its dedicated editor"));}
                    extension.target_uid=Some(identities.get(&extension.target_slug).cloned().ok_or_else(||invalid("Unknown Extension target"))?);
                    if !document.records.iter().any(|record|Some(&record.uid)==extension.target_uid.as_ref()){return Err(invalid("Declare metadata alongside its Record"));}
                    if extension.namespace==lince_resources::RESOURCE_NAMESPACE {
                        let base=lince_resources::address(&path.to_string_lossy()).map_err(invalid)?.to_string();
                        let mut resource:lince_resources::Resource=serde_json::from_value(extension.fields.clone())?;
                        resource.uri=lince_resources::resolve(&base,&resource.uri).map_err(invalid)?;
                        for representation in &mut resource.representations{representation.uri=lince_resources::resolve(&base,&representation.uri).map_err(invalid)?;}
                        resource.validate().map_err(invalid)?;
                        extension.fields=serde_json::to_value(resource)?;
                    }
                }
                if let Some(known)=state.files.get(path).filter(|known|known.source!=document.source){
                    let current=self.lingua_extensions(&known.records).await?;
                    if current!=known.extensions&&current!=document.extensions{return Err(invalid("The file and Record metadata both changed; resolve the conflict before syncing"));}
                }
                for frequency in &document.automation.frequencies {
                    if let Some(row) = store::records::get(&self.store.pool, &frequency.uid).await? {
                        if row.organ_uid.as_deref() != Some(organ) || row.kind != "frequency" {
                            return Err(invalid("The Frequency belongs to another Organ or has another type"));
                        }
                    }
                }
                for rule in &mut document.automation.rules {
                    rule.record_uid = Some(identities.get(&rule.record_slug).cloned().ok_or_else(|| invalid(format!("Unknown Rule target @{}", rule.record_slug)))?);
                    if !rule.frequency_slug.is_empty() {
                        rule.frequency_uid = Some(identities.get(&rule.frequency_slug).filter(|uid| frequency_uids.contains(*uid)).cloned().ok_or_else(|| invalid("Unknown Rule frequency"))?);
                    }
                    if let Some(target) = store::records::get(&self.store.pool, rule.record_uid.as_ref().unwrap()).await? {
                        if target.organ_uid.as_deref() != Some(organ) { return Err(invalid("The Rule target belongs to another Organ")); }
                    }
                    if let Some(source) = &rule.condition {
                        for read in nucleus::karma::Condition::parse(source).map_err(invalid)?.reads() {
                            if read.func != nucleus::expr::ASSERTION {
                                for (position,slug) in read.slug.split('|').enumerate() {
                                    if read.func==nucleus::expr::HAS_ASSERTION&&position==1 {continue;}
                                    if identities.get(slug).is_none_or(|uid| read.func == "freq" && !frequency_uids.contains(uid)) {
                                        return Err(invalid(format!("Unknown condition reference @{slug}")));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(current) = store::recurrence::get(&self.store.pool, &rule.uid).await? {
                        if Some(&current.record_uid) != rule.record_uid.as_ref() { return Err(invalid("The Rule belongs to another Record")); }
                    }
                }
                if let Some(known) = state.files.get(path).filter(|known| known.source != document.source) {
                    if known.automation.uids().next().is_some() {
                        let current = self.current_lingua_automation(known).await?;
                        if current != known.automation && current != document.automation {
                            return Err(invalid("The file and Karma both changed; resolve the conflict before syncing"));
                        }
                    }
                }
                for record in &mut document.records {
                    for assertion in &mut record.assertions {
                        if let Some(uri) = &mut assertion.object_uri {
                            *uri = lince_resources::resolve(&lince_resources::Reference::path(path).map_err(invalid)?.target, uri).map_err(invalid)?;
                        }
                        if let Some(slug) = &assertion.object_slug {
                            assertion.object_uid = Some(
                                identities
                                    .get(slug)
                                    .cloned()
                                    .ok_or_else(|| invalid(format!("Unknown Record @{slug}")))?,
                            );
                        }
                    }
                    normalize(record)?;
                    if let Err(reason) = self.check_body_links(&record.body, &declared.keys().cloned().collect()).await? { return Err(invalid(reason)); }
                    if let Some(known) = state.files.get(path).filter(|known| known.source != document.source) {
                        if let Some(previous) = known.records.iter().find(|r| r.uid == record.uid) {
                            let current = self.lingua_record(&record.uid).await?;
                            if current.as_ref() != Some(previous) && current.as_ref() != Some(record) {
                                return Err(invalid("The file and Record both changed; resolve the conflict before syncing"));
                            }
                        }
                    }
                    if let Some(row) = store::records::get(&self.store.pool, &record.uid).await? {
                        if row.organ_uid.as_deref() != Some(organ) {
                            return Err(invalid("The Record belongs to another Organ"));
                        }
                    }
                }
                Ok::<_, EngineError>(())
            }
            .await;
            if let Err(error) = validation {
                conflict(&mut report, path, error);
                documents.remove(path);
            }
        }
        let mut failed = Vec::new();
        for (path, document) in &documents {
            if state
                .files
                .get(path)
                .is_none_or(|known| known.source != document.source)
            {
                if let Err(error) = self
                    .apply_lingua_frequencies(&document.automation, &mut report)
                    .await
                {
                    if matches!(error, EngineError::ExecutionLimit(_)) {
                        return Err(error);
                    }
                    conflict(&mut report, path, error);
                    failed.push(path.clone());
                }
            }
        }
        for path in failed {
            documents.remove(&path);
        }
        for (path, document) in &documents {
            if state
                .files
                .get(path)
                .is_some_and(|known| known.source == document.source)
            {
                continue;
            }
            for record in &document.records {
                if store::records::get(&self.store.pool, &record.uid)
                    .await?
                    .is_none()
                {
                    self.act(
                        Action::CreateRecordDraft {
                            draft: crate::record_creation::Draft {
                                uid: record.uid.clone(),
                                head: record.head.clone(),
                                body: record.body.clone(),
                                slug: record.slug.clone(),
                                quantity: record.quantity.as_ref().unwrap().0.clone(),
                                ..Default::default()
                            },
                        },
                        None,
                    )
                    .await?;
                    report.created.push(record.uid.clone());
                }
            }
        }
        for path in &paths {
            let Some(mut document) = documents.remove(path) else {
                continue;
            };
            let result = async {
                let known = state.files.get(path);
                let disk_changed = known.is_none_or(|known| known.source != document.source);
                let mut current = Vec::new();
                for record in &document.records {
                    current.push(self.lingua_record(&record.uid).await?);
                }
                if disk_changed {
                    for (record, current) in document.records.iter().zip(&current) {
                        if current.as_ref() == Some(record) {
                            continue;
                        }
                        self.apply_lingua_record(record).await?;
                        if !report.created.contains(&record.uid) {
                            report.updated_from_disk.push(record.uid.clone());
                        }
                    }
                    self.apply_lingua_rules(&document.automation, &mut report)
                        .await?;
                    for extension in &document.extensions {
                        self.act(
                            Action::SetExtension {
                                target: extension
                                    .target_uid
                                    .clone()
                                    .ok_or_else(|| invalid("Missing metadata target"))?,
                                namespace: extension.namespace.clone(),
                                fds: extension.fields.clone(),
                            },
                            None,
                        )
                        .await?;
                    }
                    if let Some(known) = known {
                        for extension in &known.extensions {
                            if !document.extensions.iter().any(|current| {
                                current.target_uid == extension.target_uid
                                    && current.namespace == extension.namespace
                            }) {
                                self.act(
                                    Action::SetExtension {
                                        target: extension
                                            .target_uid
                                            .clone()
                                            .ok_or_else(|| invalid("Missing metadata target"))?,
                                        namespace: extension.namespace.clone(),
                                        fds: serde_json::json!({}),
                                    },
                                    None,
                                )
                                .await?;
                            }
                        }
                    }
                    if let Some(known) = known {
                        for old in &known.automation.rules {
                            if !declared.contains_key(&old.uid) {
                                self.act(
                                    Action::DeleteRecurrence {
                                        recurrence: old.uid.clone(),
                                    },
                                    None,
                                )
                                .await?;
                                report.deleted.push(old.uid.clone());
                            }
                        }
                        for old in &known.automation.frequencies {
                            if !declared.contains_key(&old.uid) {
                                self.act(
                                    Action::DeleteFrequency {
                                        frequency: old.uid.clone(),
                                    },
                                    None,
                                )
                                .await?;
                                report.deleted.push(old.uid.clone());
                            }
                        }
                    }
                    if let Some(known) = known {
                        for old in &known.records {
                            if !declared.contains_key(&old.uid) {
                                self.act(
                                    Action::DeleteRecord {
                                        target: old.uid.clone(),
                                    },
                                    None,
                                )
                                .await?;
                                report.deleted.push(old.uid.clone());
                            }
                        }
                    }
                }
                let mut automation = self.current_lingua_automation(&document).await?;
                automation
                    .rules
                    .retain(|rule| declared.get(&rule.uid).is_none_or(|owner| owner == path));
                let mut records = Vec::new();
                for record in &document.records {
                    if !automation.frequencies.iter().any(|f| f.uid == record.uid) {
                        if let Some(record) = self.lingua_record(&record.uid).await? {
                            records.push(record);
                        }
                    }
                }
                let extensions = self.lingua_extensions(&records).await?;
                let changed = records != document.records
                    || automation != document.automation
                    || extensions != document.extensions;
                let typed = automation.uids().next().is_some();
                if changed || (typed && disk_changed) {
                    let text = render_document(
                        &document.identified,
                        &records,
                        &automation,
                        &extensions,
                        path,
                    )?;
                    if text != document.source {
                        write_document(path, Some(&document.source), &text)?;
                        report.written_to_disk.extend(
                            records
                                .iter()
                                .map(|r| r.uid.clone())
                                .chain(automation.uids().cloned()),
                        );
                    }
                    document.source = text.clone();
                    document.identified = text;
                    document.records = records;
                    document.automation = automation;
                    document.extensions = extensions;
                }
                Ok::<_, EngineError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    self.remember_lingua_source(path, &document).await?;
                    state.files.insert(path.clone(), document);
                }
                Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
                Err(error) => conflict(&mut report, path, error),
            }
        }
        let missing: Vec<_> = state
            .files
            .keys()
            .filter(|path| !disk.contains_key(*path))
            .cloned()
            .collect();
        'missing: for path in missing {
            let document = state.files.get_mut(&path).unwrap();
            document.missing += 1;
            if document.missing < MISSING_TICKS_BEFORE_DELETE {
                continue;
            }
            for rule in &document.automation.rules {
                if !declared.contains_key(&rule.uid)
                    && store::recurrence::get(&self.store.pool, &rule.uid)
                        .await?
                        .is_some()
                {
                    self.act(
                        Action::DeleteRecurrence {
                            recurrence: rule.uid.clone(),
                        },
                        None,
                    )
                    .await?;
                    report.deleted.push(rule.uid.clone());
                }
            }
            for frequency in &document.automation.frequencies {
                if !declared.contains_key(&frequency.uid)
                    && store::records::get(&self.store.pool, &frequency.uid)
                        .await?
                        .is_some()
                {
                    match self
                        .act(
                            Action::DeleteFrequency {
                                frequency: frequency.uid.clone(),
                            },
                            None,
                        )
                        .await
                    {
                        Ok(_) => report.deleted.push(frequency.uid.clone()),
                        Err(error) => {
                            conflict(&mut report, &path, error);
                            continue 'missing;
                        }
                    }
                }
            }
            for record in &document.records {
                if !declared.contains_key(&record.uid)
                    && store::records::get(&self.store.pool, &record.uid)
                        .await?
                        .is_some()
                {
                    self.act(
                        Action::DeleteRecord {
                            target: record.uid.clone(),
                        },
                        None,
                    )
                    .await?;
                    report.deleted.push(record.uid.clone());
                }
            }
            state.files.remove(&path);
        }
        let represented: HashSet<_> = state
            .files
            .values()
            .flat_map(|document| {
                document
                    .records
                    .iter()
                    .map(|record| record.uid.clone())
                    .chain(document.automation.uids().cloned())
            })
            .chain(declared.keys().cloned())
            .collect();
        let selected = self.selected_records(organ, config).await?;
        for (path, (uid, _)) in desired_paths(dir, &selected, "lingua") {
            if represented.contains(&uid) || disk.contains_key(&path) {
                continue;
            }
            let result = async {
                let record = self
                    .lingua_record(&uid)
                    .await?
                    .ok_or_else(|| invalid("Record disappeared during sync"))?;
                let mut document = Document {
                    source: String::new(),
                    identified: String::new(),
                    records: vec![record],
                    extensions: Vec::new(),
                    automation: Automation::default(),
                    missing: 0,
                };
                document.automation = self.current_lingua_automation(&document).await?;
                document
                    .automation
                    .rules
                    .retain(|rule| !represented.contains(&rule.uid));
                document.records.retain(|r| {
                    !document
                        .automation
                        .frequencies
                        .iter()
                        .any(|f| f.uid == r.uid)
                });
                document.extensions = self.lingua_extensions(&document.records).await?;
                let text = render_document(
                    "",
                    &document.records,
                    &document.automation,
                    &document.extensions,
                    &path,
                )?;
                write_document(&path, None, &text)?;
                document.source = text.clone();
                document.identified = text;
                self.remember_lingua_source(&path, &document).await?;
                state.files.insert(path.clone(), document);
                report.written_to_disk.push(uid);
                Ok::<_, EngineError>(())
            }
            .await;
            if let Err(error) = result {
                conflict(&mut report, &path, error);
            }
        }
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .insert(organ.into(), report.conflicts.clone());
        Ok(report)
    }
}

fn render_document(
    source: &str,
    records: &[ProjectedRecord],
    automation: &Automation,
    extensions: &[anicca::ProjectedExtension],
    path: &Path,
) -> Result<String, EngineError> {
    let mut document = anicca::parse(source).map_err(invalid)?;
    document.declarations.retain(|declaration| matches!(declaration, ast::Declaration::Record(record) if records.iter().any(|r| Some(r.uid.as_str()) == record.opening.uid())));
    for record in records {
        let (quantity, unit) = record.quantity.as_ref().unwrap();
        let subject = format!(
            "{}{}{}",
            record
                .slug
                .as_ref()
                .map(|s| format!("@{s}: "))
                .unwrap_or_default(),
            quantity,
            unit.as_ref().map(|u| format!(" @{u}")).unwrap_or_default()
        );
        let skeleton = format!("Record ({subject}) {{ {}\n}} {}\n", record.uid, record.uid);
        let mut parsed = anicca::parse(&skeleton).map_err(invalid)?;
        let ast::Declaration::Record(template) = parsed.declarations.remove(0) else {
            unreachable!()
        };
        match document.declarations.iter_mut().find(
            |d| matches!(d, ast::Declaration::Record(r) if r.opening.uid() == Some(&record.uid)),
        ) {
            Some(ast::Declaration::Record(existing)) => {
                existing.header.subject = template.header.subject;
            }
            _ => document
                .declarations
                .push(ast::Declaration::Record(template)),
        }
        let mut assertions = record.assertions.clone();
        let base = lince_resources::address(
            &lince_resources::Reference::path(path)
                .map_err(invalid)?
                .target,
        )
        .map_err(invalid)?;
        for assertion in &mut assertions {
            if let Some(uri) = &mut assertion.object_uri
                && let Ok(target) = lince_resources::address(uri)
                && let Some(relative) = base.make_relative(&target)
            {
                *uri = relative;
            }
        }
        let updated = anicca::set_record_runtime(
            &anicca::format(&document),
            &record.uid,
            &record.head,
            &record.body,
            quantity,
            &assertions,
        )
        .map_err(invalid)?;
        document = anicca::parse(&updated).map_err(invalid)?;
    }
    let mut text = anicca::format(&document);
    for extension in extensions {
        let mut fields = extension.fields.clone();
        if extension.namespace == lince_resources::RESOURCE_NAMESPACE {
            let base = lince_resources::address(&path.to_string_lossy()).map_err(invalid)?;
            for property in ["uri"] {
                if let Some(uri) = fields[property]
                    .as_str()
                    .and_then(|uri| lince_resources::address(uri).ok())
                    && let Some(relative) = base.make_relative(&uri)
                {
                    fields[property] = serde_json::json!(relative);
                }
            }
            if let Some(representations) = fields["representations"].as_array_mut() {
                for representation in representations {
                    if let Some(uri) = representation["uri"]
                        .as_str()
                        .and_then(|uri| lince_resources::address(uri).ok())
                        && let Some(relative) = base.make_relative(&uri)
                    {
                        representation["uri"] = serde_json::json!(relative);
                    }
                }
            }
        }
        text.push_str(
            &anicca::extension::render(&extension.target_slug, &extension.namespace, &fields)
                .map_err(invalid)?,
        );
    }
    text.push_str(&automation.render()?);
    let document = anicca::parse(&text).map_err(invalid)?;
    anicca::project(&document).map_err(invalid)?;
    Ok(anicca::format(&document))
}

fn shared_extension(namespace: &str) -> bool {
    !namespace.starts_with("lince.")
        || matches!(
            namespace,
            lince_resources::RESOURCE_NAMESPACE
                | lince_resources::SOURCE_NAMESPACE
                | "lince.practice"
                | "lince.controls"
                | "lince.vocabulary"
        )
}

fn write_document(path: &Path, expected: Option<&str>, text: &str) -> Result<(), EngineError> {
    let existing = match std::fs::read_to_string(path) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if existing.as_deref() != expected {
        return Err(invalid(
            "The file changed during sync; it will be read again",
        ));
    }
    let temporary = path.with_extension(format!("{}.tmp", nucleus::new_uid("sync")));
    std::fs::write(&temporary, text)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(temporary);
        return Err(error.into());
    }
    Ok(())
}
