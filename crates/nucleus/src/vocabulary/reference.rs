use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeaningReference {
    pub vocabulary_uid: String,
    pub concept_uid: String,
    pub revision: i64,
    pub definition_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversionReference {
    pub from: MeaningReference,
    pub to: MeaningReference,
    pub revision: i64,
    pub numerator: String,
    pub denominator: String,
    pub withdrawn: bool,
    pub hash: String,
}

impl ConversionReference {
    pub fn new(
        from: MeaningReference,
        to: MeaningReference,
        revision: i64,
        numerator: i128,
        denominator: i128,
        withdrawn: bool,
    ) -> Result<Self, &'static str> {
        from.validate()?;
        to.validate()?;
        if revision < 1 || numerator <= 0 || denominator <= 0 || from.concept_uid == to.concept_uid
        {
            return Err("Choose a saved positive conversion between distinct units");
        }
        let mut reference = Self {
            from,
            to,
            revision,
            numerator: numerator.to_string(),
            denominator: denominator.to_string(),
            withdrawn,
            hash: String::new(),
        };
        reference.hash = reference.digest()?;
        Ok(reference)
    }

    fn digest(&self) -> Result<String, &'static str> {
        let bytes = serde_json::to_vec(&(
            "lince.meaning.conversion.v1",
            &self.from,
            &self.to,
            self.revision,
            &self.numerator,
            &self.denominator,
            self.withdrawn,
        ))
        .map_err(|_| "Cannot encode conversion reference")?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let ratio = |value: &str| {
            if value.len() > 39
                || value.starts_with('0')
                || !value.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            value.parse::<i128>().ok().filter(|value| *value > 0)
        };
        let numerator = ratio(&self.numerator).ok_or("Invalid conversion numerator")?;
        let denominator = ratio(&self.denominator).ok_or("Invalid conversion denominator")?;
        let expected = Self::new(
            self.from.clone(),
            self.to.clone(),
            self.revision,
            numerator,
            denominator,
            self.withdrawn,
        )?;
        if expected.hash != self.hash {
            return Err("Conversion reference hash does not match its versioned terms");
        }
        Ok(())
    }
}

impl MeaningReference {
    pub fn from_definition(
        vocabulary_uid: &str,
        definition: &super::DefinitionVersion,
    ) -> Result<Self, &'static str> {
        if definition.definition.len() > 16384 {
            return Err("Meaning definition exceeds 16384 bytes");
        }
        let bytes = serde_json::to_vec(&(
            "lince.meaning.definition.v1",
            vocabulary_uid,
            &definition.concept_uid,
            definition.revision,
            &definition.definition,
        ))
        .map_err(|_| "Cannot encode meaning definition")?;
        let reference = Self {
            vocabulary_uid: vocabulary_uid.into(),
            concept_uid: definition.concept_uid.clone(),
            revision: definition.revision,
            definition_hash: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::valid_uid(&self.vocabulary_uid, "g")
            || !crate::valid_uid(&self.concept_uid, "c")
            || self.revision < 1
            || self.definition_hash.len() != 64
            || !self
                .definition_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("Choose a versioned meaning with a canonical SHA-256 definition hash");
        }
        Ok(())
    }

    pub fn matches_definition(&self, definition: &super::DefinitionVersion) -> bool {
        Self::from_definition(&self.vocabulary_uid, definition).as_ref() == Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_bind_meaning_revision_and_vocabulary_without_timestamp_dependence() {
        let mut definition = crate::vocabulary::DefinitionVersion {
            concept_uid: crate::new_uid("c"),
            revision: 1,
            definition: "A measured portion of food".into(),
            created_at: "2026-10-15".into(),
        };
        let vocabulary = crate::new_uid("g");
        let reference = MeaningReference::from_definition(&vocabulary, &definition).unwrap();
        definition.created_at = "another local import time".into();
        assert!(reference.matches_definition(&definition));
        definition.definition.push_str(" and water");
        assert!(!reference.matches_definition(&definition));
        definition.definition = "A measured portion of food".into();
        definition.revision = 2;
        assert!(!reference.matches_definition(&definition));
        definition.revision = 1;
        assert_ne!(
            reference,
            MeaningReference::from_definition(&crate::new_uid("g"), &definition).unwrap()
        );
    }
}
