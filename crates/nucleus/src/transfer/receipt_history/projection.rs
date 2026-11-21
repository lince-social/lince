//! Project signed events and their evidence onto stable outcome identifiers.

use std::{borrow::Cow, collections::BTreeMap};

use serde::{Deserialize, Serialize};

use super::super::receipt::{
    Content, Event, PartyAttribution, PublicPerson, Publication, RecordedFloat, Scope,
};
use crate::{
    karma::{CanonicalHash, canonical_hash},
    vocabulary::reference::MeaningReference,
};

pub(super) fn hash<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalHash, String> {
    canonical_hash(domain, value).map_err(|error| error.to_string())
}

/// Hash the origin and occurrence that identify a receipt source.
pub fn source_key(origin: &str, occurrence: &CanonicalHash) -> Result<CanonicalHash, String> {
    hash("lince.transfer.history.source.v1", &(origin, occurrence))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub event: CanonicalHash,
    pub source: CanonicalHash,
    pub binding: CanonicalHash,
    pub slot: CanonicalHash,
    pub outcome: CanonicalHash,
    pub semantic: CanonicalHash,
    pub parent: Option<CanonicalHash>,
    pub need: Option<CanonicalHash>,
    pub issuer: String,
}

fn outcome(scope: &Scope, event: &Event) -> Result<CanonicalHash, String> {
    hash(
        "lince.transfer.history.outcome.v1",
        &(
            &scope.source.origin,
            &scope.source.occurrence,
            scope.organ(event.content.role()),
            event.content.role(),
            event.content.stage(),
            &event.source_commitment,
        ),
    )
}

/// Derive the ordered history projections of a complete publication.
///
/// # Errors
///
/// The publication is invalid, or its event evidence cannot be encoded or resolved.
pub fn project(publication: &Publication) -> Result<Vec<Projection>, String> {
    publication.validate(publication.evidence_time())?;
    if publication.events.is_empty() {
        return Ok(Vec::new());
    }
    let events = publication
        .events
        .iter()
        .map(|signed| Ok((signed.event.id()?, &signed.event)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let projector = Projector::new(&publication.scope)?;
    events
        .iter()
        .map(|(id, event)| projector.event(id, event, |id| events.get(id).copied()))
        .collect()
}

pub(in crate::transfer) struct Projector<'a> {
    scope: &'a Scope,
    source: CanonicalHash,
    binding: CanonicalHash,
}

impl<'a> Projector<'a> {
    pub(in crate::transfer) fn new(scope: &'a Scope) -> Result<Self, String> {
        Ok(Self {
            scope,
            source: source_key(&scope.source.origin, &scope.source.occurrence)?,
            binding: hash(
                "lince.transfer.history.source-binding.v1",
                &(&scope.source, &scope.giver, &scope.receiver),
            )?,
        })
    }

    pub(in crate::transfer) fn event<'e>(
        &self,
        id: &CanonicalHash,
        event: &Event,
        get: impl Fn(&CanonicalHash) -> Option<&'e Event>,
    ) -> Result<Projection, String> {
        let scope = self.scope;
        let resolve =
            |id: &CanonicalHash| outcome(scope, get(id).ok_or("History dependency is missing")?);
        let mut content = Cow::Borrowed(&event.content);
        let mut need = None;
        if matches!(&event.content, Content::Statement { need: Some(_), .. })
            && let Content::Statement { need: link, .. } = content.to_mut()
        {
            need = link.take();
        }
        Ok(Projection {
            event: id.clone(),
            source: self.source.clone(),
            binding: self.binding.clone(),
            slot: event.slot(scope)?,
            outcome: outcome(scope, event)?,
            semantic: hash(
                "lince.transfer.history.semantic.v1",
                &(event.recorded_at, content),
            )?,
            parent: event.supersedes.as_ref().map(resolve).transpose()?,
            need: need.as_ref().map(resolve).transpose()?,
            issuer: scope.organ(event.content.role()).into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "provenance", rename_all = "snake_case")]
pub enum Provenance {
    Record { record: String, version: u64 },
    Artifact { hash: CanonicalHash },
}

impl Provenance {
    pub fn record(&self) -> Option<&str> {
        match self {
            Self::Record { record, .. } => Some(record),
            Self::Artifact { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Copy {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub persons: Vec<PublicPerson>,
    #[serde(flatten)]
    pub provenance: Provenance,
    pub scope: CanonicalHash,
    pub attribution: PartyAttribution,
    pub projection: Projection,
    pub content: Content,
    pub recorded_at: i64,
    pub canonical_unit: Option<MeaningReference>,
    pub agreed_quantity: Option<RecordedFloat>,
}

/// Retain an owned copy of a projected event and the evidence that disclosed it.
///
/// # Errors
///
/// The event or scope fails identity validation, or the proof does not match.
pub fn project_copy(
    scope: &Scope,
    event: &Event,
    projection: &Projection,
    provenance: Provenance,
) -> Result<Copy, String> {
    if projection.event != event.id()? || event.scope != scope.id()? {
        return Err("The projected event differs from its original proof".into());
    }
    Ok(Copy::from_resolved(
        scope,
        event,
        projection.clone(),
        provenance,
    ))
}

impl Copy {
    /// Retain a projection whose event and scope have already been checked.
    pub(crate) fn from_resolved(
        scope: &Scope,
        event: &Event,
        projection: Projection,
        provenance: Provenance,
    ) -> Self {
        Self {
            persons: scope.public_persons.clone(),
            provenance,
            scope: event.scope.clone(),
            attribution: scope.party_attribution,
            projection,
            content: event.content.clone(),
            recorded_at: event.recorded_at,
            canonical_unit: scope.canonical_unit.clone(),
            agreed_quantity: scope.agreed_quantity.clone(),
        }
    }
}
