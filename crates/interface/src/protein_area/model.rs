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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpawnPlacement {
    #[default]
    Source,
    MatchingAreas,
    Physics,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub hide_filled: bool,
    #[serde(default)]
    pub relations: bool,
    #[serde(default)]
    pub motion: Option<crate::protein_motion::Settings>,
    #[serde(default)]
    pub fiote: bool,
    #[serde(default)]
    pub command: Option<crate::command_castle::Settings>,
    pub record_cards: bool,
    #[serde(default)]
    pub viewport_height: Option<f32>,
    #[serde(default)]
    pub max_height: Option<f32>,
    #[serde(default)]
    pub group_with_source: bool,
    pub placement: SpawnPlacement,
    pub spawn_targets: Vec<String>,
    pub settling_ticks: u16,
    #[serde(default)]
    pub show_labels: bool,
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
            hide_filled: false,
            relations: false,
            motion: None,
            fiote: false,
            command: None,
            record_cards: false,
            viewport_height: None,
            max_height: None,
            group_with_source: false,
            placement: SpawnPlacement::Source,
            spawn_targets: Vec::new(),
            settling_ticks: 120,
            show_labels: false,
            closest_end_date: false,
            calendar_dates: false,
            enabled: true,
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
    pub(super) fn same_template(&self, other: &Self) -> bool {
        self.fiote == other.fiote
            && self.command.is_some() == other.command.is_some()
            && self.record_cards == other.record_cards
            && self.viewport_height == other.viewport_height
            && self.max_height == other.max_height
            && self.show_labels == other.show_labels
            && self.bindings == other.bindings
            && self.width == other.width
            && self.delete_button == other.delete_button
    }

    pub fn records() -> Self {
        Self {
            record_cards: true,
            show_labels: true,
            bindings: protein::record_schema::fields()
                .into_iter()
                .filter(|field| field.editable)
                .map(|field| Binding {
                    editable: true,
                    overflow: OverflowMode::GrowDown,
                    height: if field.key == "body" { 72.0 } else { 32.0 },
                    ..Binding::new(field.key)
                })
                .collect(),
            ..Default::default()
        }
    }

    pub fn valid(&self) -> bool {
        self.command.as_ref().is_none_or(|settings| settings.cwd.len() <= 4096 && !settings.cwd.contains('\0'))
            && self.motion
            .as_ref()
            .is_none_or(crate::protein_motion::Settings::valid)
            && (!self.relations || self.record_cards)
            && (self.motion.is_none() || (!self.group_with_source && !self.grouping.active()))
            && self
                .viewport_height
                .is_none_or(|height| height.is_finite() && (80.0..=4000.0).contains(&height))
            && self
                .max_height
                .is_none_or(|height| height.is_finite() && (80.0..=4000.0).contains(&height))
            && (!self.group_with_source || self.placement == SpawnPlacement::Source)
            && self.settling_ticks <= 600
            && self.spawn_targets.len() <= 256
            && self
                .spawn_targets
                .iter()
                .all(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            && self.draft.valid_storage()
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
        if self.relations {
            fields.push("links".into());
            query.include.links.get_or_insert(protein::LinksInclude {
                kinds: vec!["*".into()],
                direction: protein::LinkDirection::Both,
                depth: 1,
            });
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.property == "work_timer")
        {
            fields.retain(|field| field != "work_timer");
            fields.extend(["work_logs", "spent_seconds", "running_since"].map(str::to_string));
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.property == "threads")
        {
            query.include.threads = Some(protein::ThreadsInclude {
                messages_limit: 50,
                ..Default::default()
            });
        }
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
