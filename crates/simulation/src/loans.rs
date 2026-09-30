use nucleus::{DecimalValue, transfer::loans::Reference};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub source: Reference,
    pub accepted_revision: u64,
    pub person: String,
    pub until: Option<String>,
    pub returned: Option<DecimalValue>,
}

impl Timing {
    pub fn validate(&self) -> crate::Result<()> {
        self.source.validate()?;
        if self.accepted_revision == 0
            || self.person.is_empty()
            || self.person.len() > 512
            || self.returned.is_some_and(|quantity| quantity.is_negative())
        {
            return Err("invalid loan assumption".into());
        }
        if let Some(until) = &self.until {
            if until.len() > 80 {
                return Err("invalid loan deadline".into());
            }
            chrono::DateTime::parse_from_rfc3339(until)?;
        }
        Ok(())
    }
}

impl crate::world::World {
    pub(crate) async fn assume_loan(
        &mut self,
        cell: &str,
        mut timing: Timing,
        cause: nucleus::simulation::Cause,
    ) -> crate::Result<()> {
        timing.source.origin = self.resolve_reference(&timing.source.origin);
        timing.source.transfer = self.resolve_reference(&timing.source.transfer);
        timing.person = self.resolve_reference(&timing.person);
        timing.validate()?;
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let pool = &node.engine().store.pool;
        let local = store::organs::local(pool).await?.ok_or("missing Organ")?;
        let person = store::records::get(pool, &timing.person)
            .await?
            .ok_or("Person unavailable")?;
        if person.kind != "person" || person.organ_uid.as_deref() != Some(&local.uid) {
            return Err("simulate loan availability only for your own Person".into());
        }
        let source = store::transfer_loans::terms(pool, false)
            .await?
            .into_iter()
            .find(|source| {
                source.origin == timing.source.origin
                    && source.transfer == timing.source.transfer
                    && source.item.exchange.as_ref().is_some_and(|route| {
                        route.uid == timing.source.exchange && route.receiver == timing.person
                    })
            })
            .ok_or("accepted loan unavailable to this borrower")?;
        let accepted = source
            .accepted
            .ok_or("agree to the loan before assuming different dates")?;
        if accepted.revision != timing.accepted_revision {
            return Err("accepted loan terms changed after review".into());
        }
        let until = timing
            .until
            .as_ref()
            .map(|date| {
                chrono::DateTime::parse_from_rfc3339(date).map(|date| date.timestamp_millis())
            })
            .transpose()?
            .unwrap_or(accepted.until_ms);
        if until <= accepted.from_ms
            || timing
                .returned
                .is_some_and(|quantity| quantity.exact_numeric_cmp(source.settled).is_gt())
        {
            return Err("assumed loan dates or return exceed the accepted loan".into());
        }
        store::sqlx::query("CREATE TABLE IF NOT EXISTS simulation_loan_timing (origin TEXT NOT NULL,transfer_uid TEXT NOT NULL,exchange_uid TEXT NOT NULL,person_uid TEXT NOT NULL,until_ms INTEGER NOT NULL,returned_mantissa TEXT NOT NULL,returned_scale INTEGER NOT NULL,PRIMARY KEY(origin,transfer_uid,exchange_uid,person_uid)) STRICT").execute(pool).await?;
        let (mantissa, scale) =
            store::exact::decimal_columns(timing.returned.unwrap_or_else(store::exact::zero));
        store::sqlx::query("INSERT INTO simulation_loan_timing VALUES (?,?,?,?,?,?,?) ON CONFLICT DO UPDATE SET until_ms = excluded.until_ms,returned_mantissa = excluded.returned_mantissa,returned_scale = excluded.returned_scale")
            .bind(timing.source.origin).bind(timing.source.transfer).bind(timing.source.exchange).bind(timing.person).bind(until).bind(mantissa).bind(scale).execute(pool).await?;
        self.tick(cell, cause).await
    }
}
