use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    pub from: String,
    pub until: String,
}

impl Interval {
    pub fn bounds(&self) -> Result<(i64, i64), &'static str> {
        let parse = |value: &str| {
            if value.len() > 80 {
                return Err("a loan date must include its timezone");
            }
            DateTime::parse_from_rfc3339(value)
                .map(|date| date.timestamp_millis())
                .map_err(|_| "a loan date must include its timezone")
        };
        let from = parse(&self.from)?;
        let until = parse(&self.until)?;
        if until <= from {
            return Err("the loan must end after it starts");
        }
        Ok((from, until))
    }

    pub fn contains(&self, at_ms: i64) -> Result<bool, &'static str> {
        let (from, until) = self.bounds()?;
        Ok(from <= at_ms && at_ms < until)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub origin: String,
    pub transfer: String,
    pub exchange: String,
}

impl Reference {
    pub fn validate(&self) -> Result<(), &'static str> {
        if [&self.origin, &self.transfer, &self.exchange]
            .into_iter()
            .any(|value| value.is_empty() || value.len() > 512)
        {
            return Err("a loan link needs its origin, Transfer and exchange");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_compare_instants_and_exclude_the_end() {
        let local = Interval {
            from: "2026-09-28T09:00:00-03:00".into(),
            until: "2026-10-01T09:00:00-03:00".into(),
        };
        let utc = Interval {
            from: "2026-09-28T12:00:00Z".into(),
            until: "2026-10-01T12:00:00Z".into(),
        };
        assert_eq!(local.bounds().unwrap(), utc.bounds().unwrap());
        let (from, until) = local.bounds().unwrap();
        assert_eq!(until - from, 3 * 86_400_000);
        assert!(!local.contains(from - 1).unwrap());
        assert!(local.contains(from).unwrap());
        assert!(local.contains(until - 1).unwrap());
        assert!(!local.contains(until).unwrap());
        assert!(
            Interval {
                from: local.from.clone(),
                until: local.from
            }
            .bounds()
            .is_err()
        );
        assert!(
            Interval {
                from: "2026-09-28T09:00:00".into(),
                until: utc.until
            }
            .bounds()
            .is_err()
        );
    }
}
