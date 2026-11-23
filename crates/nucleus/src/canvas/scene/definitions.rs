//! Definition identity survives movement between documents. A placement pins its source,
//! logical ID and content revision. Import permission belongs to the publishing document.
use super::*;
use std::borrow::Cow;

pub const DEFINITION_DOMAIN: &str = "lince.workspace.definition.v1";
pub const MAX_REQUIRES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub document: DocumentId,
    pub part: PartId,
    pub revision: CanonicalHash,
}

impl Pin {
    pub fn valid(&self) -> bool {
        self.document.valid() && self.part.valid()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DefinitionKind {
    #[default]
    Part,
    Material {
        grade: String,
    },
    Unit {
        symbol: String,
    },
}

impl DefinitionKind {
    fn plain(&self) -> bool {
        matches!(self, Self::Part)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: PartId,
    pub name: String,
    #[serde(default, skip_serializing_if = "DefinitionKind::plain")]
    pub intrinsic: DefinitionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Pin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<DocumentId>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub local_only: bool,
}

impl Part {
    pub fn new(name: &str, intrinsic: DefinitionKind) -> Self {
        Self {
            id: PartId::new(),
            name: name.into(),
            intrinsic,
            requires: Vec::new(),
            source: None,
            local_only: false,
        }
    }

    /// Membership and disclosure policy travel in the catalogue. The same authored body keeps
    /// its digest across documents, including the original two-field Stage 002 format.
    pub fn content(&self) -> Cow<'_, Self> {
        if self.source.is_none() && !self.local_only {
            return Cow::Borrowed(self);
        }
        Cow::Owned(Self {
            source: None,
            local_only: false,
            ..self.clone()
        })
    }

    pub fn revision(&self) -> Result<CanonicalHash, String> {
        canonical_hash(DEFINITION_DOMAIN, self.content().as_ref()).map_err(|e| e.to_string())
    }

    pub fn pin(&self, owner: &DocumentId) -> Result<Pin, String> {
        Ok(Pin {
            document: self.source.as_ref().unwrap_or(owner).clone(),
            part: self.id.clone(),
            revision: self.revision()?,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let intrinsic_valid = match &self.intrinsic {
            DefinitionKind::Part => true,
            DefinitionKind::Material { grade } => valid_name(grade),
            DefinitionKind::Unit { symbol } => valid_name(symbol),
        };
        if !self.id.valid()
            || !valid_name(&self.name)
            || !intrinsic_valid
            || self.source.as_ref().is_some_and(|id| !id.valid())
        {
            return Err("Invalid definition identity, name or intrinsic value.".into());
        }
        if self.requires.len() > MAX_REQUIRES
            || self.requires.iter().any(|p| !p.valid())
            || self.requires.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("Use up to eight distinct required definitions in pin order.".into());
        }
        Ok(())
    }
}

impl Slot {
    pub fn pin(&self, owner: &DocumentId) -> Pin {
        Pin {
            document: self.source.as_ref().unwrap_or(owner).clone(),
            part: self.part.clone(),
            revision: self.revision.clone(),
        }
    }
}

impl Scene {
    pub fn definition(&self, pin: &Pin) -> Option<&Part> {
        self.definition_index(pin).map(|i| &self.parts[i])
    }

    fn definition_index(&self, pin: &Pin) -> Option<usize> {
        self.parts.iter().position(|part| {
            part.id == pin.part
                && part.source.as_ref().unwrap_or(&self.document) == &pin.document
                && part.revision().as_ref() == Ok(&pin.revision)
        })
    }

    pub fn place_definition(&mut self, pin: &Pin) -> Result<Handle, String> {
        let part = self
            .definition(pin)
            .ok_or("Missing pinned definition.")?
            .clone();
        self.insert_slot(&part)
    }

    /// Create a local revision for the selected occurrence. Sibling occurrences retain their
    /// original definition, local translation and Record binding.
    pub fn require_definition(&mut self, handle: &Handle, required: &Pin) -> Result<(), String> {
        let slot = self.slot(handle).ok_or("Select a placement.")?;
        if slot.source.is_some() {
            return Err("Select a locally authored placement to revise.".into());
        }
        let selected = slot.pin(&self.document);
        let mut next = self
            .definition(&selected)
            .ok_or("Missing selected definition.")?
            .clone();
        if self.definition(required).is_none() || selected == *required {
            return Err("Choose another available definition.".into());
        }
        let at = next
            .requires
            .binary_search(required)
            .err()
            .ok_or("This definition is already required.")?;
        next.requires.insert(at, required.clone());
        next.validate()?;
        let pin = next.pin(&self.document)?;
        if self.definition(&pin).is_none() {
            if self.parts.len() >= MAX_PARTS {
                return Err("The scene already has 64 definition revisions.".into());
            }
            Arc::make_mut(&mut self.parts).push(next);
        }
        let slot = Arc::make_mut(&mut self.slots)
            .iter_mut()
            .find(|s| s.id == handle.slot)
            .expect("selected slot was resolved");
        slot.revision = pin.revision;
        Ok(())
    }

    pub fn set_local_only(&mut self, pin: &Pin, local_only: bool) -> Result<(), String> {
        let index = self
            .definition_index(pin)
            .ok_or("Definition is unavailable.")?;
        if self.parts[index].local_only == local_only {
            return Ok(());
        }
        Arc::make_mut(&mut self.parts)[index].local_only = local_only;
        Ok(())
    }
}
