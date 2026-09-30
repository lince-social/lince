use std::{cell::RefCell, collections::BTreeMap, future::Future};

use nucleus::DecimalValue;
use nucleus::transfer::karma::Snapshot;

use crate::{Engine, EngineError};

#[derive(Default)]
pub(crate) struct Readings {
    actor: Option<String>,
    pub(crate) snapshots: BTreeMap<String, Snapshot>,
    pub(crate) guards: BTreeMap<String, nucleus::transfer::karma::Guard>,
}

tokio::task_local! {
    pub(crate) static READINGS: RefCell<Readings>;
}

pub(crate) async fn scope<F: Future>(actor: Option<&str>, future: F) -> F::Output {
    if READINGS
        .try_with(|readings| readings.borrow().actor.as_deref() == actor)
        .unwrap_or(false)
    {
        future.await
    } else {
        READINGS
            .scope(
                RefCell::new(Readings {
                    actor: actor.map(str::to_owned),
                    snapshots: BTreeMap::new(),
                    guards: BTreeMap::new(),
                }),
                future,
            )
            .await
    }
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_transfer_reading_invalid",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn resolve_karma_transfer(&self, token: &str) -> Result<String, EngineError> {
        let token = token.trim_start_matches('@');
        if let Some(record) = store::records::resolve(&self.store.pool, token).await? {
            return if record.kind == "transfer" {
                Ok(record.uid)
            } else {
                Err(invalid("Choose a Transfer for this reading"))
            };
        }
        store::sqlx::query_scalar("SELECT transfer_uid FROM transfer_remote_reference WHERE (transfer_uid = ? OR uid = ?) AND state = 'active' AND projection IS NOT NULL AND recipient_organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL) ORDER BY last_transfer_revision DESC LIMIT 1")
            .bind(token).bind(token).fetch_optional(&self.store.pool).await?
            .ok_or_else(|| invalid("Transfer is unavailable or its delivery access was revoked"))
    }

    pub(crate) async fn transfer_karma_snapshot(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<Snapshot, EngineError> {
        self.require_permission(actor, "transfer:read").await?;
        let transfer = self.resolve_karma_transfer(token).await?;
        let signer = self.signer.lock().await.clone();
        let rows = protein::execute_for_with_signer(
            &self.store,
            &protein::Protein {
                source: protein::Source::Transfer,
                filter: vec![protein::Predicate::UidEq(transfer.clone())],
                fields: Some(vec!["uid".into(), "revision".into(), "karma_state".into()]),
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: Some(1),
            },
            actor,
            signer.as_ref().map(|signer| signer.actor_uid.as_str()),
        )
        .await?;
        let row = rows
            .into_iter()
            .find(|row| row["uid"] == transfer)
            .ok_or_else(|| invalid("Transfer is outside what this login may read"))?;
        let state: Snapshot = serde_json::from_value(row["karma_state"].clone()).map_err(|_| {
            invalid(
                "Transfer has no current authorized automation snapshot; receive a fresh delivery",
            )
        })?;
        state.validate().map_err(invalid)?;
        if state.transfer != transfer || row["revision"].as_u64() != Some(state.revision) {
            return Err(invalid(
                "Transfer snapshot does not match its visible terms",
            ));
        }
        Ok(state)
    }

    pub(crate) async fn cached_transfer_snapshot(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<Snapshot, EngineError> {
        self.require_permission(actor, "transfer:read").await?;
        if let Ok(Some(state)) = READINGS.try_with(|readings| {
            let readings = readings.borrow();
            (readings.actor.as_deref() == actor)
                .then(|| {
                    readings
                        .snapshots
                        .get(token.trim_start_matches('@'))
                        .cloned()
                })
                .flatten()
        }) {
            return Ok(state);
        }
        let transfer = self.resolve_karma_transfer(token).await?;
        if let Ok(Some(state)) =
            READINGS.try_with(|readings| readings.borrow().snapshots.get(&transfer).cloned())
        {
            return Ok(state);
        }
        let state = self.transfer_karma_snapshot(&transfer, actor).await?;
        let _ = READINGS.try_with(|readings| {
            readings
                .borrow_mut()
                .snapshots
                .insert(transfer, state.clone())
        });
        Ok(state)
    }

    pub(crate) async fn transfer_condition_reading(
        &self,
        function: &str,
        references: &str,
        now_ms: i64,
    ) -> Result<DecimalValue, EngineError> {
        let actor = READINGS
            .try_with(|readings| readings.borrow().actor.clone())
            .unwrap_or(None);
        let names: Vec<_> = references.split('|').collect();
        let state = self
            .cached_transfer_snapshot(names[0], actor.as_deref())
            .await?;
        let integer = |value: i128| DecimalValue::from_mantissa(0, value).map_err(invalid);
        let mut observed_person = None;
        let result = match function {
            "transfer_revision" => integer(i128::from(state.revision)),
            "transfer_active" => integer(i128::from(state.active)),
            "transfer_published" => integer(i128::from(state.published)),
            "transfer_ready" => integer(i128::from(state.ready)),
            name if nucleus::transfer::karma::is_agreement_reading(name) => {
                let person = names
                    .get(1)
                    .ok_or_else(|| invalid("Agreement readings need a Transfer and a Person"))?;
                let person = store::records::resolve(&self.store.pool, person)
                    .await?
                    .filter(|record| record.kind == "person")
                    .ok_or_else(|| invalid("Agreement Person is unavailable"))?;
                observed_person = Some(person.uid.clone());
                let agreement = state
                    .participants
                    .get(&person.uid)
                    .ok_or_else(|| invalid("This Person's agreement is unavailable or hidden"))?;
                if name == "agreement_level" {
                    integer(i128::from(agreement.guard.level))
                } else {
                    let changed = agreement
                        .changed_at_ms
                        .ok_or_else(|| invalid("No agreement change time is available"))?;
                    if name == "agreement_changed_at" {
                        integer(i128::from(changed))
                    } else {
                        DecimalValue::from_mantissa(3, i128::from(now_ms) - i128::from(changed))
                            .map_err(invalid)
                    }
                }
            }
            _ => Err(invalid("Unknown Transfer reading")),
        };
        if result.is_ok() {
            let _ = READINGS.try_with(|readings| {
                readings
                    .borrow_mut()
                    .guards
                    .entry(state.transfer.clone())
                    .or_insert_with(|| nucleus::transfer::karma::Guard::base(&state))
                    .observe(&state, function, observed_person.as_deref())
            });
        }
        result
    }

    pub(crate) async fn refuse_unreadable_karma_inputs(
        &self,
        actor: Option<&str>,
        inputs: &[String],
    ) -> Result<(), EngineError> {
        for input in inputs {
            if self.resolve_karma_transfer(input).await.is_ok() {
                self.cached_transfer_snapshot(input, actor).await?;
            } else {
                self.refuse_unreadable(actor, &[input.clone()]).await?;
            }
        }
        Ok(())
    }
}
