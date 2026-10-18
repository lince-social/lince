use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_DURATION_SECONDS: u32 = 3600;
pub const MAX_DURATION_SECONDS: u32 = 86400;
pub const MAX_RECIPIENTS: usize = 32;
pub const MAX_SESSIONS: usize = 64;
pub const MAX_FIX_AGE_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub record_uid: String,
    pub authority_node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<InvitationGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InvitationGrant {
    pub uid: String,
    pub issuer: String,
    pub recipient: String,
    pub expires_at_ms: i64,
    pub radius_metres: Option<u32>,
}

impl Invitation {
    pub fn reference(&self) -> Result<String, &'static str> {
        self.validate()?;
        serde_json::to_string(self)
            .map(|value| format!("lince-location:1:{value}"))
            .map_err(|_| "Cannot encode the location reference")
    }

    pub fn parse(reference: &str) -> Result<Self, &'static str> {
        if reference.len() > 1024 {
            return Err("Location reference exceeds its limit");
        }
        let raw = reference
            .trim()
            .strip_prefix("lince-location:1:")
            .ok_or("Use a Lince location reference")?;
        let value: Self = serde_json::from_str(raw).map_err(|_| "Invalid location reference")?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::valid_uid(&self.record_uid, "r")
            || self.authority_node_id.len() != 64
            || !self
                .authority_node_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid location Record or authority endpoint");
        }
        if let Some(grant) = &self.grant {
            if !crate::valid_uid(&grant.uid,"li") || !crate::valid_uid(&grant.issuer,"r") || !crate::valid_uid(&grant.recipient,"r") || chrono::DateTime::from_timestamp_millis(grant.expires_at_ms).is_none() || grant.radius_metres.is_some_and(|radius|!crate::place::valid_area_radius(radius)) {
                return Err("Invalid private location invitation scope");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Device,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub record_uid: String,
    pub controller_uid: String,
    pub source_cell_uid: String,
    pub source_node_id: String,
    pub source_kind: SourceKind,
    pub duration_seconds: u32,
    pub recipients: Vec<String>,
    pub transfer_uid: Option<String>,
}

impl Settings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::valid_uid(&self.record_uid, "r")
            || !crate::valid_uid(&self.controller_uid, "r")
            || !crate::valid_uid(&self.source_cell_uid, "r")
            || self
                .transfer_uid
                .as_deref()
                .is_some_and(|uid| !crate::valid_uid(uid, "r"))
        {
            return Err("Choose valid Record, Person, device, and Transfer identities");
        }
        if self.source_node_id.len() != 64
            || !self
                .source_node_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Choose an authenticated device endpoint");
        }
        if !(60..=MAX_DURATION_SECONDS).contains(&self.duration_seconds) {
            return Err("Location duration must be between one minute and one day");
        }
        if self.recipients.len() > MAX_RECIPIENTS {
            return Err("Choose at most 32 location recipients");
        }
        let mut seen = std::collections::HashSet::new();
        if self
            .recipients
            .iter()
            .any(|uid| !crate::valid_uid(uid, "r") || !seen.insert(uid))
        {
            return Err("Choose distinct, valid location recipients");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fix {
    pub sequence: u64,
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_metres: Option<f64>,
    pub captured_at_ms: i64,
    pub capture_age_ms: u64,
}

impl Fix {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sequence == 0 || self.sequence > i64::MAX as u64 {
            return Err("Invalid location sequence");
        }
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
            || self
                .accuracy_metres
                .is_some_and(|value| !value.is_finite() || !(0.0..=100_000.0).contains(&value))
        {
            return Err("Invalid location coordinates or accuracy");
        }
        if self.capture_age_ms >= MAX_FIX_AGE_MS as u64 {
            return Err("Location capture age exceeds the freshness limit");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Context {
        person: String,
        record_uid: String,
    },
    Configure {
        settings: Settings,
    },
    Handover {
        settings: Settings,
        expected_session_uid: String,
    },
    PrepareInvitation {
        person: String,
        record_uid: String,
        recipient: String,
        duration_seconds: u32,
        radius_metres: Option<u32>,
    },
    ObserveInvitation {
        person: String,
        invitation: Invitation,
    },
    ReadInvitation {
        person: String,
        invitation: Invitation,
    },
    SendInvitation {
        person: String,
        conversation: String,
        invitation: Invitation,
    },
    Start {
        person: String,
        record_uid: String,
    },
    Approve {
        person: String,
        record_uid: String,
        expected_session_uid: String,
    },
    View {
        person: String,
        record_uid: String,
    },
    Observe {
        person: String,
        record_uid: String,
        node_id: String,
    },
    Stop {
        person: String,
        record_uid: String,
    },
    StopAll {
        person: String,
    },
    StopTransfer {
        person: String,
        transfer_uid: String,
    },
    Publish {
        person: String,
        record_uid: String,
        session_uid: String,
        fix: Fix,
    },
}

impl Command {
    pub fn person(&self) -> &str {
        match self {
            Self::Configure { settings } | Self::Handover { settings, .. } => &settings.controller_uid,
            Self::Context { person, .. }
            | Self::Start { person, .. }
            | Self::Approve { person, .. }
            | Self::View { person, .. }
            | Self::Observe { person, .. }
            | Self::Stop { person, .. }
            | Self::StopAll { person }
            | Self::StopTransfer { person, .. }
            | Self::Publish { person, .. } => person,
            Self::PrepareInvitation {person,..} | Self::ObserveInvitation {person,..} | Self::ReadInvitation {person,..} | Self::SendInvitation {person,..} => person,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    pub uid: String,
    pub label: String,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    AwaitingApproval,
    Acquiring,
    Live,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct View {
    pub record_uid: String,
    pub status: Status,
    pub session_uid: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub source_kind: Option<SourceKind>,
    pub fix: Option<Fix>,
    pub age_ms: Option<u64>,
    pub remaining_millis: Option<u64>,
    pub next_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_metres: Option<u32>,
}

impl View {
    pub fn elapse(&mut self, millis: u64) {
        if let Some(remaining) = &mut self.remaining_millis {*remaining = remaining.saturating_sub(millis);}
        if let Some(age) = &mut self.age_ms {*age = age.saturating_add(millis);}
        if self.remaining_millis == Some(0) {
            self.status = Status::Stopped;
            self.session_uid = None;
            self.next_sequence = None;
            self.fix = None;
            self.age_ms = None;
            self.radius_metres = None;
        } else if self.age_ms.is_some_and(|age|age >= MAX_FIX_AGE_MS as u64) {
            self.status = Status::Unavailable;
            self.fix = None;
            self.age_ms = None;
            self.radius_metres = None;
        } else if self.age_ms.is_some_and(|age|age > 15_000) && self.status == Status::Live {self.status = Status::Stale;}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Context {
    pub authority_node_id: String,
    pub settings: Option<Settings>,
    pub people: Vec<Choice>,
    pub devices: Vec<Choice>,
    pub current_cell_uid: String,
    pub current_node_id: String,
    pub view: View,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLease {
    pub settings: Settings,
    pub authority_node_id: String,
    pub session_uid: String,
    pub expires_at_ms: i64,
    pub approved: bool,
    pub challenge_uid: String,
    pub lease_millis: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PeerRequest {
    Visibility {
        command: crate::visibility::Command,
        person: Option<String>,
    },
    Read {
        record_uid: String,
        data: crate::visibility::Data,
    },
    ReadPerson {
        record_uid: String,
        data: crate::visibility::Data,
        person: String,
    },
    Control {
        command: Command,
    },
    RequestSource {
        lease: SourceLease,
    },
    ApproveSource {
        person: String,
        record_uid: String,
        session_uid: String,
    },
    SourceLease {
        record_uid: String,
        session_uid: String,
    },
    StopSource {
        record_uid: String,
        session_uid: String,
    },
    Publish {
        record_uid: String,
        session_uid: String,
        challenge_uid: String,
        fix: Fix,
    },
    Unavailable {
        record_uid: String,
        session_uid: String,
    },
}
