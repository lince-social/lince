//! Authored item terms and the audience of each field.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::transfer::{
    exchange::ExchangeRoute,
    loans::{Interval, Reference},
    transport::Terms,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudienceScope {
    #[default]
    Everyone,
    Participants,
    Owner,
    Selected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FieldAudience {
    pub scope: AudienceScope,
    pub people: Vec<String>,
}

impl FieldAudience {
    /// Check a field's audience, including the current field owner's access.
    pub fn allows(&self, viewer: Option<&str>, owner: Option<&str>, participant: bool) -> bool {
        if viewer.is_some() && viewer == owner {
            return true;
        }
        match self.scope {
            AudienceScope::Everyone => true,
            AudienceScope::Participants => participant,
            AudienceScope::Owner => false,
            AudienceScope::Selected => {
                viewer.is_some_and(|uid| self.people.iter().any(|p| p == uid))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ItemDisclosure {
    pub title: FieldAudience,
    pub description: FieldAudience,
    pub source: FieldAudience,
    pub parties: FieldAudience,
    pub quantity: FieldAudience,
    pub location: FieldAudience,
}

impl Default for ItemDisclosure {
    fn default() -> Self {
        Self {
            title: FieldAudience::default(),
            description: FieldAudience::default(),
            source: FieldAudience {
                scope: AudienceScope::Owner,
                ..FieldAudience::default()
            },
            parties: FieldAudience::default(),
            quantity: FieldAudience::default(),
            location: FieldAudience::default(),
        }
    }
}

impl ItemDisclosure {
    /// Visit the six editable audience settings in field order.
    pub fn fields_mut(&mut self) -> [&mut FieldAudience; 6] {
        [
            &mut self.title,
            &mut self.description,
            &mut self.source,
            &mut self.parties,
            &mut self.quantity,
            &mut self.location,
        ]
    }

    /// Pair each item field name with its audience.
    pub fn fields(&self) -> [(&'static str, &FieldAudience); 6] {
        [
            ("title", &self.title),
            ("description", &self.description),
            ("source", &self.source),
            ("parties", &self.parties),
            ("quantity", &self.quantity),
            ("location", &self.location),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TransferItem {
    pub title: String,
    pub description: String,
    pub disclosure: ItemDisclosure,
    pub exchange: Option<ExchangeRoute>,
    pub loan: Option<Interval>,
    pub return_of: Option<Reference>,
    pub future_need_for: Option<Reference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Terms>,
}

impl TransferItem {
    /// Validate authored item text, linked terms, and audience limits.
    ///
    /// # Errors
    ///
    /// Returns the first invalid text, link, transport, or audience setting.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.title.trim().is_empty() || self.title.chars().count() > 500 {
            return Err("item title must contain 1 to 500 characters");
        }
        if self.description.chars().count() > 20_000 {
            return Err("item description must contain at most 20000 characters");
        }
        if let Some(exchange) = &self.exchange {
            exchange.validate()?;
        }
        if let Some(transport) = &self.transport {
            transport.validate()?;
        }
        if [
            self.loan.is_some(),
            self.return_of.is_some(),
            self.future_need_for.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count()
            > 1
        {
            return Err("choose a loan, a return or a future Need for this item");
        }
        if let Some(loan) = &self.loan {
            loan.bounds()?;
            if self.exchange.is_none() {
                return Err("a loan needs a lender and borrower");
            }
        }
        for link in [&self.return_of, &self.future_need_for]
            .into_iter()
            .flatten()
        {
            link.validate()?;
        }
        for (_, field) in self.disclosure.fields() {
            if field.people.len() > 100
                || field.people.iter().any(|p| p.is_empty() || p.len() > 512)
            {
                return Err("item field audience is invalid");
            }
            if field.scope != AudienceScope::Selected && !field.people.is_empty() {
                return Err("only a selected audience can list People");
            }
        }
        Ok(())
    }
}

/// Fields an individual viewer may see for one obligation.
#[derive(Debug, Clone, Copy)]
pub struct ItemAccess {
    pub title: bool,
    pub description: bool,
    pub source: bool,
    pub parties: bool,
    pub quantity: bool,
    pub location: bool,
}

impl ItemAccess {
    pub(super) const ALL: Self = Self {
        title: true,
        description: true,
        source: true,
        parties: true,
        quantity: true,
        location: true,
    };

    pub(super) fn intersect(self, other: &Self) -> Self {
        Self {
            title: self.title && other.title,
            description: self.description && other.description,
            source: self.source && other.source,
            parties: self.parties && other.parties,
            quantity: self.quantity && other.quantity,
            location: self.location && other.location,
        }
    }

    /// Compute the viewer's permissions for an item and its obligation.
    ///
    /// Exchange participants may act as owners for item fields. Private source
    /// access comes from the source field's audience or privileged access.
    pub fn for_item(
        item: Option<&TransferItem>,
        viewer: Option<&str>,
        owner: Option<&str>,
        participant: bool,
        privileged: bool,
    ) -> Self {
        let Some(item) = item.filter(|_| !privileged) else {
            return Self::ALL;
        };
        let obligation_owner = viewer
            .filter(|viewer| {
                item.exchange
                    .as_ref()
                    .is_some_and(|exchange| exchange.involves(viewer))
            })
            .or(owner);
        let disclosure = &item.disclosure;
        let allows = |field: &FieldAudience| field.allows(viewer, obligation_owner, participant);
        Self {
            title: allows(&disclosure.title),
            description: allows(&disclosure.description),
            source: disclosure.source.allows(viewer, None, participant),
            parties: allows(&disclosure.parties),
            quantity: allows(&disclosure.quantity),
            location: allows(&disclosure.location),
        }
    }

    /// Whether every item field is visible.
    pub fn complete(self) -> bool {
        self.title
            && self.description
            && self.source
            && self.parties
            && self.quantity
            && self.location
    }
}
