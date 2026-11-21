//! Borrowed document views for allocation-free serialization and shared validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::{
    Approval, Authority, Content, MAX_AUTHORITIES, MAX_BUNDLE_BYTES, MAX_BYTES, MAX_EVENTS,
    PersonApproval, Scope, SignedEvent, signature_shape,
};

#[derive(Clone, Copy)]
pub(super) enum Validation {
    Review,
    Fragment,
    Complete,
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Publication<'a> {
    #[serde(skip_serializing_if = "empty_slice")]
    pub person_approvals: &'a [PersonApproval],
    pub scope: &'a Scope,
    pub approvals: &'a [Approval],
    pub events: &'a [SignedEvent],
}

fn empty_slice<T>(values: &&[T]) -> bool {
    values.is_empty()
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Bundle<'a> {
    pub publication: Publication<'a>,
    pub authorities: &'a [Authority],
}

impl<'a> From<&'a super::Publication> for Publication<'a> {
    fn from(value: &'a super::Publication) -> Self {
        Self {
            person_approvals: &value.person_approvals,
            scope: &value.scope,
            approvals: &value.approvals,
            events: &value.events,
        }
    }
}

impl<'a> From<&'a super::Bundle> for Bundle<'a> {
    fn from(value: &'a super::Bundle) -> Self {
        Self {
            publication: Publication::from(&value.publication),
            authorities: &value.authorities,
        }
    }
}

impl<'a> From<&'a super::Review> for Bundle<'a> {
    fn from(value: &'a super::Review) -> Self {
        Self {
            publication: Publication {
                person_approvals: &value.person_approvals,
                scope: &value.scope,
                approvals: &value.approvals,
                events: &[],
            },
            authorities: &value.authorities,
        }
    }
}

impl Publication<'_> {
    pub(super) fn validate_mode(&self, now: i64, mode: Validation) -> Result<(), String> {
        self.scope.validate()?;
        let complete = !matches!(mode, Validation::Review);
        let dependencies = !matches!(mode, Validation::Fragment);
        let latest = now.saturating_add(300);
        if self.scope.issued_at > latest
            || self.approvals.len() > 3
            || (!complete && !self.events.is_empty())
            || self.events.len() > MAX_EVENTS
            || crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_BYTES
        {
            return Err("Invalid public receipt approval, time or resource budget".into());
        }
        let mut roles = BTreeSet::new();
        let mut latest_approval = self.scope.issued_at;
        let mut persons = BTreeSet::new();
        if self.person_approvals.len() > self.scope.public_persons.len() {
            return Err("Person consent includes an unrequested identity".into());
        }
        if complete && self.person_approvals.len() != self.scope.public_persons.len() {
            return Err(
                "Public Person identities require their own explicit consent signatures".into(),
            );
        }
        for approval in self.person_approvals {
            approval.validate(self.scope, now)?;
            if !persons.insert(approval.role) {
                return Err("Person consent must not repeat a role".into());
            }
            latest_approval = latest_approval.max(approval.issued_at);
        }
        for approval in self.approvals {
            signature_shape(&approval.key_id, &approval.signature)?;
            if !roles.insert(approval.role)
                || approval.issued_at < self.scope.issued_at
                || approval.issued_at >= self.scope.expires_at
                || approval.issued_at > latest
            {
                return Err(
                    "Receipt requires distinct origin, giver and receiver approvals".into(),
                );
            }
            latest_approval = latest_approval.max(approval.issued_at);
        }
        roles.extend(self.scope.upfront_roles());
        if complete && roles.len() != 3 {
            return Err("Receipt requires origin attestation and each party's exact-scope or signed upfront publication consent".into());
        }
        let mut events = BTreeMap::new();
        let mut scope_id = None;
        for signed in self.events {
            signature_shape(&signed.key_id, &signed.signature)?;
            let id = scope_id
                .get_or_insert_with(|| self.scope.validated_id())
                .as_ref()
                .map_err(Clone::clone)?;
            signed.event.validate_for_scope(self.scope, id, now)?;
            if signed.event.issued_at < latest_approval {
                return Err("Public events must follow approval of their scope".into());
            }
            if events.insert(signed.event.id()?, &signed.event).is_some() {
                return Err("A receipt bundle must not repeat an event".into());
            }
        }
        let mut roots = BTreeSet::new();
        for (id, event) in &events {
            let slot = event.slot(self.scope)?;
            if let Some(parent) = &event.supersedes {
                if parent == id {
                    return Err("Receipt correction changes its source, role or measurement".into());
                }
                if let Some(previous) = events.get(parent) {
                    if previous.slot(self.scope)? != slot || previous.issued_at > event.issued_at {
                        return Err(
                            "Receipt correction changes its source, role or measurement".into()
                        );
                    }
                } else if dependencies {
                    return Err("Receipt correction is missing its original".into());
                }
            } else if !roots.insert(slot) && dependencies {
                return Err("Repeated outcome roots cannot multiply contribution history".into());
            }
            if let Content::Statement {
                measurement,
                need: Some(need),
                ..
            } = &event.content
            {
                if let Some(binding) = events.get(need) {
                    if !matches!(&binding.content, Content::Need { measurement: bound, .. } if bound == measurement)
                        || binding.issued_at > event.issued_at
                    {
                        return Err(
                            "Receipt confirmation does not match its receiver-authored Need".into(),
                        );
                    }
                } else if dependencies {
                    return Err("Receipt statement is missing its Need proof".into());
                }
            }
            let mut cursor = *event;
            let mut visited = BTreeSet::new();
            while let Some(parent) = &cursor.supersedes {
                if !visited.insert(parent) {
                    return Err("Receipt correction history contains a loop".into());
                }
                if let Some(previous) = events.get(parent) {
                    cursor = previous;
                } else if dependencies {
                    return Err("Receipt correction history is incomplete".into());
                } else {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl Bundle<'_> {
    pub(super) fn validate_mode(&self, now: i64, mode: Validation) -> Result<(), String> {
        self.publication.validate_mode(now, mode)?;
        if self.authorities.len() + self.publication.scope.original_authorities.len()
            > MAX_AUTHORITIES
            || crate::json::encoded_len(self).map_err(|error| error.to_string())? > MAX_BUNDLE_BYTES
        {
            return Err("Public receipt authority or byte budget exceeded".into());
        }
        let mut bindings = BTreeMap::new();
        let scope = self.publication.scope;
        for authority in self.authorities {
            authority.validate(now)?;
            if authority.participation.is_some() {
                return Err(
                    "Participation approval cannot authorize receipt publication signatures".into(),
                );
            }
            if ![&scope.source.origin, &scope.giver, &scope.receiver].contains(&&authority.organ) {
                return Err("Receipt includes an unrelated signing authority".into());
            }
            if bindings
                .insert(
                    (&authority.organ, &authority.key_id),
                    &authority.signing_key,
                )
                .is_some_and(|key| key != &authority.signing_key)
            {
                return Err("Receipt signing key identifier has conflicting bindings".into());
            }
        }
        Ok(())
    }
}
