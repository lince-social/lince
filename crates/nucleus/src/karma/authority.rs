use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    CanonicalHash, Capability, CapabilitySet, DecimalValue, DurationMs, ReferenceKind, Slug,
    TimestampMs, TypedUid, canonical_hash,
};
use crate::karma::KarmaBoundaryError;

pub const GRANT_REVISION_HASH_DOMAIN: &str = "karma.grant-revision.v1";
pub const GRANT_AUTHORITY_REQUEST_HASH_DOMAIN: &str = "karma.grant-authority-request.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DelegationGrantSchema {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantStatus {
    Draft,
    Active,
    Revoked,
}

impl GrantStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "draft" => Self::Draft,
            "active" => Self::Active,
            "revoked" => Self::Revoked,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "kebab-case")]
pub enum GrantProgramRevisionScope {
    AnyActive,
    Exact { revision_hash: CanonicalHash },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "kebab-case")]
pub enum GrantTemplateScope {
    Any,
    Only { templates: BTreeSet<Slug> },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "uid", rename_all = "kebab-case")]
pub enum GrantTarget {
    Record(TypedUid),
    Concept(TypedUid),
    Person(TypedUid),
    Organ(TypedUid),
    Place(TypedUid),
    Controller(TypedUid),
}

impl GrantTarget {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        let (uid, expected) = match self {
            Self::Record(uid) | Self::Controller(uid) => (uid, ReferenceKind::Record),
            Self::Concept(uid) => (uid, ReferenceKind::Concept),
            Self::Person(uid) => (uid, ReferenceKind::Person),
            Self::Organ(uid) => (uid, ReferenceKind::Organ),
            Self::Place(uid) => (uid, ReferenceKind::Place),
        };
        if uid.kind() != expected {
            return Err(KarmaBoundaryError::invalid_input(format!(
                "grant target {} requires a {} reference",
                target_kind(self),
                expected.short()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "kebab-case")]
pub enum GrantTargetScope {
    Any,
    Only { targets: BTreeSet<GrantTarget> },
}

/// A fixed count of intents over a fixed tumbling window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantWindowLimit {
    pub count: u64,
    pub duration_ms: DurationMs,
}

/// A total quantity a grant may ever authorize, in one declared unit. The scale
/// is part of the consent: a differently scaled amount is refused rather than
/// rounded into range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantQuantityLimit {
    pub unit_uid: TypedUid,
    pub limit: DecimalValue,
}

/// What a grant may spend. `None` everywhere means unlimited, which is why the
/// narrowing comparator treats absence as the widest possible value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_intents: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_window: Option<GrantWindowLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_limit: Option<GrantQuantityLimit>,
}

impl GrantBudget {
    /// An unlimited budget is omitted from the wire entirely, so a grant that
    /// declares no budget hashes exactly as it did before budgets existed and
    /// revisions stored by K5.1 still verify against their recorded hash.
    pub fn is_unlimited(&self) -> bool {
        self.max_intents.is_none() && self.per_window.is_none() && self.quantity_limit.is_none()
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.max_intents == Some(0) {
            return Err(KarmaBoundaryError::invalid_input(
                "grant intent cap must be at least 1; omit it for no cap",
            ));
        }
        if let Some(window) = &self.per_window
            && (window.count == 0 || window.duration_ms.get() <= 0)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant window limit needs a positive count and duration",
            ));
        }
        if let Some(quantity) = &self.quantity_limit {
            if quantity.unit_uid.kind() != ReferenceKind::Unit {
                return Err(KarmaBoundaryError::invalid_input(
                    "grant quantity limit must name a unit concept",
                ));
            }
            if quantity.limit.mantissa() <= 0 {
                return Err(KarmaBoundaryError::invalid_input(
                    "grant quantity limit must be positive",
                ));
            }
        }
        Ok(())
    }

    /// Which tumbling window an instant falls in, counted from `valid_from`.
    /// Deterministic, so a replay lands in the same window as the original.
    pub fn window_index(&self, valid_from: TimestampMs, at: TimestampMs) -> Option<u64> {
        let window = self.per_window.as_ref()?;
        let elapsed = at.as_millis().checked_sub(valid_from.as_millis())?;
        if elapsed < 0 {
            return None;
        }
        u64::try_from(elapsed / window.duration_ms.get()).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationGrantSpec {
    pub schema: DelegationGrantSchema,
    pub purpose: String,
    pub program_uid: TypedUid,
    pub program_revision: GrantProgramRevisionScope,
    pub candidate_templates: GrantTemplateScope,
    pub capabilities: CapabilitySet,
    pub targets: GrantTargetScope,
    #[serde(default, skip_serializing_if = "GrantBudget::is_unlimited")]
    pub budget: GrantBudget,
    pub valid_from: TimestampMs,
    pub expires_at: TimestampMs,
}

impl DelegationGrantSpec {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.purpose.is_empty()
            || self.purpose.len() > 500
            || self.purpose.trim() != self.purpose
            || self.purpose.chars().any(char::is_control)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant purpose must contain 1 to 500 trimmed non-control bytes",
            ));
        }
        if self.program_uid.kind() != ReferenceKind::Program {
            return Err(KarmaBoundaryError::invalid_input(
                "grant program_uid must be a Program reference",
            ));
        }
        if self.capabilities.is_empty() {
            return Err(KarmaBoundaryError::invalid_input(
                "grant capabilities must not be empty",
            ));
        }
        if self.capabilities.contains(Capability::KarmaGrantNarrow)
            || self.capabilities.contains(Capability::KarmaGrantWiden)
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant management capability cannot be delegated by a Karma grant",
            ));
        }
        if let GrantTemplateScope::Only { templates } = &self.candidate_templates
            && templates.is_empty()
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant template only-scope must not be empty",
            ));
        }
        if let GrantTargetScope::Only { targets } = &self.targets {
            if targets.is_empty() {
                return Err(KarmaBoundaryError::invalid_input(
                    "grant target only-scope must not be empty",
                ));
            }
            for target in targets {
                target.validate()?;
            }
        }
        if self.valid_from >= self.expires_at {
            return Err(KarmaBoundaryError::invalid_input(
                "grant validity must be a non-empty [valid_from, expires_at) interval",
            ));
        }
        self.budget.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationGrantRevision {
    pub schema: DelegationGrantSchema,
    pub principal_person_uid: TypedUid,
    pub spec: DelegationGrantSpec,
}

impl DelegationGrantRevision {
    pub fn new(
        principal_person_uid: TypedUid,
        spec: DelegationGrantSpec,
    ) -> Result<Self, KarmaBoundaryError> {
        let value = Self {
            schema: DelegationGrantSchema::V1,
            principal_person_uid,
            spec,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != DelegationGrantSchema::V1 || self.spec.schema != DelegationGrantSchema::V1
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant revision schema is unsupported",
            ));
        }
        if self.principal_person_uid.kind() != ReferenceKind::Person {
            return Err(KarmaBoundaryError::invalid_input(
                "grant principal must be a Person reference",
            ));
        }
        self.spec.validate()
    }

    pub fn revision_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(GRANT_REVISION_HASH_DOMAIN, self)
    }

    pub fn compare_replacement(
        &self,
        replacement: &Self,
    ) -> Result<GrantRevisionChange, KarmaBoundaryError> {
        self.validate()?;
        replacement.validate()?;
        if self.principal_person_uid != replacement.principal_person_uid
            || self.spec.program_uid != replacement.spec.program_uid
            || self.spec.purpose != replacement.spec.purpose
        {
            return Ok(GrantRevisionChange::Mixed);
        }
        let mut relation = ScopeRelation::Equal;
        relation = relation.combine(revision_relation(
            &self.spec.program_revision,
            &replacement.spec.program_revision,
        ));
        relation = relation.combine(template_relation(
            &self.spec.candidate_templates,
            &replacement.spec.candidate_templates,
        ));
        relation = relation.combine(set_relation(
            self.spec.capabilities.as_set(),
            replacement.spec.capabilities.as_set(),
        ));
        relation = relation.combine(target_relation(
            &self.spec.targets,
            &replacement.spec.targets,
        ));
        relation = relation.combine(budget_relation(&self.spec.budget, &replacement.spec.budget));
        relation = relation.combine(bound_relation(
            replacement.spec.valid_from,
            self.spec.valid_from,
        ));
        relation = relation.combine(bound_relation(
            self.spec.expires_at,
            replacement.spec.expires_at,
        ));
        Ok(match relation {
            ScopeRelation::Equal => GrantRevisionChange::Equivalent,
            ScopeRelation::Narrower => GrantRevisionChange::Narrowing,
            ScopeRelation::Wider => GrantRevisionChange::Widening,
            ScopeRelation::Mixed => GrantRevisionChange::Mixed,
        })
    }

    pub fn evaluate(&self, request: &GrantAuthorityRequest) -> GrantAuthorityDecision {
        let mut denials = Vec::new();
        if &self.principal_person_uid != request.principal_person_uid() {
            denials.push(GrantAuthorityDenial::PrincipalMismatch);
        }
        if &self.spec.program_uid != request.program_uid() {
            denials.push(GrantAuthorityDenial::ProgramMismatch);
        }
        if let GrantProgramRevisionScope::Exact { revision_hash } = &self.spec.program_revision
            && revision_hash != &request.program_revision_hash
        {
            denials.push(GrantAuthorityDenial::ProgramRevisionMismatch);
        }
        if let GrantTemplateScope::Only { templates } = &self.spec.candidate_templates
            && !templates.contains(&request.candidate_template)
        {
            denials.push(GrantAuthorityDenial::CandidateTemplateMismatch);
        }
        if !self.spec.capabilities.contains(request.capability) {
            denials.push(GrantAuthorityDenial::CapabilityMissing);
        }
        match (&self.spec.targets, &request.target) {
            (GrantTargetScope::Any, _) => {}
            (GrantTargetScope::Only { .. }, None) => {
                denials.push(GrantAuthorityDenial::TargetRequired)
            }
            (GrantTargetScope::Only { targets }, Some(target)) if !targets.contains(target) => {
                denials.push(GrantAuthorityDenial::TargetMismatch)
            }
            (GrantTargetScope::Only { .. }, Some(_)) => {}
        }
        if request.logical_at < self.spec.valid_from {
            denials.push(GrantAuthorityDenial::NotYetValid);
        }
        if request.logical_at >= self.spec.expires_at {
            denials.push(GrantAuthorityDenial::Expired);
        }
        GrantAuthorityDecision {
            allowed: denials.is_empty(),
            denials,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantRevisionChange {
    Equivalent,
    Narrowing,
    Widening,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantAuthorityRequest {
    pub principal_person_uid: TypedUid,
    pub program_uid: TypedUid,
    pub program_revision_hash: CanonicalHash,
    pub candidate_template: Slug,
    pub capability: Capability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<GrantTarget>,
    pub logical_at: TimestampMs,
}

impl GrantAuthorityRequest {
    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.principal_person_uid.kind() != ReferenceKind::Person
            || self.program_uid.kind() != ReferenceKind::Program
        {
            return Err(KarmaBoundaryError::invalid_input(
                "authority request requires typed Person and Program references",
            ));
        }
        if let Some(target) = &self.target {
            target.validate()?;
        }
        Ok(())
    }

    pub fn request_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(GRANT_AUTHORITY_REQUEST_HASH_DOMAIN, self)
    }

    fn principal_person_uid(&self) -> &TypedUid {
        &self.principal_person_uid
    }

    fn program_uid(&self) -> &TypedUid {
        &self.program_uid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantAuthorityDenial {
    GrantMissing,
    GrantInactive,
    GrantRevoked,
    PrincipalMismatch,
    ProgramMismatch,
    ProgramRevisionMismatch,
    CandidateTemplateMismatch,
    CapabilityMissing,
    TargetRequired,
    TargetMismatch,
    NotYetValid,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantAuthorityDecision {
    pub allowed: bool,
    pub denials: Vec<GrantAuthorityDenial>,
}

impl GrantAuthorityDecision {
    pub fn denied(reason: GrantAuthorityDenial) -> Self {
        Self {
            allowed: false,
            denials: vec![reason],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationSignature {
    pub signer_person_uid: TypedUid,
    pub key_id: String,
    pub signature: String,
}

impl DelegationSignature {
    pub fn validate_for(&self, principal: &TypedUid) -> Result<(), KarmaBoundaryError> {
        if self.signer_person_uid.kind() != ReferenceKind::Person
            || &self.signer_person_uid != principal
        {
            return Err(KarmaBoundaryError::invalid_input(
                "grant signature signer must equal the principal Person",
            ));
        }
        for (name, value, maximum) in [
            ("key id", self.key_id.as_str(), 200usize),
            ("signature", self.signature.as_str(), 2048usize),
        ] {
            if value.is_empty()
                || value.len() > maximum
                || value.trim() != value
                || value.chars().any(char::is_control)
            {
                return Err(KarmaBoundaryError::invalid_input(format!(
                    "grant {name} must contain 1 to {maximum} trimmed non-control bytes"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantMutationAction {
    Create,
    Narrow,
    Activate,
    Revoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantMutationEvidenceSchema {
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantMutationEvidence {
    pub schema: GrantMutationEvidenceSchema,
    pub request_id: String,
    pub action: GrantMutationAction,
    pub grant_uid: String,
    pub handle_revision: u64,
    pub status: GrantStatus,
    pub head_revision_hash: CanonicalHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_revision_hash: Option<CanonicalHash>,
    pub principal_person_uid: TypedUid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeRelation {
    Equal,
    Narrower,
    Wider,
    Mixed,
}

impl ScopeRelation {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Mixed, _) | (_, Self::Mixed) => Self::Mixed,
            (Self::Equal, value) | (value, Self::Equal) => value,
            (Self::Narrower, Self::Narrower) => Self::Narrower,
            (Self::Wider, Self::Wider) => Self::Wider,
            (Self::Narrower, Self::Wider) | (Self::Wider, Self::Narrower) => Self::Mixed,
        }
    }
}

fn revision_relation(
    old: &GrantProgramRevisionScope,
    new: &GrantProgramRevisionScope,
) -> ScopeRelation {
    match (old, new) {
        (GrantProgramRevisionScope::AnyActive, GrantProgramRevisionScope::AnyActive) => {
            ScopeRelation::Equal
        }
        (GrantProgramRevisionScope::AnyActive, GrantProgramRevisionScope::Exact { .. }) => {
            ScopeRelation::Narrower
        }
        (GrantProgramRevisionScope::Exact { .. }, GrantProgramRevisionScope::AnyActive) => {
            ScopeRelation::Wider
        }
        (
            GrantProgramRevisionScope::Exact { revision_hash: old },
            GrantProgramRevisionScope::Exact { revision_hash: new },
        ) if old == new => ScopeRelation::Equal,
        (GrantProgramRevisionScope::Exact { .. }, GrantProgramRevisionScope::Exact { .. }) => {
            ScopeRelation::Mixed
        }
    }
}

fn template_relation(old: &GrantTemplateScope, new: &GrantTemplateScope) -> ScopeRelation {
    match (old, new) {
        (GrantTemplateScope::Any, GrantTemplateScope::Any) => ScopeRelation::Equal,
        (GrantTemplateScope::Any, GrantTemplateScope::Only { .. }) => ScopeRelation::Narrower,
        (GrantTemplateScope::Only { .. }, GrantTemplateScope::Any) => ScopeRelation::Wider,
        (
            GrantTemplateScope::Only { templates: old },
            GrantTemplateScope::Only { templates: new },
        ) => set_relation(old, new),
    }
}

fn target_relation(old: &GrantTargetScope, new: &GrantTargetScope) -> ScopeRelation {
    match (old, new) {
        (GrantTargetScope::Any, GrantTargetScope::Any) => ScopeRelation::Equal,
        (GrantTargetScope::Any, GrantTargetScope::Only { .. }) => ScopeRelation::Narrower,
        (GrantTargetScope::Only { .. }, GrantTargetScope::Any) => ScopeRelation::Wider,
        (GrantTargetScope::Only { targets: old }, GrantTargetScope::Only { targets: new }) => {
            set_relation(old, new)
        }
    }
}

fn set_relation<T: Ord>(old: &BTreeSet<T>, new: &BTreeSet<T>) -> ScopeRelation {
    if old == new {
        ScopeRelation::Equal
    } else if new.is_subset(old) {
        ScopeRelation::Narrower
    } else if old.is_subset(new) {
        ScopeRelation::Wider
    } else {
        ScopeRelation::Mixed
    }
}

/// Absence of a limit is unlimited, so adding one narrows and dropping one
/// widens. Where both sides carry a limit, the replacement may only go lower.
fn budget_relation(old: &GrantBudget, new: &GrantBudget) -> ScopeRelation {
    let intents = limit_relation(old.max_intents.as_ref(), new.max_intents.as_ref(), |old, new| {
        // A smaller cap is the narrower one.
        old.cmp(new)
    });
    let window = limit_relation(old.per_window.as_ref(), new.per_window.as_ref(), |old, new| {
        // Conservative on purpose: a replacement counts as narrower only when it
        // allows no more events over no shorter a window. A lower rate carrying a
        // bigger burst is Mixed, not narrower, and is refused.
        match (new.count.cmp(&old.count), new.duration_ms.get().cmp(&old.duration_ms.get())) {
            (std::cmp::Ordering::Equal, std::cmp::Ordering::Equal) => std::cmp::Ordering::Equal,
            (std::cmp::Ordering::Greater, _) | (_, std::cmp::Ordering::Less) => {
                std::cmp::Ordering::Less
            }
            _ => std::cmp::Ordering::Greater,
        }
    });
    let quantity = limit_relation(
        old.quantity_limit.as_ref(),
        new.quantity_limit.as_ref(),
        |old, new| {
            // A different unit or scale is not comparable, so it cannot be a narrowing.
            if old.unit_uid != new.unit_uid || old.limit.scale() != new.limit.scale() {
                return std::cmp::Ordering::Less;
            }
            new.limit.mantissa().cmp(&old.limit.mantissa()).reverse()
        },
    );
    intents.combine(window).combine(quantity)
}

/// `compare` reports Greater when the replacement is strictly narrower, Equal
/// when identical, and Less for anything wider or incomparable.
fn limit_relation<T: PartialEq>(
    old: Option<T>,
    new: Option<T>,
    compare: impl FnOnce(T, T) -> std::cmp::Ordering,
) -> ScopeRelation {
    match (old, new) {
        (None, None) => ScopeRelation::Equal,
        (None, Some(_)) => ScopeRelation::Narrower,
        (Some(_), None) => ScopeRelation::Wider,
        (Some(old), Some(new)) => match compare(old, new) {
            std::cmp::Ordering::Equal => ScopeRelation::Equal,
            std::cmp::Ordering::Greater => ScopeRelation::Narrower,
            std::cmp::Ordering::Less => ScopeRelation::Wider,
        },
    }
}

fn bound_relation(narrow_when_greater: TimestampMs, old: TimestampMs) -> ScopeRelation {
    match narrow_when_greater.cmp(&old) {
        std::cmp::Ordering::Equal => ScopeRelation::Equal,
        std::cmp::Ordering::Greater => ScopeRelation::Narrower,
        std::cmp::Ordering::Less => ScopeRelation::Wider,
    }
}

fn target_kind(target: &GrantTarget) -> &'static str {
    match target {
        GrantTarget::Record(_) => "record",
        GrantTarget::Concept(_) => "concept",
        GrantTarget::Person(_) => "person",
        GrantTarget::Organ(_) => "organ",
        GrantTarget::Place(_) => "place",
        GrantTarget::Controller(_) => "controller",
    }
}
