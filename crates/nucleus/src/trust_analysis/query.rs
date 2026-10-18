use super::{Filter, Identity, IdentityContext, MAX_BYTES, MAX_DISCLOSURES, paths};
use crate::{
    DecimalValue,
    karma::{CanonicalHash, Rounding, canonical_hash},
    transfer::{
        outcome::Role,
        receipt::{ApprovalRole, Content, Measurement, PartyAttribution},
        receipt_history::{self, Copy, Provenance},
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const NAMESPACE: &str = "lince.trust-query";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    NeedLinkedReceiver,
    Receiver,
    Giver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CounterpartyBasis {
    Organ,
    AttributedPerson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    pub need_linked_receiver: DecimalValue,
    pub receiver: DecimalValue,
    pub giver: DecimalValue,
    pub distance_decay: DecimalValue,
}

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            need_linked_receiver: decimal("1"),
            receiver: decimal("0.5"),
            giver: decimal("0.1"),
            distance_decay: decimal("0.5"),
        }
    }
}

impl Weights {
    fn validate(&self) -> Result<(), String> {
        for weight in [self.need_linked_receiver, self.receiver, self.giver] {
            if weight.is_negative() || weight.exact_numeric_cmp(decimal("100")).is_gt() {
                return Err("Choose evidence weights from 0 to 100".into());
            }
        }
        if self.distance_decay.is_negative()
            || self.distance_decay.exact_numeric_cmp(decimal("1")).is_gt()
        {
            return Err("Choose a distance decay from 0 to 1".into());
        }
        Ok(())
    }

    fn evidence(&self, level: EvidenceLevel) -> DecimalValue {
        match level {
            EvidenceLevel::NeedLinkedReceiver => self.need_linked_receiver,
            EvidenceLevel::Receiver => self.receiver,
            EvidenceLevel::Giver => self.giver,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub subject: Identity,
    pub role: Role,
    pub counterparty_basis: CounterpartyBasis,
    pub measurements: Vec<Measurement>,
    pub from: Option<i64>,
    pub until: Option<i64>,
    pub levels: Vec<EvidenceLevel>,
    pub endpoints: Filter,
    pub issuers: Filter,
    pub counterparties: Filter,
    pub rely_on_all_visible: bool,
    pub contacts: Option<paths::Policy>,
    pub exclude_everywhere: Vec<Identity>,
    pub weights: Weights,
    pub issuer_cap: Option<DecimalValue>,
    pub first_contact_cap: Option<DecimalValue>,
}

impl Query {
    pub fn for_subject(subject: Identity) -> Self {
        Self {
            subject,
            role: Role::Giver,
            counterparty_basis: CounterpartyBasis::Organ,
            measurements: Vec::new(),
            from: None,
            until: None,
            levels: vec![EvidenceLevel::NeedLinkedReceiver],
            endpoints: Filter::default(),
            issuers: Filter::default(),
            counterparties: Filter::default(),
            rely_on_all_visible: false,
            contacts: None,
            exclude_everywhere: Vec::new(),
            weights: Weights::default(),
            issuer_cap: None,
            first_contact_cap: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.subject.validate()?;
        self.endpoints.validate()?;
        self.issuers.validate()?;
        self.counterparties.validate()?;
        self.weights.validate()?;
        if self.measurements.len() > 32
            || self.levels.is_empty()
            || self.levels.len() > 3
            || self.levels.iter().collect::<BTreeSet<_>>().len() != self.levels.len()
            || self.exclude_everywhere.len() > super::MAX_IDENTITIES
            || self.from.zip(self.until).is_some_and(|(a, b)| a >= b)
            || [self.from, self.until]
                .into_iter()
                .flatten()
                .any(|time| chrono::DateTime::from_timestamp(time, 0).is_none())
        {
            return Err(
                "Choose distinct evidence levels, at most 32 measurements and a valid period"
                    .into(),
            );
        }
        for measurement in &self.measurements {
            measurement.validate()?;
        }
        for identity in &self.exclude_everywhere {
            identity.validate()?;
        }
        if let Some(policy) = &self.contacts {
            policy.validate()?;
        } else if self.first_contact_cap.is_some() {
            return Err("A first-contact cap requires an explicit contact-path policy".into());
        }
        if [self.issuer_cap, self.first_contact_cap]
            .into_iter()
            .flatten()
            .any(|cap| cap.is_negative())
        {
            return Err(
                "Influence caps must be nonnegative indices in each measurement basis".into(),
            );
        }
        if serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len()
            > 65536
        {
            return Err("Trust query exceeds 64 KiB".into());
        }
        Ok(())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.trust-query.policy.v1", self).map_err(|error| error.to_string())
    }
}

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
    pub agreed_quantity: Option<crate::transfer::receipt::RecordedFloat>,
    pub canonical_unit: Option<crate::vocabulary::reference::MeaningReference>,
    pub issuer: Identity,
    pub counterparty: Identity,
    pub measurement: Measurement,
    pub level: EvidenceLevel,
    pub recorded_at: i64,
    pub quantity: DecimalValue,
    pub distance: Option<usize>,
    pub path: Option<paths::Path>,
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
    pub canonical_unit: Option<crate::vocabulary::reference::MeaningReference>,
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

fn subject_matches(copy: &Copy, source: &SourceEvidence, query: &Query) -> bool {
    let role = match query.role {
        Role::Giver => ApprovalRole::Giver,
        Role::Receiver => ApprovalRole::Receiver,
    };
    match &query.subject {
        Identity::Organ(uid) => {
            uid == if query.role == Role::Giver {
                &source.giver
            } else {
                &source.receiver
            }
        }
        Identity::Person(uid) => {
            copy.attribution == PartyAttribution::OriginalOrganAuthority
                && copy
                    .persons
                    .iter()
                    .any(|person| person.role == role && &person.person == uid)
        }
    }
}

fn permits(
    query: &Query,
    identity: &Identity,
    filter: &Filter,
    contexts: &BTreeMap<Identity, IdentityContext>,
) -> bool {
    !query.exclude_everywhere.contains(identity)
        && contexts.get(identity).is_some_and(|context| {
            context.readable && !context.blocked && filter.allows(identity, context)
        })
}

fn multiply(left: DecimalValue, right: DecimalValue) -> Option<(DecimalValue, bool)> {
    let scale = |value: DecimalValue| {
        value
            .canonical()
            .split_once('.')
            .map_or(0, |(_, part)| part.len()) as u8
    };
    let product = left.mul_exact(
        right,
        scale(left).saturating_add(scale(right)).min(18),
        Rounding::HalfEven,
    )?;
    Some((product.value, !product.exact))
}

fn capped(
    value: DecimalValue,
    used: DecimalValue,
    cap: Option<DecimalValue>,
) -> Option<DecimalValue> {
    Some(if let Some(cap) = cap {
        let remaining = cap.aligned_sub(used)?;
        if value.exact_numeric_cmp(remaining).is_gt() {
            remaining
        } else {
            value
        }
    } else {
        value
    })
}

pub fn analyze(
    sources: Vec<SourceEvidence>,
    contexts: &BTreeMap<Identity, IdentityContext>,
    contact_paths: Option<&paths::ResultSet>,
    query: &Query,
) -> Result<Analysis, String> {
    query.validate()?;
    let mut analysis = Analysis {
        policy: query.id()?,
        rows: Vec::new(),
        protocol: Vec::new(),
        groups: Vec::new(),
        withheld: Vec::new(),
        partial: false,
        observed_only: true,
        input_events: 0,
    };
    if !permits(query, &query.subject, &Filter::default(), contexts) {
        return Ok(analysis);
    }
    let mut bytes = 0usize;
    let mut combined: BTreeMap<CanonicalHash, SourceEvidence> = BTreeMap::new();
    let mut source_conflicts = BTreeSet::new();
    for source in sources {
        let key = receipt_history::source_key(&source.origin, &source.occurrence)?;
        if source
            .copies
            .iter()
            .any(|copy| copy.projection.source != key)
        {
            return Err("Evidence differs from its verified source".into());
        }
        analysis.input_events = analysis.input_events.saturating_add(source.copies.len());
        bytes = bytes.saturating_add(
            serde_json::to_vec(&source.copies)
                .map_err(|error| error.to_string())?
                .len(),
        );
        if analysis.input_events > MAX_DISCLOSURES || bytes > MAX_BYTES {
            return Err("Trust analysis exceeds its event or byte budget; load bounded complete source groups".into());
        }
        if let Some(previous) = combined.get_mut(&key) {
            if previous.giver != source.giver || previous.receiver != source.receiver {
                source_conflicts.insert(key);
            }
            previous.partial |= source.partial;
            previous.copies.extend(source.copies);
        } else {
            combined.insert(key, source);
        }
    }
    if query.contacts.is_some() {
        analysis.partial |= contact_paths
            .is_none_or(|paths| paths.identity_limit_reached || paths.depth_limit_reached);
    }
    for (key, source) in combined {
        if [&source.origin, &source.giver, &source.receiver]
            .into_iter()
            .any(|uid| {
                !permits(
                    query,
                    &Identity::Organ(uid.clone()),
                    &Filter::default(),
                    contexts,
                )
            })
        {
            continue;
        }
        analysis.partial |= source.partial;
        let originals = source.copies.iter().map(|copy| copy.provenance.clone()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let records = source
            .copies
            .iter()
            .filter_map(|copy| copy.provenance.record().map(str::to_owned))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if source.partial {
            analysis.withheld.push(Withheld {
                source: key,
                reason: "incomplete_current_source",
                records,
                originals,
            });
            continue;
        }
        if source
            .copies
            .iter()
            .flat_map(|copy| &copy.persons)
            .any(|person| {
                query
                    .exclude_everywhere
                    .contains(&Identity::Person(person.person.clone()))
            })
        {
            continue;
        }
        let view = receipt_history::merge(source.copies.clone());
        if view.source_conflict || source_conflicts.contains(&key) {
            analysis.withheld.push(Withheld {
                source: key,
                reason: "conflicting_source_or_person_attribution",
                records,
                originals,
            });
            continue;
        }
        let heads = view
            .slots
            .iter()
            .filter(|slot| !slot.conflicted && slot.heads.len() == 1)
            .map(|slot| &slot.heads[0])
            .collect::<BTreeSet<_>>();
        if view.slots.iter().any(|slot| slot.conflicted) {
            analysis.withheld.push(Withheld {
                source: key.clone(),
                reason: "conflicting_current_outcomes",
                records,
                originals,
            });
            continue;
        }
        let current = view
            .outcomes
            .iter()
            .filter(|outcome| !outcome.conflicted && heads.contains(&outcome.id))
            .collect::<Vec<_>>();
        let disputed = current.iter().any(|outcome| {
            matches!(
                &outcome.copies[0].content,
                Content::Dispute { active: true, .. }
            )
        });
        let settled = current.iter().any(|outcome| {
            outcome.copies.iter().any(|copy| {
                copy.attribution == PartyAttribution::OriginalOrganAuthority
                    && match &copy.content {
                        Content::Settled { quantity }
                        | Content::SettlementStatus { quantity, .. } => {
                            quantity.value().is_ok_and(|amount| amount > 0.0)
                        }
                        _ => false,
                    }
            })
        });
        let previous_rows = analysis.rows.len();
        for outcome in &current {
            let Some(copy) = outcome
                .copies
                .iter()
                .filter(|copy| subject_matches(copy, &source, query))
                .max_by_key(|copy| copy.attribution == PartyAttribution::OriginalOrganAuthority)
            else {
                continue;
            };
            let Content::Statement {
                role,
                measurement,
                quantity,
                withdrawn,
                ..
            } = &copy.content
            else {
                continue;
            };
            if quantity.is_negative()
                || query.from.is_some_and(|from| copy.recorded_at < from)
                || query.until.is_some_and(|until| copy.recorded_at >= until)
                || (!query.measurements.is_empty() && !query.measurements.contains(measurement))
            {
                continue;
            }
            let issuer = Identity::Organ(copy.projection.issuer.clone());
            let counterparty_role = if query.role == Role::Giver {
                ApprovalRole::Receiver
            } else {
                ApprovalRole::Giver
            };
            let counterparty = match query.counterparty_basis {
                CounterpartyBasis::Organ => Identity::Organ(if query.role == Role::Giver {
                    source.receiver.clone()
                } else {
                    source.giver.clone()
                }),
                CounterpartyBasis::AttributedPerson => {
                    let Some(person) = outcome
                        .copies
                        .iter()
                        .filter(|copy| copy.attribution == PartyAttribution::OriginalOrganAuthority)
                        .flat_map(|copy| &copy.persons)
                        .find(|person| person.role == counterparty_role)
                    else {
                        continue;
                    };
                    Identity::Person(person.person.clone())
                }
            };
            if !permits(query, &issuer, &query.issuers, contexts)
                || !permits(query, &counterparty, &query.counterparties, contexts)
            {
                continue;
            }
            let path = contact_paths
                .and_then(|paths| {
                    paths.paths.iter().find(|path| {
                        path.identities.last() == Some(&counterparty)
                            && path
                                .identities
                                .iter()
                                .all(|identity| !query.exclude_everywhere.contains(identity))
                    })
                })
                .cloned();
            let at_anchor = query
                .contacts
                .as_ref()
                .is_some_and(|policy| policy.anchor == counterparty);
            if query.contacts.is_some() && path.is_none() && !at_anchor {
                continue;
            }
            let need_linked = copy.projection.need.as_ref().is_some_and(|need| view.outcomes.iter().any(|candidate| {
                candidate.id == *need && !candidate.conflicted && candidate.copies.iter().any(|binding| {
                    binding.attribution == PartyAttribution::OriginalOrganAuthority
                        && binding.projection.issuer == source.receiver
                        && matches!(&binding.content, Content::Need { measurement: expected, .. } if expected == measurement)
                })
            }));
            let level = match role {
                Role::Giver => EvidenceLevel::Giver,
                Role::Receiver
                    if need_linked
                        && settled
                        && copy.attribution == PartyAttribution::OriginalOrganAuthority =>
                {
                    EvidenceLevel::NeedLinkedReceiver
                }
                Role::Receiver => EvidenceLevel::Receiver,
            };
            let distance = if at_anchor {
                Some(0)
            } else {
                path.as_ref().map(paths::Path::distance)
            };
            analysis.rows.push(EvidenceRow {
                originals: outcome.copies.iter().map(|copy| copy.provenance.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
                source: key.clone(),
                outcome: outcome.id.clone(),
                need: copy.projection.need.clone(),
                agreed_quantity: outcome
                    .copies
                    .iter()
                    .find_map(|copy| copy.agreed_quantity.clone()),
                canonical_unit: outcome
                    .copies
                    .iter()
                    .find_map(|copy| copy.canonical_unit.clone()),
                issuer,
                counterparty,
                measurement: measurement.clone(),
                level,
                recorded_at: copy.recorded_at,
                quantity: *quantity,
                distance,
                path,
                records: outcome
                    .copies
                    .iter()
                    .filter_map(|copy| copy.provenance.record().map(str::to_owned))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                disputed,
                withdrawn: *withdrawn,
                selected: !disputed
                    && !withdrawn
                    && query.levels.contains(&level)
                    && (query.rely_on_all_visible
                        || !query.issuers.include.is_empty()
                        || query.contacts.is_some()),
                source_qualified: query.rely_on_all_visible
                    || !query.issuers.include.is_empty()
                    || query.contacts.is_some(),
                weighted_index: None,
                index_rounded: false,
                capped: false,
                arithmetic_error: false,
            });
        }
        if analysis.rows.len() > previous_rows {
            let needed = analysis.rows[previous_rows..]
                .iter()
                .filter_map(|row| row.need.as_ref())
                .collect::<BTreeSet<_>>();
            for outcome in &view.outcomes {
                let is_current = heads.contains(&outcome.id);
                if outcome.conflicted || (!is_current && !needed.contains(&outcome.id)) {
                    continue;
                }
                let copy = &outcome.copies[0];
                if matches!(copy.content, Content::Statement { .. }) {
                    continue;
                }
                analysis.protocol.push(ProtocolRow {
                    originals: outcome.copies.iter().map(|copy| copy.provenance.clone()).collect::<BTreeSet<_>>().into_iter().collect(),
                    source: key.clone(),
                    outcome: outcome.id.clone(),
                    issuer: Identity::Organ(copy.projection.issuer.clone()),
                    content: copy.content.clone(),
                    current: is_current,
                    records: outcome
                        .copies
                        .iter()
                        .filter_map(|copy| copy.provenance.record().map(str::to_owned))
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                    canonical_unit: outcome
                        .copies
                        .iter()
                        .find_map(|copy| copy.canonical_unit.clone()),
                });
            }
        }
    }
    analysis
        .rows
        .sort_by(|a, b| (&a.source, &a.outcome).cmp(&(&b.source, &b.outcome)));
    aggregate(&mut analysis, query)?;
    Ok(analysis)
}

fn aggregate(analysis: &mut Analysis, query: &Query) -> Result<(), String> {
    let mut groups: BTreeMap<(CanonicalHash, EvidenceLevel), Vec<usize>> = BTreeMap::new();
    for (index, row) in analysis
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.selected)
    {
        groups
            .entry((row.measurement.id()?, row.level))
            .or_default()
            .push(index);
    }
    let mut issuers = BTreeMap::new();
    let mut contacts = BTreeMap::new();
    for ((measurement, level), indices) in groups {
        let mut group = Group {
            measurement: analysis.rows[indices[0]].measurement.clone(),
            level,
            raw_quantity: Some(decimal("0")),
            weighted_index: Some(decimal("0")),
            occurrences: 0,
            issuers: Vec::new(),
            counterparties: Vec::new(),
            index_rounded: false,
            capped: false,
            arithmetic_error: false,
        };
        let mut sources = BTreeSet::new();
        let mut issuer_set = BTreeSet::new();
        let mut counterparties = BTreeSet::new();
        for index in indices {
            let row = &mut analysis.rows[index];
            group.raw_quantity = group
                .raw_quantity
                .and_then(|sum| sum.aligned_add(row.quantity));
            let mut weighted = multiply(row.quantity, query.weights.evidence(level));
            for _ in 1..row.distance.unwrap_or(1) {
                weighted = weighted.and_then(|(value, rounded)| {
                    multiply(value, query.weights.distance_decay)
                        .map(|(next, changed)| (next, rounded || changed))
                });
            }
            let computed = weighted.and_then(|(value, rounded)| {
                let issuer_key = (measurement.clone(), row.issuer.clone());
                let issuer_used = *issuers.get(&issuer_key).unwrap_or(&decimal("0"));
                let retained = capped(value, issuer_used, query.issuer_cap)?;
                let first = row
                    .path
                    .as_ref()
                    .and_then(|path| path.identities.get(1))
                    .unwrap_or(&row.counterparty);
                let contact_key = (measurement.clone(), first.clone());
                let contact_used = *contacts.get(&contact_key).unwrap_or(&decimal("0"));
                let retained = capped(retained, contact_used, query.first_contact_cap)?;
                let next_issuer = if query.issuer_cap.is_some() {
                    Some(issuer_used.aligned_add(retained)?)
                } else {
                    None
                };
                let next_contact = if query.first_contact_cap.is_some() {
                    Some(contact_used.aligned_add(retained)?)
                } else {
                    None
                };
                if let Some(next) = next_issuer {
                    issuers.insert(issuer_key, next);
                }
                if let Some(next) = next_contact {
                    contacts.insert(contact_key, next);
                }
                Some((retained, rounded, retained.exact_numeric_cmp(value).is_lt()))
            });
            if let Some((value, rounded, capped)) = computed {
                row.weighted_index = Some(value);
                row.index_rounded = rounded;
                row.capped = capped;
                group.weighted_index = group.weighted_index.and_then(|sum| sum.aligned_add(value));
            } else {
                row.arithmetic_error = true;
                group.weighted_index = None;
            }
            sources.insert(row.source.clone());
            issuer_set.insert(row.issuer.clone());
            counterparties.insert(row.counterparty.clone());
            group.index_rounded |= row.index_rounded;
            group.capped |= row.capped;
            group.arithmetic_error |= row.arithmetic_error
                || group.raw_quantity.is_none()
                || group.weighted_index.is_none();
        }
        group.occurrences = sources.len();
        group.issuers = issuer_set.into_iter().collect();
        group.counterparties = counterparties.into_iter().collect();
        analysis.groups.push(group);
    }
    if query.issuer_cap.is_some() || query.first_contact_cap.is_some() {
        let uncertain = analysis
            .rows
            .iter()
            .filter(|row| row.arithmetic_error)
            .map(|row| row.measurement.id())
            .collect::<Result<BTreeSet<_>, _>>()?;
        for row in &mut analysis.rows {
            if uncertain.contains(&row.measurement.id()?) {
                row.weighted_index = None;
                row.arithmetic_error = true;
            }
        }
        for group in &mut analysis.groups {
            if uncertain.contains(&group.measurement.id()?) {
                group.weighted_index = None;
                group.arithmetic_error = true;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
