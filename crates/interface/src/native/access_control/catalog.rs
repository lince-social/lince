use super::Tab;
use serde_json::Value;
use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
};

/// Display order and identity lookup belong to the exact immutable response.
/// No permission decision or action validity is retained in these indexes.
#[derive(Clone, Default)]
pub(super) struct Rows(Arc<Snapshot>);

#[derive(Default)]
struct Snapshot {
    values: Arc<Vec<Value>>,
    ordered: [OnceLock<Vec<usize>>; 2],
    identities: [OnceLock<Vec<usize>>; 2],
    role_names: OnceLock<Vec<usize>>,
    permissions: OnceLock<Vec<(usize, usize)>>,
}

impl From<Arc<Vec<Value>>> for Rows {
    fn from(values: Arc<Vec<Value>>) -> Self {
        Self(Arc::new(Snapshot {
            values,
            ..Default::default()
        }))
    }
}

impl From<Vec<Value>> for Rows {
    fn from(values: Vec<Value>) -> Self {
        Arc::new(values).into()
    }
}

impl Deref for Rows {
    type Target = [Value];
    fn deref(&self) -> &Self::Target {
        &self.0.values
    }
}

fn kind(tab: Tab) -> (usize, &'static str) {
    match tab {
        Tab::Users => (0, "user"),
        Tab::Roles => (1, "role"),
    }
}

impl Rows {
    /// Preserve semantic change admission, but replace representation-only
    /// changes too: later editor preparation must see the newest exact data.
    pub(super) fn replace(&mut self, values: &Arc<Vec<Value>>) -> bool {
        let _preparation = nucleus::performance::Span::new("access-catalog-snapshot");
        if nucleus::json::same_values_encoding(&self.0.values, values) {
            return false;
        }
        let changed = self.0.values != *values;
        *self = values.clone().into();
        nucleus::performance::add("access-catalog-snapshots-prepared", 1);
        changed
    }

    fn index(&self, tab: Tab, identity: bool) -> &[usize] {
        let (slot, kind) = kind(tab);
        let key = if identity { "id" } else { "name" };
        let cache = if identity {
            &self.0.identities[slot]
        } else {
            &self.0.ordered[slot]
        };
        cache.get_or_init(|| {
            let _preparation = nucleus::performance::Span::new("access-catalog-index");
            let mut positions: Vec<_> = self
                .iter()
                .enumerate()
                .filter_map(|(i, row)| (row["kind"] == kind).then_some(i))
                .collect();
            // Stable sorting preserves source order for equal/missing names
            // and first-source-row lookup for duplicate or absent identities.
            positions
                .sort_by(|&left, &right| self[left][key].as_str().cmp(&self[right][key].as_str()));
            nucleus::performance::add("access-catalog-indexes-prepared", 1);
            nucleus::performance::add("access-catalog-index-entries", positions.len() as u64);
            positions
        })
    }

    pub(super) fn ordered(&self, tab: Tab) -> impl ExactSizeIterator<Item = &Value> {
        self.index(tab, false).iter().map(|&i| &self[i])
    }

    pub(super) fn find(&self, tab: Tab, id: Option<&str>) -> Option<&Value> {
        let positions = self.index(tab, true);
        let first = positions.partition_point(|&i| self[i]["id"].as_str() < id);
        let row = &self[*positions.get(first)?];
        (row["id"].as_str() == id).then_some(row)
    }

    pub(super) fn has_role(&self, name: &str) -> bool {
        let positions = self.index(Tab::Roles, false);
        let first = positions.partition_point(|&i| self[i]["name"].as_str() < Some(name));
        positions
            .get(first)
            .is_some_and(|&i| self[i]["name"].as_str() == Some(name))
    }

    pub(super) fn role_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0
            .role_names
            .get_or_init(|| {
                let _preparation = nucleus::performance::Span::new("access-catalog-index");
                let positions: Vec<_> = self
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| {
                        (row["kind"] == "role" && row["name"].is_string()).then_some(i)
                    })
                    .collect();
                nucleus::performance::add("access-catalog-indexes-prepared", 1);
                nucleus::performance::add("access-catalog-index-entries", positions.len() as u64);
                positions
            })
            .iter()
            .map(|&i| self[i]["name"].as_str().unwrap())
    }

    pub(super) fn permissions(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0
            .permissions
            .get_or_init(|| {
                let _preparation = nucleus::performance::Span::new("access-catalog-index");
                let mut positions = Vec::new();
                for (i, row) in self
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row["kind"] == "permission_catalog")
                {
                    if let Some(keys) = row["keys"].as_array() {
                        positions.extend(
                            keys.iter()
                                .enumerate()
                                .filter_map(|(key, value)| value.is_string().then_some((i, key))),
                        );
                    }
                }
                nucleus::performance::add("access-catalog-indexes-prepared", 1);
                nucleus::performance::add("access-catalog-index-entries", positions.len() as u64);
                positions
            })
            .iter()
            .map(|&(i, key)| self[i]["keys"][key].as_str().unwrap())
    }

    #[cfg(test)]
    pub(super) fn edit(&mut self) -> &mut Vec<Value> {
        let snapshot = Arc::make_mut(&mut self.0);
        snapshot.ordered = Default::default();
        snapshot.identities = Default::default();
        snapshot.role_names = Default::default();
        snapshot.permissions = Default::default();
        Arc::make_mut(&mut snapshot.values)
    }
}

#[cfg(test)]
impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            values: self.values.clone(),
            ..Default::default()
        }
    }
}
