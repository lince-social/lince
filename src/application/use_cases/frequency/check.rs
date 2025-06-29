use crate::{
    domain::entities::frequency::Frequency, infrastructure::cross_cutting::InjectedServices,
};
use chrono::{Datelike, Duration, NaiveDateTime, TimeZone, Utc};

pub async fn use_case_frequency_check(
    services: InjectedServices,
    id: u32,
) -> (u32, Option<Frequency>) {
    dbg!(&id);
    let frequency = match services.providers.frequency.get(id).await {
        Ok(Some(f)) => f,
        _ => return (0, None),
    };

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    dbg!(&now);
    dbg!(&frequency);

    if frequency.next_date > now {
        return (0, None);
    }

    if frequency.finish_date.is_some() && frequency.finish_date.clone().unwrap() < now {
        let frequency = Frequency {
            quantity: 0.0,
            ..frequency.clone()
        };
        let _ = services.providers.frequency.update(frequency).await;
        return (0, None);
    }

    let naive = NaiveDateTime::parse_from_str(&frequency.next_date, "%Y-%m-%d %H:%M:%S").unwrap();

    let shifted =
        naive + Duration::days(frequency.days as i64) + Duration::seconds(frequency.seconds as i64);

    let mut year = shifted.year();
    let mut month = shifted.month() as i32 + frequency.months as i32;
    let day = shifted.day();

    while month > 12 {
        year += 1;
        month -= 12;
    }
    while month < 1 {
        year -= 1;
        month += 12;
    }

    let date = chrono::NaiveDate::from_ymd_opt(year, month as u32, day).unwrap();

    let shifted_final = NaiveDateTime::new(date, shifted.time());
    let new_next_date = Utc
        .from_utc_datetime(&shifted_final)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let new_frequency = Frequency {
        next_date: new_next_date,
        ..frequency.clone()
    };

    (frequency.quantity as u32, Some(new_frequency))
}
