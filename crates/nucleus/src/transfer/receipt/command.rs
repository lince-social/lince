//! Receipt command wire types and the permissions required to run them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ApprovalRole, PersonApproval, ScopeRequest};
use crate::{
    karma::CanonicalHash,
    transfer::{receipt_delivery, receipt_source},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventSource {
    Snapshot {
        record: String,
    },
    Outcome {
        record: String,
    },
    Settlement {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    SettlementStatus {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Cancellation {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Compensation {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
    Dispute {
        fact: String,
        supersedes: Option<CanonicalHash>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventRequest {
    pub record: String,
    pub expected_version: u64,
    pub authority_record: String,
    pub source: EventSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ApprovePerson {
        record: String,
        expected_version: u64,
        approval: PersonApproval,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_transfer: Option<String>,
    },
    History {
        origin: String,
        occurrence: CanonicalHash,
        #[serde(default)]
        before: Option<String>,
    },
    InspectVersion {
        record: String,
        version: u64,
    },
    ExportVersion {
        record: String,
        version: u64,
    },
    Versions {
        record: String,
        #[serde(default)]
        before: Option<String>,
    },
    Proof {
        record: String,
        version: u64,
        #[serde(default)]
        before: Option<String>,
    },
    Delivery {
        request: receipt_delivery::Command,
    },
    RecordSource {
        request: receipt_source::SourceRequest,
    },
    InspectSource {
        record: String,
    },
    SourceOptions {
        receipt: String,
    },
    IssueAuthority {
        cell: String,
    },
    PrepareParticipation {
        transfer: String,
        expected_revision: u64,
        party: Option<String>,
    },
    AuthorizeParticipation {
        cell: String,
        request: String,
    },
    Prepare {
        scope: ScopeRequest,
    },
    Import {
        document: String,
    },
    PreviewEvent {
        request: EventRequest,
    },
    PublishEvent {
        request: EventRequest,
        reviewed_at: i64,
        expected_projection: CanonicalHash,
    },
    Approve {
        record: String,
        expected_version: u64,
        role: ApprovalRole,
        authority_record: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_transfer: Option<String>,
    },
    Inspect {
        record: String,
    },
    Export {
        record: String,
    },
    List,
}

impl Command {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::ApprovePerson { .. } => "record:update",
            Self::Delivery { request } => request.permission(),
            Self::RecordSource { .. } => "transfer:update",
            Self::InspectSource { .. } | Self::SourceOptions { .. } => "record:read",
            Self::IssueAuthority { .. } | Self::AuthorizeParticipation { .. } => "organ:update",
            Self::Prepare { .. } | Self::PrepareParticipation { .. } => "transfer:read",
            Self::PreviewEvent { request }
                if matches!(request.source, EventSource::Snapshot { .. }) =>
            {
                "record:read"
            }
            Self::PreviewEvent { .. } => "transfer:read",
            Self::Import { .. } => "record:create",
            Self::Approve { .. } | Self::PublishEvent { .. } => "transfer:update",
            Self::Inspect { .. }
            | Self::Export { .. }
            | Self::List
            | Self::History { .. }
            | Self::InspectVersion { .. }
            | Self::ExportVersion { .. }
            | Self::Versions { .. }
            | Self::Proof { .. } => "record:read",
        }
    }

    pub fn is_mutation(&self) -> bool {
        if let Self::Delivery { request } = self {
            return request.is_mutation();
        }
        !matches!(
            self,
            Self::Inspect { .. }
                | Self::InspectVersion { .. }
                | Self::ExportVersion { .. }
                | Self::Versions { .. }
                | Self::Proof { .. }
                | Self::History { .. }
                | Self::Export { .. }
                | Self::List
                | Self::PrepareParticipation { .. }
                | Self::PreviewEvent { .. }
                | Self::InspectSource { .. }
                | Self::SourceOptions { .. }
        )
    }
}
