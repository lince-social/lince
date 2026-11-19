use super::{CanonicalHash, EngineError, Resolution, invalid};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(in super::super) struct Artifact {
    saved: store::transfer_receipts::Saved,
    scope: CanonicalHash,
    events: Vec<CanonicalHash>,
}

impl Artifact {
    pub(super) fn new(saved: store::transfer_receipts::Saved) -> Result<Self, EngineError> {
        let _span = nucleus::performance::Span::new("receipt-artifact-identities");
        let bundle = saved
            .document
            .public_bundle()
            .ok_or_else(|| invalid("Choose an original public proof artifact"))?;
        let scope = bundle.publication.scope.id().map_err(invalid)?;
        let events: Vec<_> = bundle
            .publication
            .events
            .iter()
            .map(|signed| signed.event.id().map_err(invalid))
            .collect::<Result<_, _>>()?;
        nucleus::performance::add(
            "receipt-artifact-identities-prepared",
            (events.len() + 1) as u64,
        );
        Ok(Self {
            saved,
            scope,
            events,
        })
    }

    pub(in super::super) fn saved(&self) -> &store::transfer_receipts::Saved {
        &self.saved
    }
    pub(in super::super) fn scope(&self) -> &CanonicalHash {
        &self.scope
    }
    pub(super) fn events(&self) -> &[CanonicalHash] {
        &self.events
    }

    pub(in super::super) fn project_history(
        &self,
        resolution: &Resolution,
    ) -> Result<(Vec<nucleus::transfer::receipt_history::Copy>, Vec<Value>), EngineError> {
        let _span = nucleus::performance::Span::new("receipt-artifact-history-projection");
        let saved = &self.saved;
        let bundle = saved.document.public_bundle().unwrap();
        let scope = &bundle.publication.scope;
        let scope_id = &self.scope;
        nucleus::performance::add(
            "receipt-artifact-identities-reused",
            (self.events.len() + 1) as u64,
        );
        let accepted = resolution
            .projections
            .iter()
            .map(|row| (&row.event, row))
            .collect::<BTreeMap<_, _>>();
        let withheld = resolution
            .unresolved
            .iter()
            .map(|row| (&row.event, row))
            .collect::<BTreeMap<_, _>>();
        let mut copies = Vec::new();
        let mut pending = Vec::new();
        for (signed, id) in bundle.publication.events.iter().zip(&self.events) {
            let event = &signed.event;
            if let Some(projection) = accepted.get(id) {
                copies.push(
                    nucleus::transfer::receipt_history::project_copy(
                        scope,
                        event,
                        projection,
                        nucleus::transfer::receipt_history::Provenance::Record {
                            record: saved.record.clone(),
                            version: saved.version,
                        },
                    )
                    .map_err(invalid)?,
                );
            } else if let Some(issue) = withheld.get(id) {
                pending.push(json!({"record":saved.record,"version":saved.version,"scope":scope_id,"event":id,"state":issue.state,"reason":issue.reason,"content":event.content,"issuer":scope.organ(event.content.role())}));
            } else {
                return Err(invalid(
                    "Selected proof event is absent from its resolution",
                ));
            }
        }
        Ok((copies, pending))
    }
}
