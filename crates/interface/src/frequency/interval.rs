use super::model::Unit;
use nucleus::karma::{CadenceStep, CivilWeekday};

pub const WEEKDAYS: [CivilWeekday; 7] = [
    CivilWeekday::Monday,
    CivilWeekday::Tuesday,
    CivilWeekday::Wednesday,
    CivilWeekday::Thursday,
    CivilWeekday::Friday,
    CivilWeekday::Saturday,
    CivilWeekday::Sunday,
];

pub fn weekday_label(day: CivilWeekday) -> &'static str {
    match day {
        CivilWeekday::Monday => "Mon",
        CivilWeekday::Tuesday => "Tue",
        CivilWeekday::Wednesday => "Wed",
        CivilWeekday::Thursday => "Thu",
        CivilWeekday::Friday => "Fri",
        CivilWeekday::Saturday => "Sat",
        CivilWeekday::Sunday => "Sun",
    }
}

pub fn parse(text: &str) -> Result<CadenceStep, String> {
    let mut step = CadenceStep::default();
    let text = text.trim().to_lowercase().replace(" and ", "+");
    for part in text.split('+') {
        let part = part.trim();
        let digits = part.bytes().take_while(u8::is_ascii_digit).count();
        let count = part[..digits]
            .parse::<u32>()
            .ok()
            .filter(|count| *count > 0)
            .ok_or("Use positive whole numbers, such as 1 month + 1 day or 1s + 100ms")?;
        let unit = match part[digits..].trim() {
            "ms" | "millisecond" | "milliseconds" => Unit::Milliseconds,
            "s" | "second" | "seconds" => Unit::Seconds,
            "m" | "minute" | "minutes" => Unit::Minutes,
            "h" | "hour" | "hours" => Unit::Hours,
            "d" | "day" | "days" => Unit::Days,
            "w" | "week" | "weeks" => Unit::Weeks,
            "month" | "months" => Unit::Months,
            "y" | "year" | "years" => Unit::Years,
            _ => return Err("Give each amount a time unit, such as 1 day + 10s + 100ms".into()),
        };
        let target = match unit {
            Unit::Milliseconds => &mut step.milliseconds,
            Unit::Seconds => &mut step.seconds,
            Unit::Minutes => &mut step.minutes,
            Unit::Hours => &mut step.hours,
            Unit::Days => &mut step.days,
            Unit::Weeks => &mut step.weeks,
            Unit::Months => &mut step.months,
            Unit::Years => &mut step.years,
        };
        *target = target
            .checked_add(count)
            .ok_or("Time amount is too large")?;
    }
    Ok(step)
}

pub fn describe(step: CadenceStep) -> String {
    [
        (step.years, "year"),
        (step.months, "month"),
        (step.weeks, "week"),
        (step.days, "day"),
        (step.hours, "hour"),
        (step.minutes, "minute"),
        (step.seconds, "second"),
        (step.milliseconds, "ms"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, unit)| {
        format!(
            "{count} {unit}{}",
            if count == 1 || unit == "ms" { "" } else { "s" }
        )
    })
    .collect::<Vec<_>>()
    .join(" + ")
}

pub fn elapsed(mut milliseconds: i64) -> String {
    let mut parts = Vec::new();
    for (unit, divisor) in [
        (Unit::Weeks, 604_800_000),
        (Unit::Days, 86_400_000),
        (Unit::Hours, 3_600_000),
        (Unit::Minutes, 60_000),
        (Unit::Seconds, 1_000),
        (Unit::Milliseconds, 1),
    ] {
        let count = milliseconds / divisor;
        milliseconds %= divisor;
        if count > 0 {
            let label = unit.label();
            let label = if count == 1 && unit != Unit::Milliseconds {
                label.trim_end_matches('s')
            } else {
                label
            };
            parts.push(format!("{count} {label}"));
        }
    }
    parts.join(" + ")
}
