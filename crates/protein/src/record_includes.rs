//! Optional Record data loaded for the selected rows and then moved into their output.

use std::collections::{HashMap, HashSet};

use nucleus::performance::Span;
use serde_json::{Value, json, to_string};
use store::{
    Store,
    facts::{Summary, summaries_for_records},
    misc::{PromiseRow, promises_for_records},
    organs::presentation::{Contact, for_records as contacts_for_records},
    records::{get_extensions, numeric_extensions_for_records},
    replica::{SharedConversation, conversations_with_many},
    sqlx::{Error::Protocol, query_as},
};

use crate::{Include, ProteinError, resolve_at};

#[derive(Default)]
pub(super) struct Prepared {
    facts: HashMap<String, Vec<Summary>>,
    promises: HashMap<String, Vec<PromiseRow>>,
    extensions: HashMap<String, Value>,
    numeric_extensions: HashMap<String, Vec<Value>>,
    reference_reads: HashMap<String, Vec<Value>>,
    contacts: HashMap<String, Contact>,
    conversations: HashMap<String, Vec<SharedConversation>>,
}

impl Prepared {
    pub(super) async fn load(
        store: &Store,
        records: &[&str],
        include: &Include,
    ) -> Result<Self, ProteinError> {
        let _includes = Span::new("protein-record-includes-await");
        let pool = &store.pool;
        let mut inputs = Self::default();
        if records.is_empty() {
            return Ok(inputs);
        }
        if let Some(facts) = &include.facts {
            inputs.facts = summaries_for_records(pool, records, facts.limit).await?;
        }
        if include.promises.is_some() {
            inputs.promises = promises_for_records(pool, records).await?;
        }
        if let Some(extension) = &include.extension {
            inputs.extensions = get_extensions(pool, records, &extension.namespace).await?;
        }
        if include.numeric_extensions {
            inputs.numeric_extensions = numeric_extensions_for_records(pool, records).await?;
        }
        if include.reference_reads {
            let selected = to_string(records).map_err(|error| Protocol(error.to_string()))?;
            let rows = query_as::<_, (String, String, i64, String, Option<String>)>(
                "SELECT rr.record_uid, rr.reader_organ, rr.reads, rr.last_read_at,
                        r.head AS reader_name
                 FROM reference_read rr
                 LEFT JOIN organ_contact c ON c.record_uid = rr.reader_organ
                 LEFT JOIN record r ON r.uid = c.record_uid
                 WHERE rr.record_uid IN (SELECT value FROM json_each(?))
                 ORDER BY rr.record_uid, rr.last_read_at DESC",
            )
            .bind(selected)
            .fetch_all(pool)
            .await?;
            for (uid, reader, reads, at, name) in rows {
                inputs
                    .reference_reads
                    .entry(uid)
                    .or_default()
                    .push(Value::from_iter([
                        ("reader_organ", Value::String(reader)),
                        ("reader_name", name.into()),
                        ("reads", reads.into()),
                        ("last_read_at", at.into()),
                    ]));
            }
        }
        if include.contact {
            inputs.contacts = contacts_for_records(pool, records).await?;
        }
        if include.conversations {
            inputs.conversations = conversations_with_many(pool, records).await?;
        }
        Ok(inputs)
    }

    pub(super) fn attach(
        &mut self,
        row: &mut Value,
        record_uid: &str,
        include: &Include,
        protected_transfers: &HashSet<String>,
    ) {
        let _publish = Span::new("protein-record-include-publish");
        if include.facts.is_some() {
            let list = self.facts.remove(record_uid).unwrap_or_default();
            let protected = protected_transfers.contains(record_uid);
            row["facts"] = Value::Array(
                list.into_iter()
                    .map(|f| {
                        Value::from_iter([
                            ("delta", Value::from(f.delta.to_f64())),
                            ("at", f.at.to_rfc3339().into()),
                            ("cause_kind", f.cause.kind.as_str().into()),
                            ("cause", if protected { None } else { f.cause.uid }.into()),
                            ("actor", if protected { None } else { f.actor_uid }.into()),
                        ])
                    })
                    .collect(),
            );
        }
        if let Some(promises) = &include.promises {
            let list = self.promises.remove(record_uid).unwrap_or_default();
            row["promises"] = Value::Array(
                list.into_iter()
                    .filter(|p| {
                        !p.transfer_uid
                            .as_ref()
                            .is_some_and(|uid| protected_transfers.contains(uid))
                            && (promises.state.is_empty()
                                || promises.state.iter().any(|s| s == p.state.as_str()))
                    })
                    .map(|p| {
                        Value::from_iter([
                            ("uid", Value::from(p.uid)),
                            ("delta", p.delta.into()),
                            ("state", p.state.as_str().into()),
                            ("window_end", p.window_end.into()),
                            ("party", p.party_uid.into()),
                        ])
                    })
                    .collect(),
            );
        }
        if include.extension.is_some() {
            row["extension"] = self.extensions.remove(record_uid).unwrap_or(Value::Null);
        }
        if include.numeric_extensions {
            row["numeric_extensions"] = Value::Array(
                self.numeric_extensions
                    .remove(record_uid)
                    .unwrap_or_default(),
            );
        }
        if include.reference_reads {
            row["reference_reads"] =
                Value::Array(self.reference_reads.remove(record_uid).unwrap_or_default());
        }
        if include.contact {
            row["contact"] = self
                .contacts
                .remove(record_uid)
                .map_or(Value::Null, contact_value);
        }
        if include.conversations {
            row["conversations"] = Value::Array(
                self.conversations
                    .remove(record_uid)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| {
                        Value::from_iter([
                            ("uid", Value::from(c.uid)),
                            ("head", c.head.into()),
                            ("state", c.state.into()),
                        ])
                    })
                    .collect(),
            );
        }
        if let Some(projection) = &include.projection
            && let Some(target) = resolve_at(&projection.at)
        {
            row["projected"] = json!({
                "at": target, "quantity": Value::Null, "status": "scenario_required",
                "reason": "Choose a Simulation scenario to inspect assumed future changes",
            });
        }
    }
}

fn contact_value(contact: Contact) -> Value {
    let access = contact.organ_access.map_or(Value::Null, |access| {
        Value::from_iter([
            ("organ_uid", Value::from(access.organ_uid)),
            ("generation", access.generation.into()),
            ("granted", access.granted.into()),
        ])
    });
    let delivery = contact.delivery_status;
    Value::from_iter([
        ("trust", contact.trust.into()),
        ("proximity", contact.proximity.into()),
        ("pending_introduction", contact.pending_introduction.into()),
        ("node_id", contact.node_id.into()),
        ("delivery", contact.delivery.as_str().into()),
        (
            "awaiting_roster_since",
            contact.awaiting_roster_since.into(),
        ),
        ("mailed_at", contact.mailed_at.into()),
        ("peer_acked_seq", contact.peer_acked_seq.into()),
        ("sync_out", contact.sync_out.into()),
        ("sync_in", contact.sync_in.into()),
        ("scope_fields", contact.scope_fields.into()),
        ("scope_version", contact.scope_version.into()),
        ("accept_fields", contact.accept_fields.into()),
        ("accept_version", contact.accept_version.into()),
        ("scope_unreadable", contact.scope_unreadable.into()),
        ("accept_unreadable", contact.accept_unreadable.into()),
        ("organ_access", access),
        (
            "delivery_status",
            Value::from_iter([
                ("saved_mail", Value::from(delivery.saved_mail)),
                ("mailbox_stored", delivery.mailbox_stored.into()),
                ("last_mailbox_storage", delivery.last_mailbox_storage.into()),
                ("queued_operations", delivery.queued_operations.into()),
            ]),
        ),
        (
            "quarantined",
            Value::Array(
                contact
                    .quarantined
                    .into_iter()
                    .map(|(reason, payload, at)| {
                        Value::from_iter([
                            ("reason", Value::from(reason)),
                            ("payload", payload.into()),
                            ("at", at.into()),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "hidden_records",
            Value::Array(
                contact
                    .hidden_records
                    .into_iter()
                    .map(|hidden| {
                        Value::from_iter([
                            ("uid", Value::from(hidden.uid)),
                            ("head", hidden.head.into()),
                            ("slug", hidden.slug.into()),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}
