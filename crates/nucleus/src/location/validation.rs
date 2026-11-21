use std::collections::HashSet;

use chrono::DateTime;
use serde_json::{from_str, to_string};

use crate::{place::valid_area_radius, valid_uid};

use super::{Fix, Invitation, MAX_DURATION_SECONDS, MAX_FIX_AGE_MS, MAX_RECIPIENTS, Settings};

const REFERENCE_PREFIX: &str = "lince-location:1:";
const MAX_REFERENCE_BYTES: usize = 1024;
const NODE_ID_BYTES: usize = 64;

fn valid_node_id(id: &str) -> bool {
    id.len() == NODE_ID_BYTES && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Invitation {
    /// Encodes a validated invitation with its versioned reference prefix.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid invitation or a serialization failure.
    pub fn reference(&self) -> Result<String, &'static str> {
        self.validate()?;
        to_string(self)
            .map(|value| format!("{REFERENCE_PREFIX}{value}"))
            .map_err(|_| "Cannot encode the location reference")
    }

    /// Decodes a versioned invitation and checks its private grant, when present.
    ///
    /// # Errors
    ///
    /// Returns an error for an oversized, malformed, or invalid reference.
    pub fn parse(reference: &str) -> Result<Self, &'static str> {
        if reference.len() > MAX_REFERENCE_BYTES {
            return Err("Location reference exceeds its limit");
        }
        let raw = reference
            .trim()
            .strip_prefix(REFERENCE_PREFIX)
            .ok_or("Use a Lince location reference")?;
        let value: Self = from_str(raw).map_err(|_| "Invalid location reference")?;
        value.validate()?;
        Ok(value)
    }

    /// Checks the endpoint and identity scopes encoded in this invitation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid Record, endpoint, or grant scope.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_uid(&self.record_uid, "r") || !valid_node_id(&self.authority_node_id) {
            return Err("Invalid location Record or authority endpoint");
        }
        if let Some(grant) = &self.grant
            && (!valid_uid(&grant.uid, "li")
                || !valid_uid(&grant.issuer, "r")
                || !valid_uid(&grant.recipient, "r")
                || DateTime::from_timestamp_millis(grant.expires_at_ms).is_none()
                || grant
                    .radius_metres
                    .is_some_and(|radius| !valid_area_radius(radius)))
        {
            return Err("Invalid private location invitation scope");
        }
        Ok(())
    }
}

impl Settings {
    /// Checks the source, duration, and recipient settings of a session.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid identities, endpoint, duration, or recipient list.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_uid(&self.record_uid, "r")
            || !valid_uid(&self.controller_uid, "r")
            || !valid_uid(&self.source_cell_uid, "r")
            || self
                .transfer_uid
                .as_deref()
                .is_some_and(|uid| !valid_uid(uid, "r"))
        {
            return Err("Choose valid Record, Person, device, and Transfer identities");
        }
        if !valid_node_id(&self.source_node_id) {
            return Err("Choose an authenticated device endpoint");
        }
        if !(60..=MAX_DURATION_SECONDS).contains(&self.duration_seconds) {
            return Err("Location duration must be between one minute and one day");
        }
        if self.recipients.len() > MAX_RECIPIENTS {
            return Err("Choose at most 32 location recipients");
        }
        let mut seen = HashSet::with_capacity(self.recipients.len());
        if self
            .recipients
            .iter()
            .any(|uid| !valid_uid(uid, "r") || !seen.insert(uid))
        {
            return Err("Choose distinct, valid location recipients");
        }
        Ok(())
    }
}

impl Fix {
    /// Checks the persisted sequence, geographic ranges, and reported capture age.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid sequence, coordinates, accuracy, or a stale capture.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sequence == 0 || i64::try_from(self.sequence).is_err() {
            return Err("Invalid location sequence");
        }
        if !(-90.0..=90.0).contains(&self.latitude)
            || !(-180.0..=180.0).contains(&self.longitude)
            || self
                .accuracy_metres
                .is_some_and(|value| !(0.0..=100_000.0).contains(&value))
        {
            return Err("Invalid location coordinates or accuracy");
        }
        if self.capture_age_ms >= MAX_FIX_AGE_MS.unsigned_abs() {
            return Err("Location capture age exceeds the freshness limit");
        }
        Ok(())
    }
}
