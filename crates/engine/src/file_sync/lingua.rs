use super::*;
use anicca::{ProjectedAssertion, ProjectedRecord, grammar::grammar as ast};

#[derive(Debug, Default)]
pub(super) struct State {
    files: HashMap<PathBuf, Document>,
}

#[derive(Debug, Clone)]
struct Document {
    source: String,
    identified: String,
    records: Vec<ProjectedRecord>,
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
    record
        .assertions
        .sort_by_key(|value| format!("{}:{:?}", value.predicate, value.object_uid));
    Ok(())
}

impl Engine {
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
                return Err(invalid(
                    "Directory sync currently accepts Record declarations; Frequency, Rule and Extension declarations are not imported",
                ));
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
                        object: a.object_slug.clone(),
                        quantity: a.quantity.clone(),
                        unit: a.unit.clone(),
                    })
                    .collect(),
                ..Default::default()
            }
            .validate()?;
        }
        Ok(Document {
            source: source.into(),
            identified: anicca::format(&document),
            records: projected.records,
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

    async fn apply_lingua_record(&self, record: &ProjectedRecord) -> Result<bool, EngineError> {
        let previous = self.lingua_record(&record.uid).await?;
        if previous.as_ref() == Some(record) {
            return Ok(false);
        }
        let created = previous.is_none();
        let (quantity, unit) = record.quantity.as_ref().unwrap();
        for name in record
            .assertions
            .iter()
            .map(|a| &a.predicate)
            .chain(record.assertions.iter().filter_map(|a| a.unit.as_ref()))
            .chain(unit.iter())
        {
            if store::concepts::resolve(&self.store.pool, name)
                .await?
                .is_none()
            {
                self.act(
                    Action::CreateConcept {
                        lingua: store::linguas::LOCAL_UID.into(),
                        name: name.clone(),
                        parents: vec![],
                    },
                    None,
                )
                .await?;
            }
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
                    unit: unit.clone(),
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
                        predicate: old.predicate.clone(),
                        object: old.object_uid.clone(),
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
                        predicate: assertion.predicate.clone(),
                        object: assertion.object_uid.clone(),
                        quantity: assertion.quantity.clone(),
                        unit: assertion.unit.clone(),
                    },
                    None,
                )
                .await?;
            }
            if let Some(identity) = record.assertions.iter().find(|a| a.identity) {
                self.act(
                    Action::SetIdentity {
                        subject: record.uid.clone(),
                        predicate: Some(identity.predicate.clone()),
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
        let mut documents = HashMap::new();
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
        let mut declared = HashMap::new();
        let mut duplicate = HashSet::new();
        for (path, document) in &documents {
            for record in &document.records {
                if let Some(other) = declared.insert(record.uid.clone(), path.clone()) {
                    duplicate.insert(other);
                    duplicate.insert(path.clone());
                }
                if let Some(slug) = &record.slug {
                    if identities.get(slug).is_some_and(|uid| uid != &record.uid) {
                        duplicate.insert(path.clone());
                    } else {
                        identities.insert(slug.clone(), record.uid.clone());
                    }
                }
            }
        }
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
                for record in &mut document.records {
                    for assertion in &mut record.assertions {
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
                } else {
                    let records: Vec<_> = current.into_iter().flatten().collect();
                    if records != document.records {
                        let text = render_document(&document.identified, &records)?;
                        write_document(path, Some(&document.source), &text)?;
                        report
                            .written_to_disk
                            .extend(records.iter().map(|r| r.uid.clone()));
                        document.source = text.clone();
                        document.identified = text;
                        document.records = records;
                    }
                }
                Ok::<_, EngineError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    state.files.insert(path.clone(), document);
                }
                Err(error) => conflict(&mut report, path, error),
            }
        }
        let missing: Vec<_> = state
            .files
            .keys()
            .filter(|path| !disk.contains_key(*path))
            .cloned()
            .collect();
        for path in missing {
            let document = state.files.get_mut(&path).unwrap();
            document.missing += 1;
            if document.missing < MISSING_TICKS_BEFORE_DELETE {
                continue;
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
            .flat_map(|document| document.records.iter().map(|record| record.uid.clone()))
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
                let text = render_document("", std::slice::from_ref(&record))?;
                write_document(&path, None, &text)?;
                state.files.insert(
                    path.clone(),
                    Document {
                        source: text.clone(),
                        identified: text,
                        records: vec![record],
                        missing: 0,
                    },
                );
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

fn render_document(source: &str, records: &[ProjectedRecord]) -> Result<String, EngineError> {
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
        let updated = anicca::set_record_runtime(
            &anicca::format(&document),
            &record.uid,
            &record.head,
            &record.body,
            quantity,
            &record.assertions,
        )
        .map_err(invalid)?;
        document = anicca::parse(&updated).map_err(invalid)?;
    }
    let text = anicca::format(&document);
    anicca::project(&document).map_err(invalid)?;
    Ok(text)
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
