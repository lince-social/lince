use super::{B64, Count, Cursor, LoroDoc, Map, performance};
use base64::Engine as _;
use cell::CollabCursor;
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub(in super::super) struct Cursors {
    snapshot: Option<Arc<Snapshot>>,
    positions: Vec<Option<Option<usize>>>,
}

pub(super) struct Snapshot {
    pub(super) wire: Vec<CollabCursor>,
    prepared: OnceLock<Prepared>,
}

struct Prepared {
    properties: Map<String, Vec<usize>>,
    focus: Vec<OnceLock<Option<Cursor>>>,
}

impl Snapshot {
    fn prepared(&self) -> &Prepared {
        self.prepared.get_or_init(|| {
            let mut properties = Map::<_, Vec<_>>::default();
            for (index, cursor) in self.wire.iter().enumerate() {
                properties
                    .entry_ref(cursor.property.as_str())
                    .or_default()
                    .push(index);
            }
            performance::count(Count::RemoteCursorSnapshotsPrepared);
            Prepared {
                properties,
                focus: (0..self.wire.len()).map(|_| OnceLock::new()).collect(),
            }
        })
    }

    pub(super) fn indices(&self, property: &str) -> &[usize] {
        self.prepared()
            .properties
            .get(property)
            .map_or(&[], Vec::as_slice)
    }

    fn focus(&self, index: usize) -> Option<&Cursor> {
        let encoded = &self.wire.get(index)?.focus;
        let focus = self.prepared().focus.get(index)?;
        focus
            .get_or_init(|| {
                performance::count(Count::RemoteCursorDecodes);
                Cursor::decode(&B64.decode(encoded).ok()?).ok()
            })
            .as_ref()
    }
}

impl Cursors {
    pub(in super::super) fn replace(&mut self, wire: Vec<CollabCursor>) {
        if wire.is_empty() {
            self.clear();
        } else if self
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.wire != wire)
        {
            self.snapshot = Some(Arc::new(Snapshot {
                wire,
                prepared: OnceLock::new(),
            }));
            self.positions.clear();
        }
    }

    pub(in super::super) fn clear(&mut self) {
        self.snapshot = None;
        self.positions = Vec::new();
    }

    pub(super) fn begin_paint(&mut self) -> bool {
        self.positions.clear();
        self.snapshot.is_some()
    }

    pub(super) fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.clone()
    }

    pub(super) fn position(&mut self, doc: &LoroDoc, index: usize) -> Option<usize> {
        let snapshot = self.snapshot.as_ref()?;
        if self.positions.is_empty() {
            self.positions.resize(snapshot.wire.len(), None);
        }
        *self.positions.get_mut(index)?.get_or_insert_with(|| {
            let focus = snapshot.focus(index)?;
            performance::count(Count::RemoteCursorPositions);
            doc.get_cursor_pos(focus)
                .ok()
                .map(|position| position.current.pos)
        })
    }
}
