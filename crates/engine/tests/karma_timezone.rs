use std::collections::BTreeMap;

use engine::karma_timezone::load_time_zone_artifact;
use nucleus::karma::{
    CanonicalHash, CivilDateTime, LocalTimeResolution, TimeZoneArtifact, TimeZoneDefinition,
    TimeZoneId, TimestampMs, TzdbRevision, TzdbVersion, UtcOffsetSegment,
};

#[test]
fn engine_loads_only_the_exact_content_addressed_timezone_artifact() {
    let timezone = TimeZoneId::new("Etc/UTC").unwrap();
    let artifact = TimeZoneArtifact::new(
        TzdbVersion::new("2026a+lince.1").unwrap(),
        BTreeMap::from([(
            timezone.clone(),
            TimeZoneDefinition::new(vec![UtcOffsetSegment::new(None, None, 0).unwrap()]).unwrap(),
        )]),
    )
    .unwrap();
    let expected = artifact.revision().unwrap();
    let path = std::env::temp_dir().join(format!(
        "lince-karma-tzdb-{}.json",
        nucleus::new_uid("test")
    ));
    std::fs::write(&path, artifact.canonical_bytes().unwrap()).unwrap();

    let provider = load_time_zone_artifact(&path, &expected).unwrap();
    assert_eq!(provider.revision(), &expected);
    assert_eq!(
        provider
            .resolve_local(
                &timezone,
                CivilDateTime::parse_canonical("2026-07-22T12:00:00.000").unwrap(),
            )
            .unwrap(),
        LocalTimeResolution::Unique {
            instant: TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap()
        }
    );

    let wrong = TzdbRevision {
        version: expected.version.clone(),
        digest: CanonicalHash::parse(format!("sha256:{}", "f".repeat(64))).unwrap(),
    };
    let error = match load_time_zone_artifact(&path, &wrong) {
        Ok(_) => panic!("wrong timezone content address must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code(), Some("karma_tzdb_artifact_invalid"));
    std::fs::remove_file(path).unwrap();
}
