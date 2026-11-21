// non-lean-yet
// Scope: enum CapabilityFamily, enum Capability, struct CapabilitySet.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 6, 40.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 8,
//   10, 12, 14, 16, 18, 20, 22, 24, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62, 64, 66, 68, 70,
//   72, 74, 76, 78, 80, 82, 84, 86, 88, 90, 92, 94, 96, 98, 100, 102, 104, 106, 108, 110, 112, 114,
//   118, 162, 165, 169, 173, 177, 181.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CapabilityFamily {
    #[serde(rename = "read-analyze")]
    ReadAnalyze,
    #[serde(rename = "local-reversible-data")]
    LocalReversibleData,
    #[serde(rename = "attention-presentation")]
    AttentionPresentation,
    #[serde(rename = "program-meta-control")]
    ProgramMetaControl,
    #[serde(rename = "external-resource")]
    ExternalResource,
    #[serde(rename = "transfer-preparation")]
    TransferPreparation,
    #[serde(rename = "transfer-social")]
    TransferSocial,
    #[serde(rename = "transfer-commitment")]
    TransferCommitment,
    #[serde(rename = "irreversible-safety-critical")]
    IrreversibleSafetyCritical,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub enum Capability {
    #[serde(rename = "karma.read")]
    KarmaRead,
    #[serde(rename = "karma.evaluate")]
    KarmaEvaluate,
    #[serde(rename = "karma.author")]
    KarmaAuthor,
    #[serde(rename = "karma.activate")]
    KarmaActivate,
    #[serde(rename = "karma.run")]
    KarmaRun,
    #[serde(rename = "karma.manage")]
    KarmaManage,
    #[serde(rename = "record.set_quantity")]
    RecordSetQuantity,
    #[serde(rename = "record.add_quantity")]
    RecordAddQuantity,
    #[serde(rename = "link.create")]
    LinkCreate,
    #[serde(rename = "metadata.write")]
    MetadataWrite,
    #[serde(rename = "task.create")]
    TaskCreate,
    #[serde(rename = "attention.decide")]
    AttentionDecide,
    #[serde(rename = "attention.notify")]
    AttentionNotify,
    #[serde(rename = "interface.control")]
    InterfaceControl,
    #[serde(rename = "external.http")]
    ExternalHttp,
    #[serde(rename = "external.command")]
    ExternalCommand,
    #[serde(rename = "external.filesystem")]
    ExternalFilesystem,
    #[serde(rename = "external.network")]
    ExternalNetwork,
    #[serde(rename = "external.payment")]
    ExternalPayment,
    #[serde(rename = "device.control")]
    DeviceControl,
    #[serde(rename = "transfer.read")]
    TransferRead,
    #[serde(rename = "transfer.project")]
    TransferProject,
    #[serde(rename = "transfer.draft_local")]
    TransferDraftLocal,
    #[serde(rename = "transfer.publish")]
    TransferPublish,
    #[serde(rename = "transfer.propose")]
    TransferPropose,
    #[serde(rename = "transfer.negotiate_own")]
    TransferNegotiateOwn,
    #[serde(rename = "transfer.revise_own")]
    TransferReviseOwn,
    #[serde(rename = "transfer.agree_own")]
    TransferAgreeOwn,
    #[serde(rename = "transfer.activate_own")]
    TransferActivateOwn,
    #[serde(rename = "transfer.claim_occurrence_own")]
    TransferClaimOccurrenceOwn,
    #[serde(rename = "transfer.confirm_own")]
    TransferConfirmOwn,
    #[serde(rename = "transfer.settle_local")]
    TransferSettleLocal,
    #[serde(rename = "transfer.withdraw_own")]
    TransferWithdrawOwn,
    #[serde(rename = "transfer.cancel_own")]
    TransferCancelOwn,
    #[serde(rename = "transfer.dispute_own")]
    TransferDisputeOwn,
    #[serde(rename = "transfer.correct_own")]
    TransferCorrectOwn,
    #[serde(rename = "transfer.declassify")]
    TransferDeclassify,
}

impl Capability {
    pub const fn family(self) -> CapabilityFamily {
        match self {
            Self::KarmaRead | Self::KarmaEvaluate | Self::TransferRead | Self::TransferProject => {
                CapabilityFamily::ReadAnalyze
            }
            Self::RecordSetQuantity
            | Self::RecordAddQuantity
            | Self::LinkCreate
            | Self::MetadataWrite
            | Self::TaskCreate => CapabilityFamily::LocalReversibleData,
            Self::AttentionDecide | Self::AttentionNotify | Self::InterfaceControl => {
                CapabilityFamily::AttentionPresentation
            }
            Self::KarmaAuthor | Self::KarmaActivate | Self::KarmaRun | Self::KarmaManage => {
                CapabilityFamily::ProgramMetaControl
            }
            Self::ExternalHttp
            | Self::ExternalCommand
            | Self::ExternalFilesystem
            | Self::ExternalNetwork
            | Self::ExternalPayment => CapabilityFamily::ExternalResource,
            Self::TransferDraftLocal => CapabilityFamily::TransferPreparation,
            Self::TransferPublish
            | Self::TransferPropose
            | Self::TransferNegotiateOwn
            | Self::TransferReviseOwn
            | Self::TransferWithdrawOwn
            | Self::TransferCancelOwn
            | Self::TransferDisputeOwn
            | Self::TransferCorrectOwn
            | Self::TransferDeclassify => CapabilityFamily::TransferSocial,
            Self::TransferAgreeOwn
            | Self::TransferActivateOwn
            | Self::TransferClaimOccurrenceOwn
            | Self::TransferConfirmOwn
            | Self::TransferSettleLocal => CapabilityFamily::TransferCommitment,
            Self::DeviceControl => CapabilityFamily::IrreversibleSafetyCritical,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
#[derive(schemars::JsonSchema)]
pub struct CapabilitySet(BTreeSet<Capability>);

impl CapabilitySet {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self(capabilities.into_iter().collect())
    }

    pub fn contains(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_set(&self) -> &BTreeSet<Capability> {
        &self.0
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = Capability> + '_ {
        self.0.iter().copied()
    }
}
