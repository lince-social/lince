//! Program activation revisions frozen for an occurrence.

use serde::{Deserialize, Serialize};

use super::validate_program_uid;
use crate::{CanonicalHash, KarmaBoundaryError, canonical_hash};

pub const OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN: &str = "karma.occurrence-program-epoch.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OccurrenceProgramEpochSchema {
    #[serde(rename = "karma.occurrence-program-epoch.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProgramEpochMember {
    pub program_uid: String,
    pub revision_hash: CanonicalHash,
    pub activation_handle_revision: u64,
}

impl ProgramEpochMember {
    pub fn new(
        program_uid: impl Into<String>,
        revision_hash: CanonicalHash,
        activation_handle_revision: u64,
    ) -> Result<Self, KarmaBoundaryError> {
        let member = Self {
            program_uid: program_uid.into(),
            revision_hash,
            activation_handle_revision,
        };
        member.validate()?;
        Ok(member)
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        validate_program_uid(&self.program_uid)?;
        if self.activation_handle_revision == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "Program epoch activation handle revision must be positive",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceProgramEpoch {
    pub schema: OccurrenceProgramEpochSchema,
    pub occurrence_hash: CanonicalHash,
    pub cell_sequence: u64,
    pub members: Vec<ProgramEpochMember>,
}

impl OccurrenceProgramEpoch {
    pub fn new(
        occurrence_hash: CanonicalHash,
        cell_sequence: u64,
        members: Vec<ProgramEpochMember>,
    ) -> Result<Self, KarmaBoundaryError> {
        let epoch = Self {
            schema: OccurrenceProgramEpochSchema::V1,
            occurrence_hash,
            cell_sequence,
            members,
        };
        epoch.validate()?;
        Ok(epoch)
    }

    pub fn epoch_hash(&self) -> Result<CanonicalHash, KarmaBoundaryError> {
        self.validate()?;
        canonical_hash(OCCURRENCE_PROGRAM_EPOCH_HASH_DOMAIN, self)
    }

    pub fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.schema != OccurrenceProgramEpochSchema::V1 || self.cell_sequence == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "occurrence Program epoch needs a supported schema and positive Cell sequence",
            ));
        }
        let mut previous: Option<&ProgramEpochMember> = None;
        for member in &self.members {
            member.validate()?;
            if previous.is_some_and(|value| value >= member) {
                return Err(KarmaBoundaryError::invalid_input(
                    "occurrence Program epoch members must be unique and strictly sorted",
                ));
            }
            if previous.is_some_and(|value| value.program_uid == member.program_uid) {
                return Err(KarmaBoundaryError::invalid_input(
                    "occurrence Program epoch cannot contain two revisions of one Program",
                ));
            }
            previous = Some(member);
        }
        Ok(())
    }
}
