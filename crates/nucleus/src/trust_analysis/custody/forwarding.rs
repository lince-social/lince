use super::*;
use crate::transfer::receipt::Content;

pub const MAX_JOBS: usize = 8;
pub const MAX_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    pub peers: Filter,
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        self.peers.validate()?;
        if self.enabled && self.peers.include.is_empty() {
            return Err(
                "Explicitly include forwarding peers before enabling evidence gossip".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    Serving,
    Custody,
}

impl SourceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Serving => "serving",
            Self::Custody => "custody",
        }
    }
}

pub fn series(artifact: &Artifact) -> Result<CanonicalHash, String> {
    match artifact {
        Artifact::Contact(document) => canonical_hash(
            "lince.evidence-forwarding.contact.v1",
            &(&document.statement.issuer, document.statement.relation()?),
        ),
        Artifact::Claim(document) => canonical_hash(
            "lince.evidence-forwarding.claim.v1",
            &(&document.statement.issuer, &document.statement.claim),
        ),
        Artifact::Receipt(document) => {
            let bundle = document
                .public_bundle()
                .ok_or("Only public original receipts can be forwarded")?;
            canonical_hash(
                "lince.evidence-forwarding.receipt.v1",
                &(
                    &bundle.publication.scope.source.origin,
                    &bundle.publication.scope.source.occurrence,
                ),
            )
        }
    }
    .map_err(|error| error.to_string())
}

pub fn control(artifact: &Artifact) -> bool {
    match artifact {
        Artifact::Contact(document) => document.statement.withdrawn,
        Artifact::Claim(document) => document.statement.withdrawn,
        Artifact::Receipt(document) => document.public_bundle().is_some_and(|bundle| {
            bundle.publication.events.iter().any(|event| {
                matches!(
                    event.event.content,
                    Content::Cancelled { .. }
                        | Content::Dispute { .. }
                        | Content::Statement {
                            withdrawn: true,
                            ..
                        }
                ) || event.event.supersedes.is_some()
            })
        }),
    }
}
