use crate::protein_castle::ProteinDraft;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    #[default]
    Local,
    Organ(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverflowMode {
    Clip,
    ScrollDown,
    ScrollRight,
    GrowDown,
    GrowRight,
}

impl OverflowMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Clip => "Clip",
            Self::ScrollDown => "Scroll down",
            Self::ScrollRight => "Scroll right",
            Self::GrowDown => "Grow down",
            Self::GrowRight => "Grow right",
        }
    }
    pub const ALL: [Self; 5] = [
        Self::Clip,
        Self::ScrollDown,
        Self::ScrollRight,
        Self::GrowDown,
        Self::GrowRight,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub property: String,
    pub square: bool,
    pub editable: bool,
    pub width: f32,
    pub height: f32,
    pub overflow: OverflowMode,
}

impl Binding {
    pub fn new(property: &str) -> Self {
        Self {
            property: property.into(),
            square: property == "head",
            editable: false,
            width: 280.0,
            height: if property == "body" { 100.0 } else { 40.0 },
            overflow: OverflowMode::ScrollDown,
        }
    }
    pub fn valid(&self) -> bool {
        protein::record_schema::fields()
            .iter()
            .any(|field| field.key == self.property && (!self.editable || field.editable))
            && [self.width, self.height]
                .iter()
                .all(|v| v.is_finite() && (24.0..=4000.0).contains(v))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub task_cards: bool,
    pub closest_end_date: bool,
    #[serde(default)]
    pub calendar_dates: bool,
    pub enabled: bool,
    pub source: Source,
    pub draft: ProteinDraft,
    pub bindings: Vec<Binding>,
    pub width: f32,
    pub gap: f32,
    pub columns: usize,
    pub delete_button: bool,
    pub grouping: super::grouping::Grouping,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            task_cards: false,
            closest_end_date: false,
            calendar_dates: false,
            enabled: false,
            source: Source::Local,
            draft: ProteinDraft::default(),
            bindings: vec![Binding::new("head"), Binding::new("body")],
            width: 320.0,
            gap: 16.0,
            columns: 1,
            delete_button: false,
            grouping: Default::default(),
        }
    }
}

impl Config {
    pub fn tasks() -> Self {
        Self {
            task_cards: true,
            bindings: [
                "head",
                "assignees",
                "start_date",
                "due_date",
                "assertions",
                "quantity_exact",
            ]
            .into_iter()
            .map(|property| Binding {
                editable: true,
                overflow: OverflowMode::GrowDown,
                ..Binding::new(property)
            })
            .collect(),
            ..Default::default()
        }
    }

    pub fn valid(&self) -> bool {
        self.draft.valid_storage()
            && self.grouping.valid()
            && self.bindings.len() <= 32
            && self.bindings.iter().all(Binding::valid)
            && self.width.is_finite()
            && (80.0..=8000.0).contains(&self.width)
            && self.gap.is_finite()
            && (0.0..=1000.0).contains(&self.gap)
            && (1..=32).contains(&self.columns)
            && match &self.source {
                Source::Local => true,
                Source::Organ(uid) => uid.len() <= 128,
            }
    }
    pub fn query(&self) -> Result<protein::Protein, String> {
        if !self.valid() {
            return Err("Invalid row template".into());
        }
        let mut query = self.draft.compile()?;
        if query.source != protein::Source::Record || query.aggregate.is_some() {
            return Err("Record templates need a Record query without aggregation".into());
        }
        let mut fields: Vec<String> = self
            .bindings
            .iter()
            .map(|binding| binding.property.clone())
            .collect();
        fields.extend(["uid", "kind", "organ"].map(str::to_string));
        if self.calendar_dates {
            fields.extend(["head", "start_date", "due_date"].map(str::to_string));
        }
        if self.closest_end_date {
            fields.push("due_date".into());
        }
        fields.extend(self.grouping.fields().map(str::to_string));
        if self.bindings.iter().any(|b| {
            b.editable
                && matches!(
                    b.property.as_str(),
                    "start_date" | "due_date" | "estimate_min" | "work_logs"
                )
        }) {
            fields.push("extension".into());
            query.include.extension = Some(protein::ExtensionInclude {
                namespace: "work".into(),
            });
        }
        fields.sort();
        fields.dedup();
        query.fields = Some(fields);
        Ok(query)
    }
}
