//! Public evidence inputs and analysis results.

use serde::Serialize;

use super::{
    super::{Identity, paths::Path},
    EvidenceLevel,
};
use crate::{
    DecimalValue,
    karma::CanonicalHash,
    transfer::{
        receipt::{Content, Measurement, RecordedFloat},
        receipt_history::{Copy, Provenance},
    },
    vocabulary::reference::MeaningReference,
};

pub struct SourceEvidence {
    pub origin: String,
    pub occurrence: CanonicalHash,
    pub giver: String,
    pub receiver: String,
    pub copies: Vec<Copy>,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRow {
    pub source: CanonicalHash,
    pub outcome: CanonicalHash,
    pub need: Option<CanonicalHash>,
    pub agreed_quantity: Option<RecordedFloat>,
    pub canonical_unit: Option<MeaningReference>,
    pub issuer: Identity,
    pub counterparty: Identity,
    pub measurement: Measurement,
    pub level: EvidenceLevel,
    pub recorded_at: i64,
    pub quantity: DecimalValue,
    pub distance: Option<usize>,
    pub path: Option<Path>,
    pub records: Vec<String>,
    pub originals: Vec<Provenance>,
    pub disputed: bool,
    pub withdrawn: bool,
    pub selected: bool,
    pub source_qualified: bool,
    pub weighted_index: Option<DecimalValue>,
    pub index_rounded: bool,
    pub capped: bool,
    pub arithmetic_error: bool,
}

#[derive(Debug, Serialize)]
pub struct ProtocolRow {
    pub source: CanonicalHash,
    pub outcome: CanonicalHash,
    pub issuer: Identity,
    pub content: Content,
    pub current: bool,
    pub records: Vec<String>,
    pub originals: Vec<Provenance>,
    pub canonical_unit: Option<MeaningReference>,
}

#[derive(Debug, Serialize)]
pub struct Group {
    pub measurement: Measurement,
    pub level: EvidenceLevel,
    pub raw_quantity: Option<DecimalValue>,
    pub weighted_index: Option<DecimalValue>,
    pub occurrences: usize,
    pub issuers: Vec<Identity>,
    pub counterparties: Vec<Identity>,
    pub index_rounded: bool,
    pub capped: bool,
    pub arithmetic_error: bool,
}

#[derive(Debug, Serialize)]
pub struct Withheld {
    pub source: CanonicalHash,
    pub reason: &'static str,
    pub records: Vec<String>,
    pub originals: Vec<Provenance>,
}

#[derive(Debug, Serialize)]
pub struct Analysis {
    pub policy: CanonicalHash,
    pub rows: Vec<EvidenceRow>,
    pub protocol: Vec<ProtocolRow>,
    pub groups: Vec<Group>,
    pub withheld: Vec<Withheld>,
    pub partial: bool,
    pub observed_only: bool,
    pub input_events: usize,
}
