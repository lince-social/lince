use serde_json::Value;
use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Default)]
pub struct Catalog(Arc<Snapshot>);

#[derive(Default)]
struct Snapshot {
    value: Value,
    indexes: [OnceLock<Vec<usize>>; 4],
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            indexes: Default::default(),
        }
    }
}

#[derive(Clone, Copy)]
#[repr(usize)]
pub(super) enum Rows {
    Foods,
    Offers,
    Templates,
    NutrientReferences,
}

impl Rows {
    fn name(self) -> &'static str {
        match self {
            Self::Foods => "foods",
            Self::Offers => "offers",
            Self::Templates => "templates",
            Self::NutrientReferences => "references",
        }
    }

    fn key(self, row: &Value) -> Option<&str> {
        match self {
            Self::NutrientReferences => row["reference"]["nutrient"].as_str(),
            _ => row["uid"].as_str(),
        }
    }
}

impl From<Value> for Catalog {
    fn from(value: Value) -> Self {
        Self(Arc::new(Snapshot {
            value,
            ..Default::default()
        }))
    }
}

impl Deref for Catalog {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0.value
    }
}

impl Catalog {
    pub(super) fn edit(&mut self) -> &mut Value {
        let snapshot = Arc::make_mut(&mut self.0);
        snapshot.indexes = Default::default();
        &mut snapshot.value
    }

    pub(super) fn find(&self, rows: Rows, key: &str) -> Option<&Value> {
        let values = self.0.value[rows.name()].as_array()?;
        if values.len() <= 1 {
            return values.iter().find(|row| rows.key(row) == Some(key));
        }
        let index = self.0.indexes[rows as usize].get_or_init(|| {
            let _prepare = nucleus::performance::Span::new("demeter-catalog-index");
            let mut index: Vec<_> = (0..values.len())
                .filter(|&i| rows.key(&values[i]).is_some())
                .collect();
            index.sort_by(|&a, &b| rows.key(&values[a]).cmp(&rows.key(&values[b])));
            nucleus::performance::add("demeter-catalog-indexes-prepared", 1);
            nucleus::performance::add("demeter-catalog-index-entries", index.len() as u64);
            index
        });
        let position = index.partition_point(|&i| rows.key(&values[i]).unwrap() < key);
        let row = &values[*index.get(position)?];
        (rows.key(row) == Some(key)).then_some(row)
    }
}
