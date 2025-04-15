use crate::{
    application::providers::frequency::{
        get::provider_frequency_get, update::provider_frequency_update,
    },
    domain::entities::frequency::Frequency,
};
use chrono::{Datelike, Duration, NaiveDateTime, TimeZone, Utc};

pub fn use_case_frequency_check(id: u32) -> (u32, Option<Frequency>) {
    futures::executor::block_on(async {
        let frequency = provider_frequency_get(id).await;
        if frequency.is_none() {
            return (0, None);
        }
        let frequency = frequency.unwrap();

        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if frequency.next_date > now {
            return (0, None);
        }

        if frequency.finish_date.is_some() && frequency.finish_date.clone().unwrap() < now {
            let frequency = Frequency {
                quantity: 0.0,
                ..frequency.clone()
            };
            provider_frequency_update(frequency).await;
            return (0, None);
        }

        let naive =
            NaiveDateTime::parse_from_str(&frequency.next_date, "%Y-%m-%d %H:%M:%S").unwrap();

        let shifted = naive
            + Duration::days(frequency.days as i64)
            + Duration::seconds(frequency.seconds as i64);

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

        (1, Some(new_frequency))
    })
}
