use crate::{Edit, MAX_FILE_BYTES, MAX_WINDOW_BYTES, Result, diff::changes};
use loro::{LoroDoc, Subscription, TextDelta, UndoManager, cursor::Cursor};
use ropey::Rope;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Conflict {
    pub disk: String,
}

pub enum Resolution {
    Local,
    Disk,
}

pub struct SavePoint {
    pub text: Rope,
    pub revision: u64,
    doc: LoroDoc,
    from: loro::VersionVector,
    to: loro::VersionVector,
}

pub struct EncodedSave {
    pub text: Rope,
    pub revision: u64,
    update: loro::JsonSchema,
}

impl SavePoint {
    pub fn encode(self) -> EncodedSave {
        EncodedSave {
            update: self.doc.export_json_updates(&self.from, &self.to),
            text: self.text,
            revision: self.revision,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextWindow {
    pub identity: u64,
    pub start: usize,
    pub end: usize,
    pub first_line: usize,
    pub total_lines: usize,
    pub text: String,
    pub clipped: bool,
    pub revision: u64,
}

pub struct Buffer {
    identity: u64,
    doc: LoroDoc,
    disk: LoroDoc,
    rope: Arc<Mutex<Rope>>,
    undo: UndoManager,
    undo_boundary: bool,
    _subscription: Subscription,
    baseline: Arc<str>,
    observed: Arc<str>,
    revision: u64,
    saved: u64,
    conflict: Option<Conflict>,
}

#[derive(Clone)]
pub struct Checkpoint {
    pub(crate) baseline: Arc<str>,
    pub(crate) observed: Arc<str>,
    pub(crate) working: Rope,
}

impl Checkpoint {
    pub fn unsaved(value: &str) -> Result<Self> {
        if value.len() > MAX_FILE_BYTES {
            return Err("Recovery text exceeds the editing limit".into());
        }
        Ok(Self {
            baseline: "".into(),
            observed: "".into(),
            working: Rope::from_str(value),
        })
    }
    pub fn restore(&self) -> Result<Buffer> {
        let mut buffer = Buffer::new(&self.baseline)?;
        for edit in changes(&self.baseline, &self.working.to_string())
            .into_iter()
            .rev()
        {
            buffer.edit(edit)?;
        }
        let plan = buffer.reconciliation().compute(self.observed.to_string())?;
        buffer.accept(plan)?;
        Ok(buffer)
    }
}

impl Buffer {
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            baseline: self.baseline.clone(),
            observed: self.observed.clone(),
            working: self.snapshot(),
        }
    }
    pub fn new(value: &str) -> Result<Self> {
        if value.len() > MAX_FILE_BYTES {
            return Err("This file exceeds the 16 MiB editing limit".into());
        }
        let disk = LoroDoc::new();
        disk.get_text("text")
            .insert(0, value)
            .map_err(|e| e.to_string())?;
        disk.commit();
        let doc = LoroDoc::new();
        doc.import_json_updates(disk.export_json_updates(&Default::default(), &disk.oplog_vv()))
            .map_err(|e| e.to_string())?;
        disk.set_peer_id(LoroDoc::new().peer_id())
            .map_err(|e| e.to_string())?;
        let rope = Arc::new(Mutex::new(Rope::from_str(value)));
        let projection = rope.clone();
        let subscription = doc.subscribe_root(Arc::new(move |event| {
            let mut rope = projection.lock().expect("text projection");
            for event in event.events {
                if let loro::event::Diff::Text(delta) = &event.diff {
                    let mut position = 0;
                    for part in delta {
                        match part {
                            TextDelta::Retain { retain, .. } => position += retain,
                            TextDelta::Insert { insert, .. } => {
                                rope.insert(position, insert);
                                position += insert.chars().count();
                            }
                            TextDelta::Delete { delete } => {
                                rope.remove(position..position + delete)
                            }
                        }
                    }
                }
            }
        }));
        let mut undo = UndoManager::new(&doc);
        undo.set_max_undo_steps(256);
        undo.set_merge_interval(500);
        Ok(Self {
            identity: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            doc,
            disk,
            rope,
            undo,
            undo_boundary: false,
            _subscription: subscription,
            baseline: value.into(),
            observed: value.into(),
            revision: 0,
            saved: 0,
            conflict: None,
        })
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved
    }
    pub fn conflict(&self) -> Option<&Conflict> {
        self.conflict.as_ref()
    }
    pub fn snapshot(&self) -> Rope {
        self.rope.lock().expect("text projection").clone()
    }
    pub fn text(&self) -> String {
        self.snapshot().to_string()
    }
    pub fn observed(&self) -> &str {
        &self.observed
    }

    pub fn edit(&mut self, edit: Edit) -> Result<()> {
        self.edit_many(&[edit])
    }

    pub fn edit_many(&mut self, edits: &[Edit]) -> Result<()> {
        let rope = self.snapshot();
        let mut end = 0;
        let mut bytes = rope.len_bytes();
        for edit in edits {
            if edit.range.start < end
                || edit.range.start > edit.range.end
                || edit.range.end > rope.len_chars()
            {
                return Err("The edited ranges are overlapping or no longer available".into());
            }
            end = edit.range.end;
            bytes = bytes
                .checked_sub(rope.char_to_byte(end) - rope.char_to_byte(edit.range.start))
                .and_then(|bytes| bytes.checked_add(edit.text.len()))
                .ok_or("The edit exceeds the editing limit")?;
        }
        if bytes > MAX_FILE_BYTES {
            return Err("The edit exceeds the 16 MiB editing limit".into());
        }
        if !edits.is_empty() {
            self.undo
                .set_merge_interval(if self.undo_boundary { 0 } else { 500 });
            apply(&self.doc, edits)?;
            self.undo_boundary = false;
            self.revision += 1;
        }
        Ok(())
    }

    pub fn edit_batch(&mut self, edits: &[Edit]) -> Result<()> {
        let previous = self.undo_boundary;
        self.undo_boundary = true;
        let result = self.edit_many(edits);
        self.undo_boundary = if result.is_ok() && !edits.is_empty() {
            true
        } else {
            previous
        };
        result
    }

    pub fn undo(&mut self, redo: bool) -> Result<bool> {
        let changed = if redo {
            self.undo.redo()
        } else {
            self.undo.undo()
        }
        .map_err(|e| e.to_string())?;
        if changed {
            self.undo_boundary = true;
            self.revision += 1;
        }
        Ok(changed)
    }

    pub fn anchor(&self, position: usize) -> Option<Cursor> {
        self.doc
            .get_text("text")
            .get_cursor(position, Default::default())
    }

    pub fn position(&self, cursor: &Cursor) -> Option<usize> {
        self.doc
            .get_cursor_pos(cursor)
            .ok()
            .map(|value| value.current.pos)
    }

    pub fn window(&self, first: usize, count: usize) -> TextWindow {
        let rope = self.snapshot();
        let first = first.min(rope.len_lines().saturating_sub(1));
        let start = rope.line_to_char(first);
        let end = if first + count >= rope.len_lines() {
            rope.len_chars()
        } else {
            rope.line_to_char(first + count)
        };
        let start_byte = rope.char_to_byte(start);
        let (actual_end, clipped) = if rope.char_to_byte(end) - start_byte > MAX_WINDOW_BYTES {
            let limit = rope.byte_to_char(start_byte + MAX_WINDOW_BYTES);
            let last_line = rope.char_to_line(limit);
            if last_line > first {
                (rope.line_to_char(last_line), false)
            } else {
                (limit, true)
            }
        } else {
            (end, false)
        };
        let text = rope.slice(start..actual_end).to_string();
        TextWindow {
            identity: self.identity,
            start,
            end: actual_end,
            first_line: first,
            total_lines: rope.len_lines(),
            text,
            clipped,
            revision: self.revision,
        }
    }

    pub fn reconcile(&mut self, value: &str) -> Result<bool> {
        let plan = self.reconciliation().compute(value.into())?;
        self.accept(plan)
    }

    pub fn reconciliation(&self) -> Reconciliation {
        Reconciliation {
            baseline: self.baseline.clone(),
            observed: self.observed.clone(),
            current: self.snapshot(),
            revision: self.revision,
        }
    }

    pub fn accept(&mut self, plan: Reconciled) -> Result<bool> {
        if plan.revision != self.revision || !Arc::ptr_eq(&plan.observed, &self.observed) {
            return Ok(false);
        }
        if plan.value == self.observed.as_ref() && self.conflict.is_none() {
            return Ok(true);
        }
        self.observed = plan.value.clone().into();
        if plan.overlap {
            self.conflict = Some(Conflict { disk: plan.value });
            self.revision += 1;
            return Ok(true);
        }
        apply(&self.disk, &plan.edits)?;
        transfer(&self.disk, &self.doc)?;
        if plan.identical {
            apply(&self.doc, &changes(&self.text(), &plan.value))?;
            transfer(&self.doc, &self.disk)?;
        }
        self.baseline = plan.value.into();
        self.conflict = None;
        self.revision += 1;
        if plan.clean || plan.identical {
            self.saved = self.revision;
        }
        Ok(true)
    }

    pub fn resolve(&mut self, resolution: Resolution) -> Result<()> {
        if self.conflict.is_none() {
            return Ok(());
        }
        let selected = match resolution {
            Resolution::Local => self.text(),
            Resolution::Disk => self.observed.to_string(),
        };
        apply(&self.disk, &changes(&self.baseline, &self.observed))?;
        transfer(&self.disk, &self.doc)?;
        apply(&self.doc, &changes(&self.text(), &selected))?;
        self.baseline = self.observed.clone();
        self.conflict = None;
        self.revision += 1;
        if selected == self.observed.as_ref() {
            self.saved = self.revision;
        }
        Ok(())
    }

    pub fn prepare_save(&self) -> Result<SavePoint> {
        if self.conflict.is_some() {
            return Err("Resolve the disk conflict before saving".into());
        }
        Ok(SavePoint {
            text: self.snapshot(),
            revision: self.revision,
            doc: self.doc.clone(),
            from: self.disk.oplog_vv(),
            to: self.doc.oplog_vv(),
        })
    }

    pub fn saved(&mut self, point: SavePoint) -> Result<()> {
        let text = point.text.to_string().into();
        self.saved_with_text(point.encode(), text)
    }

    pub fn saved_with_text(&mut self, point: EncodedSave, text: Arc<str>) -> Result<()> {
        self.disk
            .import_json_updates(point.update)
            .map_err(|e| e.to_string())?;
        self.observed = text.clone();
        self.baseline = text;
        self.saved = point.revision;
        Ok(())
    }
}

pub struct Reconciliation {
    baseline: Arc<str>,
    observed: Arc<str>,
    current: Rope,
    revision: u64,
}

pub struct Reconciled {
    observed: Arc<str>,
    value: String,
    edits: Vec<Edit>,
    overlap: bool,
    identical: bool,
    clean: bool,
    revision: u64,
}

impl Reconciliation {
    pub fn compute(self, value: String) -> Result<Reconciled> {
        if value.len() > MAX_FILE_BYTES {
            return Err("The changed file exceeds the editing limit".into());
        }
        let current = self.current.to_string();
        let local = changes(&self.baseline, &current);
        let external = changes(&self.baseline, &value);
        Ok(Reconciled {
            observed: self.observed.clone(),
            overlap: current != value
                && local.iter().any(|a| external.iter().any(|b| a.overlaps(b))),
            identical: current == value,
            clean: local.is_empty(),
            edits: external,
            value,
            revision: self.revision,
        })
    }
}

fn apply(doc: &LoroDoc, edits: &[Edit]) -> Result<()> {
    let text = doc.get_text("text");
    for edit in edits.iter().rev() {
        if !edit.range.is_empty() {
            text.delete(edit.range.start, edit.range.len())
                .map_err(|e| e.to_string())?;
        }
        if !edit.text.is_empty() {
            text.insert(edit.range.start, &edit.text)
                .map_err(|e| e.to_string())?;
        }
    }
    doc.commit();
    Ok(())
}

fn transfer(from: &LoroDoc, to: &LoroDoc) -> Result<()> {
    to.import_json_updates(from.export_json_updates(&to.oplog_vv(), &from.oplog_vv()))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests;
