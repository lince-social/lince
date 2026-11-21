//! Calculate private commitments and the stock left after reservations.

use nucleus::DecimalValue;
use sqlx::SqliteConnection;

use super::{
    Balance, Commitment,
    inputs::Scope,
    invalid,
    occurrence::{Occurrence, cancelled_on},
    promise::Promise,
    reserved_for,
};
use crate::{StoreError, exact, transfer_accounting as accounting, transfer_effects};

impl Scope<'_> {
    pub(super) async fn balance_on(
        &mut self,
        connection: &mut SqliteConnection,
        record: &str,
        actual: DecimalValue,
    ) -> Result<Balance, StoreError> {
        let selected = self.records.remove(record).ok_or(StoreError::RowNotFound)?;
        let mut incomplete = selected.incomplete;
        let mut commitments = Vec::new();
        for ((_, _, person), promise) in selected.promises {
            if !promise.has_commitment() {
                continue;
            }
            if promise.unavailable {
                incomplete.push(format!(
                    "{}: delivery access is unavailable",
                    promise.transfer
                ));
            }
            let paths = self.occurrences.paths(promise);
            let unactivated = paths.is_empty() && promise.state != "kept";
            for occurrence in unactivated
                .then_some(None)
                .into_iter()
                .chain(paths.into_values().map(Some))
            {
                match self
                    .commitment(connection, record, person, promise, occurrence)
                    .await
                {
                    Ok(Some(commitment)) => commitments.push(commitment),
                    Ok(None) => {}
                    Err(error) => incomplete.push(format!("{}: {error}", promise.transfer)),
                }
            }
        }
        let reserved = reserved_for(&commitments)?;
        let loan = self.loans.get(record);
        let loan_unit_changed = loan.is_some_and(|loan| loan.unit_changed);
        if loan_unit_changed {
            incomplete.push("Loan availability needs the original Record unit".into());
        }
        let available =
            exact::sum_exact([actual, loan.map_or_else(exact::zero, |loan| loan.delta)])?;
        let surplus = exact::difference(available, reserved)?;
        let can_offer = if loan_unit_changed || surplus.is_negative() {
            exact::zero()
        } else {
            surplus
        };
        Ok(Balance {
            actual,
            available,
            reserved,
            surplus,
            can_offer,
            commitments,
            incomplete,
        })
    }

    async fn commitment(
        &self,
        connection: &mut SqliteConnection,
        record: &str,
        person: &str,
        promise: &Promise,
        occurrence: Option<&Occurrence>,
    ) -> Result<Option<Commitment>, StoreError> {
        let (occurrence, total, due) = match occurrence {
            Some(value) => (
                Some(value.uid.as_str()),
                value.quantity,
                value.due.as_deref(),
            ),
            None => (None, promise.quantity.abs(), promise.due.as_deref()),
        };
        if total <= 0.0 {
            return Err(invalid("the agreed quantity is unavailable"));
        }
        let mut applied = match occurrence {
            Some(uid) => accounting::applied_on(connection, uid, person).await?,
            None => accounting::Applied {
                canonical: exact::zero(),
                local: exact::zero(),
            },
        };
        if self.scenario_table {
            let effects = sqlx::query(
                "SELECT canonical_mantissa, canonical_scale, local_mantissa, local_scale
                 FROM simulation_assumed_transfer_effect
                 WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ? AND record_uid = ?
                   AND (occurrence_uid IS ? OR occurrence_uid IS NULL)",
            )
            .bind(&promise.transfer)
            .bind(&promise.exchange)
            .bind(person)
            .bind(record)
            .bind(occurrence)
            .fetch_all(&mut *connection)
            .await?;
            for effect in effects {
                applied.canonical = exact::sum_exact([
                    applied.canonical,
                    exact::read_decimal(&effect, "canonical")?,
                ])?;
                applied.local =
                    exact::sum_exact([applied.local, exact::read_decimal(&effect, "local")?])?;
            }
        }
        let total = accounting::amount(total)?;
        let cancelled = cancelled_on(connection, promise, occurrence).await?;
        let total = exact::difference(total, cancelled)?;
        let canonical_remaining = exact::difference(total, applied.canonical)?;
        if !canonical_remaining.is_positive() {
            return Ok(None);
        }
        let policy =
            accounting::policy_on(connection, &promise.transfer, &promise.exchange, person).await?;
        let outgoing = promise.outgoing(person);
        let binding = accounting::Binding {
            transfer: &promise.transfer,
            exchange: &promise.exchange,
            occurrence,
            person,
            record: policy.is_none().then_some(record),
            unit: promise.unit.as_deref(),
            outgoing,
        };
        let application =
            accounting::effective_with_policy_on(connection, &binding, policy).await?;
        let group = transfer_effects::quote_prepared_on(
            connection,
            &binding,
            total,
            &application,
            Some(self.scenario_table),
            |record| self.loans.get(record).copied(),
        )
        .await?;
        let private_remaining = if let Some(group) = group {
            group
                .effects
                .into_iter()
                .find(|effect| effect.record == record)
                .ok_or_else(|| invalid("private Record is outside the effect group"))?
                .delta
        } else {
            accounting::calculate(&application.formula, total, applied.local)?.0
        };
        let reserved = if promise.reserves() && private_remaining.is_negative() {
            exact::negate(private_remaining)?
        } else {
            exact::zero()
        };
        Ok(Some(Commitment {
            transfer: promise.transfer.clone(),
            title: promise.title.clone(),
            revision: promise.revision,
            promise: promise.uid.clone(),
            exchange: promise.exchange.clone(),
            occurrence: occurrence.map(str::to_owned),
            person: person.into(),
            record: record.into(),
            application_record: application.record.unwrap_or_else(|| record.into()),
            unit: promise.unit.clone(),
            canonical_remaining,
            private_remaining,
            outgoing,
            reserved,
            state: promise.state.clone(),
            due: due.map(str::to_owned),
            alternative_group: promise.alternative.clone(),
        }))
    }
}
