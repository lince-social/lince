use super::*;
use loro::ExportMode;
use store::record_edit_drafts::Metadata;

pub(super) type Retired = (
    Arc<engine::Engine>,
    String,
    String,
    store::record_edit_drafts::Stamp,
);

pub(super) struct Cleanup {
    task: tokio::task::JoinHandle<()>,
    done: Arc<std::sync::atomic::AtomicBool>,
}

impl Cleanup {
    pub fn start(retired: Vec<Retired>, wake: Option<crate::native::wake::WakeSignal>) -> Self {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = done.clone();
        let task = tokio::spawn(async move {
            for (engine, source, record, stamp) in retired {
                for attempt in 0..3 {
                    match engine
                        .retire_record_edit_draft(&source, &record, &stamp)
                        .await
                    {
                        Ok(_) => break,
                        Err(error) => {
                            if attempt == 2 {
                                warn!("Could not retire an unused recovery checkpoint: {error}");
                            } else {
                                tokio::time::sleep(Duration::from_millis(100 << attempt)).await;
                            }
                        }
                    }
                }
            }
            finished.store(true, std::sync::atomic::Ordering::Release);
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        Self { task, done }
    }

    pub fn finished(&self) -> bool {
        self.done.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn retire(world: &World, key: &Key, document: &Document, retired: &mut Vec<Retired>) {
    if let Some(metadata) = &document.journal.metadata
        && metadata.pending.is_empty()
        && let Some(engine) = crate::native::practice_cells::engine(world, &document.source)
    {
        retired.push((
            engine,
            storage_source(key, document.principal.as_deref()),
            key.1.clone(),
            metadata.stamp.clone(),
        ));
    }
}

pub(super) struct Journal {
    pub instance: String,
    pub metadata: Option<Metadata>,
    pub failed: bool,
    generation: String,
    checkpoint: Option<std::sync::Arc<String>>,
    initializing: bool,
    version: VersionVector,
    atoms: usize,
    pending_imports: Vec<Vec<u8>>,
}

pub(super) struct Cursor {
    version: VersionVector,
    atoms: usize,
    imports: usize,
}

impl Default for Journal {
    fn default() -> Self {
        let instance = nucleus::new_uid("ed");
        Self {
            generation: instance.clone(),
            instance,
            metadata: None,
            failed: false,
            checkpoint: None,
            initializing: true,
            version: VersionVector::default(),
            atoms: 0,
            pending_imports: Vec::new(),
        }
    }
}

impl Journal {
    pub fn loaded(&mut self, metadata: Option<Metadata>, restored: bool, doc: &LoroDoc) {
        if restored {
            self.generation = metadata.as_ref().unwrap().stamp.generation.clone();
            self.initializing = false;
        }
        self.metadata = metadata;
        self.version = doc.oplog_vv();
        self.atoms = doc.len_ops();
    }

    pub fn base(&mut self, snapshot: String, empty_before: bool, doc: &LoroDoc) -> bool {
        if self.initializing
            && self.checkpoint.is_none()
            && self.pending_imports.is_empty()
            && empty_before
        {
            self.checkpoint = Some(std::sync::Arc::new(snapshot));
            self.version = doc.oplog_vv();
            self.atoms = doc.len_ops();
            true
        } else {
            false
        }
    }

    pub fn pending_import(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let total = self
            .pending_imports
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            .saturating_add(bytes.len());
        if self.pending_imports.len() >= 4096 || total > store::record_edit_drafts::MAX_BYTES {
            return Err("Pending collaboration history exceeds its recovery limit".into());
        }
        self.pending_imports.push(bytes);
        Ok(())
    }

    pub fn prepare(
        &mut self,
        doc: &LoroDoc,
        pending: &VecDeque<std::sync::Arc<engine::record_change::Request>>,
    ) -> Result<(engine::record_edit::Write, Cursor), String> {
        if self.initializing && self.checkpoint.is_none() {
            let bytes = doc
                .export(ExportMode::Snapshot)
                .map_err(|error| error.to_string())?;
            performance::count(Count::RecordDraftSnapshot);
            performance::add(Count::RecordDraftSnapshotBytes, bytes.len() as u64);
            let snapshot = B64.encode(bytes);
            performance::add(Count::RecordDraftEncodedBytes, snapshot.len() as u64);
            self.checkpoint = Some(std::sync::Arc::new(snapshot));
            self.version = doc.oplog_vv();
            self.atoms = doc.len_ops();
        }
        let version = doc.oplog_vv();
        let mut updates = Vec::new();
        if version != self.version {
            updates.push(
                doc.export(ExportMode::updates(&self.version))
                    .map_err(|error| error.to_string())?,
            );
        }
        updates.extend(self.pending_imports.iter().cloned());
        for update in &updates {
            performance::count(Count::RecordDraftDelta);
            performance::add(Count::RecordDraftDeltaBytes, update.len() as u64);
        }
        if let Some(checkpoint) = &self.checkpoint {
            performance::add(Count::RecordDraftCheckpointBytes, checkpoint.len() as u64);
        }
        let atoms = doc.len_ops();
        Ok((
            engine::record_edit::Write {
                expected: self.metadata.clone(),
                generation: self.generation.clone(),
                checkpoint: self.checkpoint.clone(),
                pending: pending.iter().cloned().collect(),
                updates,
                atoms: atoms.saturating_sub(self.atoms),
            },
            Cursor {
                version,
                atoms,
                imports: self.pending_imports.len(),
            },
        ))
    }

    pub fn accept(&mut self, metadata: Metadata, cursor: Cursor) {
        self.metadata = Some(metadata);
        self.version = cursor.version;
        self.atoms = cursor.atoms;
        self.pending_imports.drain(..cursor.imports);
        self.initializing = false;
        self.checkpoint = None;
        self.failed = false;
    }
}

crate::native::laboratory_cases! {
    journal_updates_remain_incremental_across_acknowledgements,
}

#[cfg_attr(test, test)]
fn journal_updates_remain_incremental_across_acknowledgements() {
    let mut seed = 0x517cc1b727220a95_u64;
    let mut body: String = (0..65536)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            char::from(b'a' + (seed % 26) as u8)
        })
        .collect();
    let mut head = "Local title".to_string();
    let mut document = Document::new(crate::native::protein_area::Source::Local, None);
    document.doc.get_text("head").insert(0, &head).unwrap();
    document.doc.get_text("body").insert(0, &body).unwrap();
    document.doc.commit();
    let checkpoint = document.doc.export(ExportMode::Snapshot).unwrap();
    let replica = LoroDoc::new();
    replica.import(&checkpoint).unwrap();
    assert!(
        document
            .journal
            .base(B64.encode(checkpoint), true, &document.doc)
    );
    let uid = nucleus::new_uid("r");
    let mut sequence = 0;
    let mut checkpoint_bytes = 0;
    let mut total_updates = 0;
    let mut total_bytes = 0;
    for property in ["head", "body"] {
        for inserted in ["é", "新", "x"] {
            let before = document.doc.oplog_vv();
            document.doc.get_text(property).insert(0, inserted).unwrap();
            document.doc.commit();
            document.queue(&uid, &before).unwrap();
            if property == "head" {
                head.insert_str(0, inserted);
            } else {
                body.insert_str(0, inserted);
            }
            let (write, cursor) = document
                .journal
                .prepare(&document.doc, &document.pending)
                .unwrap();
            let bytes: usize = write.updates.iter().map(Vec::len).sum();
            assert!(bytes > 0 && bytes < 4096, "{bytes}");
            if sequence == 0 {
                checkpoint_bytes = write.checkpoint.as_ref().unwrap().len();
                assert!(checkpoint_bytes > 65536);
            } else {
                assert!(write.checkpoint.is_none());
            }
            replica.import_batch(&write.updates).unwrap();
            assert_eq!(replica.get_text("head").to_string(), head);
            assert_eq!(replica.get_text("body").to_string(), body);
            total_updates += write.updates.len();
            total_bytes += bytes;
            sequence += 1;
            let metadata = Metadata {
                stamp: store::record_edit_drafts::Stamp {
                    generation: write.generation,
                    sequence,
                },
                pending: write
                    .pending
                    .iter()
                    .map(|request| request.id.clone())
                    .collect(),
                update_count: total_updates,
                update_bytes: total_bytes,
                request_bytes: 0,
                checkpoint_bytes,
                write_hash: String::new(),
            };
            document.journal.accept(metadata.clone(), cursor);
            document.pending.clear();
            let (ack, cursor) = document
                .journal
                .prepare(&document.doc, &document.pending)
                .unwrap();
            assert!(ack.checkpoint.is_none() && ack.updates.is_empty() && ack.pending.is_empty());
            sequence += 1;
            document.journal.accept(
                Metadata {
                    stamp: store::record_edit_drafts::Stamp {
                        sequence,
                        ..metadata.stamp
                    },
                    pending: Vec::new(),
                    ..metadata
                },
                cursor,
            );
        }
    }
}
