//! Select observable evidence and retain the protocol context for the results.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    mem::take,
};

use super::{
    super::{Filter, Identity, IdentityContext, MAX_BYTES, MAX_DISCLOSURES, paths},
    Analysis, CounterpartyBasis, EvidenceLevel, EvidenceRow, ProtocolRow, Query, SourceEvidence,
    Withheld,
    aggregate::aggregate,
};
use crate::{
    json::encoded_len,
    karma::CanonicalHash,
    transfer::{
        outcome::Role,
        receipt::{ApprovalRole, Content, Measurement, PartyAttribution},
        receipt_history::{self, Copy, Provenance, View},
    },
};

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

fn provenance<'a>(copies: impl Iterator<Item = &'a Copy>) -> (Vec<Provenance>, Vec<String>) {
    let originals = copies.map(|copy| &copy.provenance).collect::<BTreeSet<_>>();
    let records = originals
        .iter()
        .filter_map(|original| original.record())
        .collect::<BTreeSet<_>>();
    (
        originals.into_iter().cloned().collect(),
        records.into_iter().map(str::to_owned).collect(),
    )
}

fn withheld<'a>(
    source: CanonicalHash,
    reason: &'static str,
    copies: impl Iterator<Item = &'a Copy>,
) -> Withheld {
    let (originals, records) = provenance(copies);
    Withheld {
        source,
        reason,
        records,
        originals,
    }
}

fn need_linked(view: &View, copy: &Copy, receiver: &str, measurement: &Measurement) -> bool {
    let Some(need) = &copy.projection.need else {
        return false;
    };
    view.outcomes
        .binary_search_by(|outcome| outcome.id.cmp(need))
        .ok()
        .and_then(|index| view.outcomes.get(index))
        .is_some_and(|outcome| {
            !outcome.conflicted
                && outcome.copies.iter().any(|binding| {
                    binding.attribution == PartyAttribution::OriginalOrganAuthority
                        && binding.projection.issuer == receiver
                        && matches!(&binding.content, Content::Need { measurement: expected, .. }
                        if expected == measurement)
                })
        })
}

/// Evaluate query policy against observed source evidence and identity context.
///
/// # Errors
///
/// Policy or evidence is invalid, exceeds a budget, or cannot be encoded.
pub fn analyze(
    sources: Vec<SourceEvidence>,
    contexts: &BTreeMap<Identity, IdentityContext>,
    contact_paths: Option<&paths::ResultSet>,
    query: &Query,
) -> Result<Analysis, String> {
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
    let mut combined = BTreeMap::<_, (SourceEvidence, bool)>::new();
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
        bytes =
            bytes.saturating_add(encoded_len(&source.copies).map_err(|error| error.to_string())?);
        if analysis.input_events > MAX_DISCLOSURES || bytes > MAX_BYTES {
            return Err("Trust analysis exceeds its event or byte budget; load bounded complete source groups".into());
        }
        match combined.entry(key) {
            Entry::Occupied(mut entry) => {
                let (previous, conflicting) = entry.get_mut();
                *conflicting |=
                    previous.giver != source.giver || previous.receiver != source.receiver;
                previous.partial |= source.partial;
                previous.copies.extend(source.copies);
            }
            Entry::Vacant(entry) => {
                entry.insert((source, false));
            }
        }
    }
    if query.contacts.is_some() {
        analysis.partial |= contact_paths
            .is_none_or(|paths| paths.identity_limit_reached || paths.depth_limit_reached);
    }
    let source_qualified =
        query.rely_on_all_visible || !query.issuers.include.is_empty() || query.contacts.is_some();
    for (key, (mut source, conflicting)) in combined {
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
        if source.partial {
            analysis.withheld.push(withheld(
                key,
                "incomplete_current_source",
                source.copies.iter(),
            ));
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
        let view = receipt_history::merge(take(&mut source.copies));
        let copies = || view.outcomes.iter().flat_map(|outcome| &outcome.copies);
        if view.source_conflict || conflicting {
            analysis.withheld.push(withheld(
                key,
                "conflicting_source_or_person_attribution",
                copies(),
            ));
            continue;
        }
        if view.slots.iter().any(|slot| slot.conflicted) {
            analysis
                .withheld
                .push(withheld(key, "conflicting_current_outcomes", copies()));
            continue;
        }
        let heads = view
            .slots
            .iter()
            .filter_map(|slot| match slot.heads.as_slice() {
                [head] => Some(head),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let current = view
            .outcomes
            .iter()
            .filter(|outcome| !outcome.conflicted && heads.contains(&outcome.id))
            .collect::<Vec<_>>();
        let disputed = current.iter().any(|outcome| {
            outcome
                .copies
                .first()
                .is_some_and(|copy| matches!(copy.content, Content::Dispute { active: true, .. }))
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
            let path = contact_paths.and_then(|paths| {
                paths.paths.iter().find(|path| {
                    path.identities.last() == Some(&counterparty)
                        && path
                            .identities
                            .iter()
                            .all(|identity| !query.exclude_everywhere.contains(identity))
                })
            });
            let at_anchor = query
                .contacts
                .as_ref()
                .is_some_and(|policy| policy.anchor == counterparty);
            if query.contacts.is_some() && path.is_none() && !at_anchor {
                continue;
            }
            let level = match role {
                Role::Giver => EvidenceLevel::Giver,
                Role::Receiver
                    if settled
                        && copy.attribution == PartyAttribution::OriginalOrganAuthority
                        && need_linked(&view, copy, &source.receiver, measurement) =>
                {
                    EvidenceLevel::NeedLinkedReceiver
                }
                Role::Receiver => EvidenceLevel::Receiver,
            };
            let distance = if at_anchor {
                Some(0)
            } else {
                path.map(paths::Path::distance)
            };
            let (originals, records) = provenance(outcome.copies.iter());
            analysis.rows.push(EvidenceRow {
                originals,
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
                path: path.cloned(),
                records,
                disputed,
                withdrawn: *withdrawn,
                selected: !disputed
                    && !withdrawn
                    && query.levels.contains(&level)
                    && source_qualified,
                source_qualified,
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
                let Some(copy) = outcome.copies.first() else {
                    continue;
                };
                if matches!(copy.content, Content::Statement { .. }) {
                    continue;
                }
                let (originals, records) = provenance(outcome.copies.iter());
                analysis.protocol.push(ProtocolRow {
                    originals,
                    source: key.clone(),
                    outcome: outcome.id.clone(),
                    issuer: Identity::Organ(copy.projection.issuer.clone()),
                    content: copy.content.clone(),
                    current: is_current,
                    records,
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
