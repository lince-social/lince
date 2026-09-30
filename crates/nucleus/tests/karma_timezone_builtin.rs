use nucleus::karma::{
    BundledTimeZoneProvider, CivilDateTime, LocalTimeResolution, TimeZoneId, TimeZoneProvider,
};

fn at(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}

fn resolve(zone: &str, local: &str) -> LocalTimeResolution {
    BundledTimeZoneProvider::new()
        .unwrap()
        .resolve_local(
            &TimeZoneId::new(zone).unwrap(),
            CivilDateTime::parse_canonical(local).unwrap(),
        )
        .unwrap()
}

#[test]
fn bundled_timezones_resolve_brazil_and_both_sides_of_a_repeated_hour() {
    let LocalTimeResolution::Unique { instant } =
        resolve("America/Sao_Paulo", "2030-01-01T09:00:00.000")
    else {
        panic!("Expected a unique local date");
    };
    assert_eq!(instant.as_millis(), at("2030-01-01T12:00:00Z"));
    let LocalTimeResolution::Fold { first, second } =
        resolve("America/New_York", "2030-11-03T01:30:00.000")
    else {
        panic!("Expected a repeated hour");
    };
    assert_eq!(first.as_millis(), at("2030-11-03T05:30:00Z"));
    assert_eq!(second.as_millis(), at("2030-11-03T06:30:00Z"));
}

#[test]
fn missing_hours_and_skipped_days_have_exact_neighboring_instants() {
    for (zone, local, expected) in [
        (
            "America/New_York",
            "2030-03-10T02:30:00.000",
            "2030-03-10T07:00:00Z",
        ),
        (
            "Pacific/Apia",
            "2011-12-30T12:00:00.000",
            "2011-12-30T10:00:00Z",
        ),
    ] {
        let LocalTimeResolution::Gap {
            before,
            first_valid_after,
        } = resolve(zone, local)
        else {
            panic!("Expected a missing local date");
        };
        assert_eq!(first_valid_after.as_millis(), at(expected));
        assert_eq!(first_valid_after.as_millis() - before.as_millis(), 1);
    }
}

#[test]
fn unknown_zones_are_refused_and_the_bundled_revision_is_stable() {
    let provider = BundledTimeZoneProvider::new().unwrap();
    assert_eq!(
        provider.revision(),
        BundledTimeZoneProvider::new().unwrap().revision()
    );
    for zone in ["Unknown/Place", "UTC"] {
        assert!(
            provider
                .resolve_local(
                    &TimeZoneId::new(zone).unwrap(),
                    CivilDateTime::parse_canonical("2030-01-01T09:00:00.000").unwrap(),
                )
                .is_err()
        );
    }
}
