//! Delivery and receipt history in the original event order.

use nucleus::transfer::OccurrenceClaimRole::{Delivery, Receipt};
use serde_json::{Value, json};
use store::transfers::OccurrenceClaimEventRow;

#[derive(Default)]
pub(super) struct Claims {
    pub all: Vec<Value>,
    pub delivery: Vec<Value>,
    pub receipt: Vec<Value>,
}

pub(super) fn project(events: &[OccurrenceClaimEventRow]) -> Claims {
    let mut claims = Claims {
        all: Vec::with_capacity(events.len()),
        ..Claims::default()
    };
    for event in events {
        let fields = [
            ("claimed", json!(event.asserted)),
            ("person", json!(event.actor_person_uid)),
            ("fact", json!(event.fact_uid)),
            ("request_id", json!(event.idempotency_key)),
            ("at", json!(event.created_at)),
        ];
        claims.all.push(Value::from_iter(
            [
                ("uid", json!(event.uid)),
                ("role", json!(event.role.as_str())),
            ]
            .into_iter()
            .chain(fields.iter().cloned()),
        ));
        let history = match event.role {
            Delivery => &mut claims.delivery,
            Receipt => &mut claims.receipt,
        };
        history.push(Value::from_iter(
            [("uid", json!(event.uid))].into_iter().chain(fields),
        ));
    }
    claims
}
