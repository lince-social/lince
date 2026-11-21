//! Prepares Transfer filters and evaluates them against the viewer’s disclosed data.

use super::{projection::RecordIndex, transfer_predicate_name, transfer_query_error};
use crate::{Predicate, ProteinError};
use chrono::{DateTime, FixedOffset};
use nucleus::{
    RecordKind,
    performance::{Span, add},
    transfer::{TransferRevisionPromise, disclosure::ItemAccess},
};
use std::{
    collections::{HashMap, HashSet},
    slice::from_ref,
};
use store::{
    Store, concepts,
    misc::PromiseRow,
    records::{self, RecordSelection},
    transfers::TransferInvitationRow,
};

const TRANSFER_STATUSES: &[&str] = &[
    "inactive",
    "draft",
    "withdrawn",
    "cancelled",
    "settled",
    "partially_settled",
    "satiated",
    "system_disputed",
    "disputed",
    "broken",
    "in_transfer",
    "agreed",
    "proposed",
];
const TRANSFER_VIEWER_ROLES: &[&str] = &["local", "creator", "participant", "invitee", "observer"];
const TRANSFER_INVITATION_STATES: &[&str] =
    &["pending", "accepted", "rejected", "withdrawn", "expired"];
const TRANSFER_PROMISE_STATES: &[&str] = &[
    "open",
    "proposed",
    "agreed",
    "active",
    "kept",
    "broken",
    "withdrawn",
];

#[derive(Default)]
pub(super) struct Context<'filter> {
    needs: Needs,
    record_tokens: HashMap<&'filter str, Option<String>>,
    person_tokens: HashMap<&'filter str, Option<String>>,
    unit_tokens: HashMap<&'filter str, Option<String>>,
    concept_families: HashMap<&'filter str, HashSet<String>>,
    instants: HashMap<&'filter str, DateTime<FixedOffset>>,
}

impl<'filter> Context<'filter> {
    pub(super) async fn prepare(
        store: &Store,
        predicates: &'filter [Predicate],
    ) -> Result<Self, ProteinError> {
        let _prepare = Span::new("protein-transfer-filter-prepare-await");
        let mut context = Self::default();
        if predicates.is_empty() {
            return Ok(context);
        }
        let mut pending = vec![predicates.iter()];
        while let Some(level) = pending.last_mut() {
            let Some(predicate) = level.next() else {
                pending.pop();
                continue;
            };
            match predicate {
                Predicate::All(inner) | Predicate::Any(inner) => {
                    pending.push(inner.iter());
                }
                Predicate::Not(inner) => {
                    pending.push(from_ref(inner.as_ref()).iter());
                }
                Predicate::UidEq(_)
                | Predicate::SlugEq(_)
                | Predicate::RevisionEq(_)
                | Predicate::RevisionLt(_)
                | Predicate::RevisionLte(_)
                | Predicate::RevisionGt(_)
                | Predicate::RevisionGte(_) => {}
                Predicate::StatusIn(statuses) => {
                    Self::validate_values("status_in", statuses, TRANSFER_STATUSES)?;
                }
                Predicate::ViewerRoleIn(roles) => {
                    Self::validate_values("viewer_role_in", roles, TRANSFER_VIEWER_ROLES)?;
                }
                Predicate::InvitationStateIn(states) => {
                    context.needs.invitations = true;
                    Self::validate_values(
                        "invitation_state_in",
                        states,
                        TRANSFER_INVITATION_STATES,
                    )?;
                }
                Predicate::StateIn(states) => {
                    Self::validate_values("state_in", states, TRANSFER_PROMISE_STATES)?;
                }
                Predicate::PersonEq(token) => {
                    context.needs.parties = true;
                    context.needs.invitations = true;
                    context.needs.promises = true;
                    let resolved = records::read::<Option<String>>(
                        &store.pool,
                        RecordSelection::Token(token),
                        false,
                    )
                    .await?
                    .pop()
                    .filter(|record| record.kind == RecordKind::Person.as_str())
                    .map(|record| record.uid);
                    context.person_tokens.insert(token.as_str(), resolved);
                }
                Predicate::RecordEq(token) => {
                    context.needs.promises = true;
                    let resolved = records::resolve_uid(&store.pool, token).await?;
                    context.record_tokens.insert(token.as_str(), resolved);
                }
                Predicate::ConceptIn(token) => {
                    context.needs.promises = true;
                    let family = match concepts::resolve(&store.pool, token).await? {
                        Some(uid) => concepts::descendants_including(&store.pool, &uid)
                            .await?
                            .into_iter()
                            .collect(),
                        None => HashSet::new(),
                    };
                    context.concept_families.insert(token.as_str(), family);
                }
                Predicate::UnitEq(token) => {
                    context.needs.promises = true;
                    let resolved = concepts::resolve(&store.pool, token).await?;
                    context.unit_tokens.insert(token.as_str(), resolved);
                }
                Predicate::WindowEndBefore(value) | Predicate::WindowEndAfter(value) => {
                    context.needs.promises = true;
                    let instant = DateTime::parse_from_rfc3339(value).map_err(|_| {
                        transfer_query_error(
                            "protein_transfer_invalid_window",
                            format_args!("`{value}` is not an RFC3339 instant"),
                        )
                    })?;
                    context.instants.insert(value.as_str(), instant);
                }
                unsupported => {
                    return Err(transfer_query_error(
                        "protein_transfer_unsupported_predicate",
                        transfer_predicate_name(unsupported),
                    ));
                }
            }
        }
        Ok(context)
    }

    fn validate_values(
        predicate: &str,
        values: &[String],
        supported: &[&str],
    ) -> Result<(), ProteinError> {
        if let Some(value) = values
            .iter()
            .find(|value| !supported.contains(&value.as_str()))
        {
            return Err(transfer_query_error(
                "protein_transfer_invalid_predicate_value",
                format_args!("{predicate} does not support `{value}`"),
            ));
        }
        Ok(())
    }

    pub(super) fn matches(&self, row: &Row<'_>, predicates: &[Predicate]) -> bool {
        let mut details = None;
        let _matching = Span::new("protein-transfer-filter-match");
        predicates
            .iter()
            .all(|predicate| self.matches_one(row, &mut details, predicate))
    }

    fn matches_one<'a>(
        &self,
        row: &Row<'a>,
        details: &mut Option<Details<'a>>,
        predicate: &Predicate,
    ) -> bool {
        match predicate {
            Predicate::All(inner) => inner
                .iter()
                .all(|item| self.matches_one(row, details, item)),
            Predicate::Any(inner) => inner
                .iter()
                .any(|item| self.matches_one(row, details, item)),
            Predicate::Not(inner) => !self.matches_one(row, details, inner),
            Predicate::UidEq(uid) => row.uid == uid,
            Predicate::SlugEq(slug) => row.slug == Some(slug.as_str()),
            Predicate::RevisionEq(revision) => row.revision == *revision,
            Predicate::RevisionLt(revision) => row.revision < *revision,
            Predicate::RevisionLte(revision) => row.revision <= *revision,
            Predicate::RevisionGt(revision) => row.revision > *revision,
            Predicate::RevisionGte(revision) => row.revision >= *revision,
            Predicate::StatusIn(statuses) => statuses.iter().any(|status| status == row.status),
            Predicate::ViewerRoleIn(roles) => roles
                .iter()
                .any(|role| row.viewer_roles.contains(&role.as_str())),
            Predicate::InvitationStateIn(states) => {
                !states.is_empty()
                    && !row.invitations.is_empty()
                    && self
                        .details(row, details)
                        .invitations
                        .iter()
                        .any(|invitation| {
                            states
                                .iter()
                                .any(|state| state == invitation.status.as_str())
                        })
            }
            Predicate::StateIn(states) => {
                !states.is_empty()
                    && row
                        .promises
                        .iter()
                        .any(|promise| states.iter().any(|state| state == promise.state.as_str()))
            }
            Predicate::PersonEq(token) => self
                .person_tokens
                .get(token.as_str())
                .and_then(Option::as_deref)
                .is_some_and(|person| {
                    let details = self.details(row, details);
                    details.people.contains(&person)
                        || details.invitations.iter().any(|invitation| {
                            invitation.addressed_person_uid == person
                                || invitation.invited_by_person_uid == person
                        })
                        || details
                            .promises
                            .iter()
                            .any(|promise| promise.person == Some(person))
                }),
            Predicate::RecordEq(token) => self
                .record_tokens
                .get(token.as_str())
                .and_then(Option::as_deref)
                .is_some_and(|record| {
                    self.details(row, details)
                        .promises
                        .iter()
                        .any(|promise| promise.record == Some(record))
                }),
            Predicate::ConceptIn(token) => {
                self.concept_families
                    .get(token.as_str())
                    .is_some_and(|family| {
                        !family.is_empty()
                            && self.details(row, details).promises.iter().any(|promise| {
                                promise
                                    .concept
                                    .is_some_and(|concept| family.contains(concept))
                            })
                    })
            }
            Predicate::UnitEq(token) => self
                .unit_tokens
                .get(token.as_str())
                .and_then(Option::as_deref)
                .is_some_and(|unit| {
                    self.details(row, details)
                        .promises
                        .iter()
                        .any(|promise| promise.unit == Some(unit))
                }),
            Predicate::WindowEndBefore(value) => self
                .instants
                .get(value.as_str())
                .is_some_and(|until| self.details(row, details).any_window_end(|end| end < until)),
            Predicate::WindowEndAfter(value) => self
                .instants
                .get(value.as_str())
                .is_some_and(|since| self.details(row, details).any_window_end(|end| end > since)),
            _ => false,
        }
    }

    fn details<'a, 'b>(
        &self,
        row: &Row<'a>,
        details: &'b mut Option<Details<'a>>,
    ) -> &'b mut Details<'a> {
        details.get_or_insert_with(|| Details::new(row, self.needs))
    }
}

pub(super) struct Row<'a> {
    pub uid: &'a str,
    pub slug: Option<&'a str>,
    pub revision: u64,
    pub status: &'a str,
    pub viewer_roles: &'a [&'static str],
    pub invitations: &'a [TransferInvitationRow],
    pub parties: &'a [(String, String, i64)],
    pub promises: &'a [PromiseRow],
    pub revision_promises: Option<&'a [TransferRevisionPromise]>,
    pub records_by_uid: &'a RecordIndex<'a>,
    pub viewer_person: Option<&'a str>,
    pub viewer_involved: bool,
    pub viewer_privileged: bool,
}

#[derive(Clone, Copy, Default)]
struct Needs {
    promises: bool,
    parties: bool,
    invitations: bool,
}

struct Promise<'a> {
    record: Option<&'a str>,
    concept: Option<&'a str>,
    person: Option<&'a str>,
    unit: Option<&'a str>,
    window_end: Option<&'a str>,
}

struct Details<'a> {
    promises: Vec<Promise<'a>>,
    people: Vec<&'a str>,
    invitations: Vec<&'a TransferInvitationRow>,
    // The outer option tracks whether this window has been parsed.
    windows: Vec<Option<Option<DateTime<FixedOffset>>>>,
}

impl<'a> Details<'a> {
    fn new(row: &Row<'a>, needs: Needs) -> Self {
        let _projection = Span::new("protein-transfer-filter-project");
        let access = row
            .revision_promises
            .unwrap_or_default()
            .iter()
            .map(|promise| {
                (
                    promise.uid.as_str(),
                    ItemAccess::for_item(
                        promise.item.as_ref(),
                        row.viewer_person,
                        promise.person_uid.as_deref(),
                        row.viewer_involved,
                        row.viewer_privileged,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        let hide_people = access.values().any(|access| !access.parties);
        let visible_people = if hide_people && (needs.parties || needs.invitations) {
            let mut visible_people = row
                .promises
                .iter()
                .filter_map(|promise| {
                    promise.party_uid.as_deref().filter(|_| {
                        access
                            .get(promise.uid.as_str())
                            .is_none_or(|access| access.parties)
                    })
                })
                .chain(row.viewer_person)
                .collect::<HashSet<_>>();
            for promise in row.revision_promises.unwrap_or_default() {
                if access
                    .get(promise.uid.as_str())
                    .is_some_and(|access| access.parties)
                    && let Some(exchange) = promise
                        .item
                        .as_ref()
                        .and_then(|item| item.exchange.as_ref())
                {
                    visible_people.insert(exchange.giver.as_str());
                    visible_people.insert(exchange.receiver.as_str());
                }
            }
            visible_people
        } else {
            HashSet::new()
        };
        let people = if needs.parties {
            row.parties
                .iter()
                .filter(|(_, person, _)| !hide_people || visible_people.contains(person.as_str()))
                .map(|(_, person, _)| person.as_str())
                .collect()
        } else {
            Vec::new()
        };
        let invitations = if needs.invitations {
            row.invitations
                .iter()
                .filter(|invitation| {
                    !hide_people
                        || (visible_people.contains(invitation.addressed_person_uid.as_str())
                            && visible_people.contains(invitation.invited_by_person_uid.as_str()))
                })
                .collect()
        } else {
            Vec::new()
        };
        let promises: Vec<_> = if needs.promises {
            match row.revision_promises {
                Some(promises) => promises
                    .iter()
                    .map(|promise| {
                        let access = access.get(promise.uid.as_str());
                        let record = promise
                            .record_uid
                            .as_deref()
                            .filter(|_| access.is_none_or(|access| access.source));
                        let metadata = record.and_then(|uid| row.records_by_uid.get(uid));
                        Promise {
                            record,
                            concept: promise
                                .concept_uid
                                .as_deref()
                                .filter(|_| access.is_none_or(|access| access.source))
                                .or_else(|| {
                                    metadata
                                        .and_then(|record| record.identity_predicate_uid.as_deref())
                                }),
                            person: promise
                                .person_uid
                                .as_deref()
                                .filter(|_| access.is_none_or(|access| access.parties)),
                            unit: promise
                                .unit_uid
                                .as_deref()
                                .filter(|_| access.is_none_or(|access| access.quantity)),
                            window_end: promise.window_end.as_deref(),
                        }
                    })
                    .collect(),
                None => row
                    .promises
                    .iter()
                    .map(|promise| {
                        let access = access.get(promise.uid.as_str());
                        let record = promise
                            .record_uid
                            .as_deref()
                            .filter(|_| access.is_none_or(|access| access.source));
                        let metadata = record.and_then(|uid| row.records_by_uid.get(uid));
                        Promise {
                            record,
                            concept: metadata
                                .and_then(|record| record.identity_predicate_uid.as_deref()),
                            person: promise
                                .party_uid
                                .as_deref()
                                .filter(|_| access.is_none_or(|access| access.parties)),
                            unit: metadata.and_then(|record| record.unit_uid.as_deref()),
                            window_end: promise.window_end.as_deref(),
                        }
                    })
                    .collect(),
            }
        } else {
            Vec::new()
        };
        add(
            "protein-transfer-filter-projected-promises",
            promises.len() as u64,
        );
        Self {
            promises,
            people,
            invitations,
            windows: Vec::new(),
        }
    }

    fn any_window_end(&mut self, matches: impl Fn(&DateTime<FixedOffset>) -> bool) -> bool {
        self.windows.resize(self.promises.len(), None);
        let mut parsed = 0;
        let matched = self
            .promises
            .iter()
            .zip(&mut self.windows)
            .any(|(promise, cached)| {
                cached
                    .get_or_insert_with(|| {
                        promise.window_end.and_then(|text| {
                            parsed += 1;
                            DateTime::parse_from_rfc3339(text).ok()
                        })
                    })
                    .as_ref()
                    .is_some_and(&matches)
            });
        add("protein-transfer-filter-window-parses", parsed);
        matched
    }
}
