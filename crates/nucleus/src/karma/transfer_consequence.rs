use crate::expr::Expr;

use super::{Consequence, DecimalValue, DurationMs};

pub fn level(value: DecimalValue) -> Result<u8, String> {
    let factor = 10_i128
        .checked_pow(u32::from(value.scale()))
        .ok_or("Agreement value is too large")?;
    let integer = value.mantissa() / factor;
    if value.mantissa() % factor != 0 || !(0..=2).contains(&integer) {
        return Err(format!(
            "Agreement target {value} must be a whole number 0, 1 or 2"
        ));
    }
    u8::try_from(integer).map_err(|error| error.to_string())
}

fn reference(value: &Expr, name: &str) -> Result<String, String> {
    match value {
        Expr::Ref(value) => Ok(value.clone()),
        _ => Err(format!("Choose @{name}")),
    }
}

fn duration(value: &Expr) -> Result<DurationMs, String> {
    match value {
        Expr::Dur(seconds) if *seconds > 0 => seconds
            .checked_mul(1000)
            .map(DurationMs::new)
            .ok_or_else(|| "Agreement delay is too large".into()),
        _ => Err("Use a positive agreement delay, such as 3d".into()),
    }
}

pub fn parse(target: &str, source: &str) -> Result<Option<Consequence>, String> {
    if !["agreement(", "publish(", "activate("]
        .iter()
        .any(|name| source.starts_with(name))
    {
        return Ok(None);
    }
    let Expr::Fn(name, arguments) = Expr::parse(source).map_err(|error| error.to_string())? else {
        return Err("Choose agreement(), publish() or activate()".into());
    };
    let person = arguments
        .first()
        .ok_or_else(|| "Choose an acting @person".to_string())
        .and_then(|value| reference(value, "person"))?;
    Ok(Some(match name.as_str() {
        "agreement" => {
            let (value, after_ms) = match arguments.as_slice() {
                [_] => (None, None),
                [_, Expr::Num(value)] => (
                    Some(DecimalValue::parse_inferred(value).map_err(|error| error.to_string())?),
                    None,
                ),
                [_, delay @ Expr::Dur(_)] => (None, Some(duration(delay)?)),
                [_, Expr::Num(value), delay] => (
                    Some(DecimalValue::parse_inferred(value).map_err(|error| error.to_string())?),
                    Some(duration(delay)?),
                ),
                _ => {
                    return Err(
                        "Use agreement(@person), agreement(@person, 2), or add a delay such as 3d"
                            .into(),
                    );
                }
            };
            if let Some(value) = value {
                level(value)?;
            }
            Consequence::SetTransferAgreement {
                transfer: target.into(),
                person,
                level: value,
                after_ms,
            }
        }
        "publish" if arguments.len() == 1 => Consequence::PublishTransfer {
            transfer: target.into(),
            person,
        },
        "activate" if arguments.len() == 3 => {
            let promise = reference(&arguments[1], "promise")?;
            let Expr::Text(fulfillment) = &arguments[2] else {
                return Err("Quote the fulfillment key, for example activate(@me, @promise, \"purchase-once\")".into());
            };
            Consequence::ActivateTransferFulfillment {
                transfer: target.into(),
                person,
                promise,
                fulfillment: fulfillment.clone(),
            }
        }
        _ => {
            return Err(
                "Use publish(@person) or activate(@person, @promise, \"fulfillment-key\")".into(),
            );
        }
    }))
}

fn display_duration(value: DurationMs) -> Option<String> {
    let milliseconds = value.get();
    if milliseconds <= 0 || milliseconds % 1000 != 0 {
        return None;
    }
    let seconds = milliseconds / 1000;
    for (unit, length) in [
        ("w", 604_800),
        ("d", 86_400),
        ("h", 3_600),
        ("m", 60),
        ("s", 1),
    ] {
        if seconds % length == 0 {
            return Some(format!("{}{unit}", seconds / length));
        }
    }
    None
}

pub fn display(target: &str, effect: &Consequence) -> Option<String> {
    let operation = match effect {
        Consequence::SetTransferAgreement {
            transfer,
            person,
            level,
            after_ms,
        } if transfer == target => {
            let value = level.map(|level| format!(", {level}")).unwrap_or_default();
            let delay = match after_ms {
                Some(value) => Some(display_duration(*value)?),
                None => None,
            };
            format!(
                "agreement(@{person}{value}{})",
                delay.map(|delay| format!(", {delay}")).unwrap_or_default()
            )
        }
        Consequence::PublishTransfer { transfer, person } if transfer == target => {
            format!("publish(@{person})")
        }
        Consequence::ActivateTransferFulfillment {
            transfer,
            person,
            promise,
            fulfillment,
        } if transfer == target => {
            format!(
                "activate(@{person}, @{promise}, {})",
                serde_json::to_string(fulfillment).ok()?
            )
        }
        _ => return None,
    };
    Some(format!("@{target}: {operation}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_exact_and_never_rounded() {
        for (value, expected) in [("0", 0), ("1.00", 1), ("2", 2)] {
            assert_eq!(
                level(DecimalValue::parse_inferred(value).unwrap()).unwrap(),
                expected
            );
        }
        for value in ["-1", "0.1", "1.999999999", "3", "1000000000"] {
            assert!(level(DecimalValue::parse_inferred(value).unwrap()).is_err());
        }
    }

    #[test]
    fn transfer_operations_round_trip_and_keep_fulfillment_identity() {
        for source in [
            "agreement(@me)",
            "agreement(@me, 2)",
            "agreement(@me, 3d)",
            "agreement(@me, 2, 3d)",
            "publish(@me)",
            "activate(@me, @promise, \"purchase-once\")",
        ] {
            let effect = parse("trade", source).unwrap().unwrap();
            assert_eq!(
                display("trade", &effect).unwrap(),
                format!("@trade: {source}")
            );
        }
        for source in [
            "agreement(@me, 1.5)",
            "agreement(@me, 3)",
            "agreement(@me, 2, 0s)",
            "publish(@me, 2)",
            "activate(@me, @promise)",
        ] {
            assert!(parse("trade", source).is_err());
        }
    }
}
