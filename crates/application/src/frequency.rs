use chrono::{Datelike, Duration, NaiveDateTime, TimeZone, Utc};
use domain::clean::frequency::Frequency;
use injection::cross_cutting::InjectedServices;

pub async fn frequency_check(services: InjectedServices, id: u32) -> (u32, Option<Frequency>) {
    let frequency = match services.repository.frequency.get(id).await {
        Ok(Some(f)) => f,
        _ => return (0, None),
    };

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    if frequency.next_date > now {
        return (0, None);
    }

    if frequency.finish_date.is_some() && frequency.finish_date.clone().unwrap() < now {
        let frequency = Frequency {
            quantity: 0.0,
            ..frequency.clone()
        };
        let _ = services.repository.frequency.update(frequency).await;
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
    // println!("stop 0");
    // let date_week_day = date.weekday().num_days_from_monday() + 1;
    // dbg!(&date_week_day);
    // // this is a string like "12347", split, parse to u32, get the number bigger than date_week_day if date_week_day is 7, the bigger number is 1,
    // // after getting that number, advance date to that day of week -1, so if today is sunday (7) the next day is monday (1) but the actual chrono Weekday is -1 so monday is 0.
    // // let week_days = frequency.day_week.chars().map(|c| parse).collect::<Vec<u32>>();

    // println!("stop 1");
    // let mut week_days = frequency
    //     .day_week
    //     .chars()
    //     .map(|c| c.to_digit(10).unwrap_or(1))
    //     .collect::<Vec<u32>>();
    // week_days.sort();
    // dbg!(&week_days);

    // println!("stop 2");
    // let mut next_week_day = week_days
    //     .iter()
    //     .find(|&&day| day > date_week_day)
    //     .unwrap_or(&week_days[0]);
    // dbg!(&next_week_day);
    // println!("stop 3");

    // if next_week_day == &7 {
    //     next_week_day = &1;
    // }
    // println!("stop 4");
    // dbg!(&next_week_day);
    // dbg!(&date_week_day);
    // date += Duration::days((next_week_day - date_week_day) as i64);
    // println!("stop 5");

    let shifted_final = NaiveDateTime::new(date, shifted.time());
    // println!("stop 6");
    let new_next_date = Utc
        .from_utc_datetime(&shifted_final)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    // println!("stop 7");

    let new_frequency = Frequency {
        next_date: new_next_date,
        ..frequency.clone()
    };

    (frequency.quantity as u32, Some(new_frequency))
}
