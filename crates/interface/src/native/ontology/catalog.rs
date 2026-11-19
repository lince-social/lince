use serde_json::Value;
use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Default)]
pub(super) struct Catalog(Arc<Snapshot>);

#[derive(Default)]
struct Snapshot {
    rows: Arc<Vec<Value>>,
    ordered: OnceLock<Vec<usize>>,
    choices: OnceLock<Vec<usize>>,
    organs: OnceLock<Vec<usize>>,
    identities: OnceLock<Vec<usize>>,
}

impl From<Arc<Vec<Value>>> for Catalog {
    fn from(rows: Arc<Vec<Value>>) -> Self {
        Self(Arc::new(Snapshot {
            rows,
            ..Default::default()
        }))
    }
}

impl From<Vec<Value>> for Catalog {
    fn from(rows: Vec<Value>) -> Self {
        Self::from(Arc::new(rows))
    }
}

impl Deref for Catalog {
    type Target = [Value];

    fn deref(&self) -> &Self::Target {
        &self.0.rows
    }
}

pub(super) fn title(row: &Value) -> &str {
    row["name"]
        .as_str()
        .or_else(|| row["head"].as_str().filter(|value| !value.is_empty()))
        .or_else(|| row["slug"].as_str())
        .or_else(|| row["uid"].as_str())
        .unwrap_or("Unnamed")
}

impl Catalog {
    pub(super) fn replace(&mut self, rows: &Arc<Vec<Value>>) -> bool {
        if nucleus::json::same_values_encoding(&self.0.rows, rows) {
            return false;
        }
        *self = Self::from(rows.clone());
        true
    }

    fn index<'a>(
        &'a self,
        cache: &'a OnceLock<Vec<usize>>,
        build: impl FnOnce() -> Vec<usize>,
    ) -> &'a [usize] {
        cache.get_or_init(|| {
            let _index = nucleus::performance::Span::new("ontology-catalog-index");
            let index = build();
            nucleus::performance::add("ontology-catalog-indexes-prepared", 1);
            nucleus::performance::add("ontology-catalog-index-entries", index.len() as u64);
            index
        })
    }

    pub(super) fn ordered(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.index(&self.0.ordered, || {
            let mut indices: Vec<_> = (0..self.len()).collect();
            indices.sort_by(|&left, &right| title(&self[left]).cmp(title(&self[right])));
            indices
        })
        .iter()
        .map(|&index| &self[index])
    }

    fn choice_indices(&self) -> &[usize] {
        self.index(&self.0.choices, || {
            let mut indices: Vec<_> = self
                .iter()
                .enumerate()
                .filter_map(|(index, row)| row["uid"].is_string().then_some(index))
                .collect();
            indices.sort_by(|&left, &right| {
                title(&self[left])
                    .cmp(title(&self[right]))
                    .then_with(|| self[left]["uid"].as_str().cmp(&self[right]["uid"].as_str()))
            });
            indices
        })
    }

    pub(super) fn choices(&self, organs: bool) -> impl ExactSizeIterator<Item = (&str, &str)> {
        let indices = if organs {
            self.index(&self.0.organs, || {
                self.choice_indices()
                    .iter()
                    .copied()
                    .filter(|&index| self[index]["kind"] == "organ")
                    .collect()
            })
        } else {
            self.choice_indices()
        };
        indices.iter().map(|&index| {
            let row = &self[index];
            (row["uid"].as_str().unwrap(), title(row))
        })
    }

    fn identities(&self) -> &[usize] {
        self.index(&self.0.identities, || {
            let mut indices: Vec<_> = self
                .iter()
                .enumerate()
                .filter_map(|(index, row)| row["uid"].is_string().then_some(index))
                .collect();
            indices.sort_by_key(|&index| self[index]["uid"].as_str().unwrap());
            indices
        })
    }

    pub(super) fn find(&self, uid: &str) -> Option<&Value> {
        let identities = self.identities();
        let position =
            identities.partition_point(|&index| self[index]["uid"].as_str().unwrap() < uid);
        identities
            .get(position)
            .map(|&index| &self[index])
            .filter(|row| row["uid"].as_str() == Some(uid))
    }

    pub(super) fn choice_title(&self, uid: &str, organs: bool) -> Option<&str> {
        let identities = self.identities();
        let position =
            identities.partition_point(|&index| self[index]["uid"].as_str().unwrap() < uid);
        identities[position..]
            .iter()
            .map(|&index| &self[index])
            .take_while(|row| row["uid"].as_str() == Some(uid))
            .filter(|row| !organs || row["kind"] == "organ")
            .map(title)
            .min()
    }
}
