use std::collections::HashSet;

use chrono::{DateTime, Utc};
use nucleus::{Cause, NewFact, RecordKind};
use protein::Predicate;

use crate::{Engine, actions::ActionOutcome, error::EngineError};

impl Engine {
    pub(crate) async fn create_tagged_record(
        &self,
        head: String,
        body: String,
        quantity: f64,
        tags: Vec<String>,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if head.trim().is_empty()
            || head.len() > 500
            || body.len() > 40000
            || !quantity.is_finite()
            || tags.len() > 40
        {
            return Err(EngineError::Consequence(
                "Enter a title and at most 40 tags.".into(),
            ));
        }
        let mut resolved = Vec::new();
        let mut family = HashSet::new();
        for tag in tags {
            let uid = store::concepts::resolve(&self.store.pool, &tag)
                .await?
                .ok_or_else(|| EngineError::Consequence("Choose an existing tag.".into()))?;
            family.extend(store::concepts::ancestors_including(&self.store.pool, &uid).await?);
            if !resolved.contains(&uid) {
                resolved.push(uid);
            }
        }
        if let Some(person) = actor.as_deref() {
            if let Some(filter) =
                protein::read_rules::effective_predicate(&self.store, person).await?
            {
                if draft_matches(&filter, &family, &head, &body, quantity) != Some(true) {
                    return Err(EngineError::Forbidden(
                        "Choose tags that satisfy your role's access rules.".into(),
                    ));
                }
            }
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("The local Organ is unavailable.".into()))?;
        let uid = nucleus::new_uid("r");
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        store::records::create_with_uid_on(
            &mut tx,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: head.trim(),
                body: &body,
                quantity: store::exact::zero(),
            },
            &organ.uid,
            None,
        )
        .await?;
        store::assertions::transition_unary(&mut tx, &uid, &[], &resolved, actor.as_deref())
            .await?;
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            NewFact {
                actor_uid: actor,
                ..NewFact::quantity_f64(uid.clone(), quantity, Cause::user_edit())
            },
            now,
            signer.as_ref(),
        )
        .await?;
        tx.commit().await?;
        let facts = if let Some(fact) = fact {
            self.observe_committed_fact(fact, now).await?
        } else {
            vec![]
        };
        Ok(ActionOutcome {
            created: Some(uid),
            facts,
            ..Default::default()
        })
    }
}

fn draft_matches(
    predicate: &Predicate,
    tags: &HashSet<String>,
    head: &str,
    body: &str,
    quantity: f64,
) -> Option<bool> {
    Some(match predicate {
        Predicate::All(children) => children
            .iter()
            .map(|child| draft_matches(child, tags, head, body, quantity))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .all(|matches| matches),
        Predicate::Any(children) => children
            .iter()
            .map(|child| draft_matches(child, tags, head, body, quantity))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .any(|matches| matches),
        Predicate::Not(child) => !draft_matches(child, tags, head, body, quantity)?,
        Predicate::ConceptIn(uid) => tags.contains(uid),
        Predicate::KindEq(kind) => kind == "plain",
        Predicate::TextContains(text) => {
            head.to_lowercase().contains(&text.to_lowercase())
                || body.to_lowercase().contains(&text.to_lowercase())
        }
        Predicate::QuantityEq(value) => quantity == value.to_f64(),
        Predicate::QuantityLt(value) => quantity < value.to_f64(),
        Predicate::QuantityLte(value) => quantity <= value.to_f64(),
        Predicate::QuantityGt(value) => quantity > value.to_f64(),
        Predicate::QuantityGte(value) => quantity >= value.to_f64(),
        _ => return None,
    })
}
