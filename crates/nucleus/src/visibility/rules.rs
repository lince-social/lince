use std::collections::HashSet;

use serde::Serialize;

use super::{
    Bound, Condition, Data, Decision, MAX_CONDITIONS, MAX_ORGANS_PER_CONDITION, MAX_POLICY_BYTES,
    Policy,
};
use crate::{json::encoded_len, place::valid_area_radius, valid_uid};

impl Condition {
    /// Checks an Organ without a specific Person identity.
    pub fn matches(&self, organ: &str, proximity: Option<u32>) -> bool {
        self.matches_principal(organ, None, proximity)
    }

    /// Checks principal identity and the supplied proximity against this condition.
    pub fn matches_principal(
        &self,
        organ: &str,
        person: Option<&str>,
        proximity: Option<u32>,
    ) -> bool {
        if organ.is_empty()
            || (!self.organs.is_empty() && !self.organs.iter().any(|uid| uid == organ))
            || (!self.persons.is_empty()
                && !person.is_some_and(|person| self.persons.iter().any(|uid| uid == person)))
        {
            return false;
        }
        if self.lower.is_none() && self.upper.is_none() {
            return true;
        }
        proximity.is_some_and(|value| {
            self.lower.is_none_or(|bound| {
                value > bound.value || (bound.inclusive && value == bound.value)
            }) && self.upper.is_none_or(|bound| {
                value < bound.value || (bound.inclusive && value == bound.value)
            })
        })
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self
            .radius_metres
            .is_some_and(|radius| !valid_area_radius(radius))
        {
            return Err("Choose an approximate area radius between 100 and 100000 metres");
        }
        if self.organs.len() > MAX_ORGANS_PER_CONDITION {
            return Err("Choose at most 64 Organs in each visibility condition");
        }
        if self.persons.len() > MAX_ORGANS_PER_CONDITION {
            return Err("Choose at most 64 Persons in each visibility condition");
        }
        let mut seen = HashSet::with_capacity(self.organs.len().max(self.persons.len()));
        if self
            .organs
            .iter()
            .any(|uid| !valid_uid(uid, "r") || !seen.insert(uid))
        {
            return Err("Choose distinct valid Organ identities");
        }
        seen.clear();
        if self
            .persons
            .iter()
            .any(|uid| !valid_uid(uid, "r") || !seen.insert(uid))
        {
            return Err("Choose distinct valid Person identities");
        }
        let minimum = match self.lower {
            Some(Bound {
                value,
                inclusive: false,
            }) => value.checked_add(1),
            Some(Bound { value, .. }) => Some(value),
            None => Some(0),
        };
        let maximum = match self.upper {
            Some(Bound {
                value,
                inclusive: false,
            }) => value.checked_sub(1),
            Some(Bound { value, .. }) => Some(value),
            None => Some(u32::MAX),
        };
        if minimum.zip(maximum).is_none_or(|(low, high)| low > high) {
            return Err("Choose a nonempty proximity range");
        }
        Ok(())
    }
}

// Because this is the borrowed form of the policy, it must encode in the same
// field order. Policy size limits and audience stages use the same bytes.
#[derive(Serialize)]
pub(super) struct PolicyView<'rules> {
    pub(super) include: &'rules [Condition],
    pub(super) exclude: &'rules [Condition],
}

impl PolicyView<'_> {
    pub(super) fn validate_for(&self, data: Data) -> Result<(), &'static str> {
        self.validate()?;
        if data == Data::Record
            && self
                .include
                .iter()
                .any(|condition| condition.radius_metres.is_some())
        {
            return Err("Approximate areas apply to saved places and live locations");
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.include.len().saturating_add(self.exclude.len()) > MAX_CONDITIONS {
            return Err("Choose at most 64 visibility conditions");
        }
        for condition in self.include.iter().chain(self.exclude) {
            condition.validate()?;
        }
        if self
            .exclude
            .iter()
            .any(|condition| condition.radius_metres.is_some())
        {
            return Err("Exclusions deny access; choose an area size on an inclusion");
        }
        if encoded_len(self).map_err(|_| "Cannot encode visibility rules")? > MAX_POLICY_BYTES {
            return Err(
                "Visibility rules exceed the 12 KiB limit; use fewer conditions or identities",
            );
        }
        Ok(())
    }
}

impl Policy {
    fn view(&self) -> PolicyView<'_> {
        PolicyView {
            include: &self.include,
            exclude: &self.exclude,
        }
    }

    /// Validates the rules for the requested type of data.
    ///
    /// # Errors
    ///
    /// Returns validation errors or an error if a Record rule sets an area radius.
    pub fn validate_for(&self, data: Data) -> Result<(), &'static str> {
        self.view().validate_for(data)
    }

    /// Validates all conditions and the encoded size.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid conditions, a radius in an exclusion, or excess size.
    pub fn validate(&self) -> Result<(), &'static str> {
        self.view().validate()
    }

    /// Decides whether a policy allows replication to the specified Organ.
    pub fn decide(&self, organ: &str, proximity: Option<u32>) -> Decision {
        self.decide_principal(organ, None, proximity)
    }

    /// Explains which rules admit or exclude a principal at the supplied proximity.
    pub fn decide_principal(
        &self,
        organ: &str,
        person: Option<&str>,
        proximity: Option<u32>,
    ) -> Decision {
        let mut included_by = Vec::new();
        let mut radius_metres: Option<u32> = None;
        for (index, condition) in self.include.iter().enumerate() {
            if condition.matches_principal(organ, person, proximity) {
                included_by.push(index);
                let candidate = condition.radius_metres.unwrap_or(0);
                radius_metres =
                    Some(radius_metres.map_or(candidate, |radius| radius.min(candidate)));
            }
        }
        let excluded_by: Vec<_> = self
            .exclude
            .iter()
            .enumerate()
            .filter_map(|(index, condition)| {
                // Organ-wide replication still observes any matching Person exclusion.
                let target = person.or_else(|| condition.persons.first().map(String::as_str));
                condition
                    .matches_principal(organ, target, proximity)
                    .then_some(index)
            })
            .collect();
        Decision {
            allowed: !included_by.is_empty() && excluded_by.is_empty(),
            included_by,
            excluded_by,
            radius_metres: radius_metres.filter(|radius| *radius > 0),
        }
    }
}
