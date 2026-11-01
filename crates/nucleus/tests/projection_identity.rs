use nucleus::karma::{CanonicalHash, canonical_hash};
use nucleus::projection::{Context, ExecutionIdentity, Window};

#[test]
fn identity_preserves_key_encoding_and_actor_isolation() {
    let build = format!("sha256:{}", "a".repeat(64));
    let identity = ExecutionIdentity::parse(build.clone()).unwrap();
    let mut context = Context {
        actor: None,
        window: Window {
            from_ms: 0,
            until_ms: 60_000,
            timezone: "UTC".into(),
        },
    };
    let expected = canonical_hash(
        "lince.projection.v1",
        &(None::<String>, &build, chrono_tz::IANA_TZDB_VERSION),
    )
    .unwrap();
    assert_eq!(context.key(&identity).unwrap(), expected);
    assert_eq!(context.key(&identity.clone()).unwrap(), expected);
    context.window.until_ms = 120_000;
    assert_eq!(context.key(&identity).unwrap(), expected);
    context.actor = Some("visitor".into());
    assert_ne!(context.key(&identity).unwrap(), expected);
    context.actor = None;
    let other = ExecutionIdentity::parse(format!("sha256:{}", "b".repeat(64))).unwrap();
    assert_ne!(context.key(&other).unwrap(), expected);
    assert_eq!(
        CanonicalHash::parse(identity.as_str()).unwrap().as_str(),
        build
    );
}

#[test]
fn execution_identity_rejects_malformed_fingerprints() {
    for value in [
        String::new(),
        "unknown".into(),
        format!("sha256:{}", "0".repeat(63)),
        format!("sha256:{}", "A".repeat(64)),
        format!("sha256:{}", "g".repeat(64)),
    ] {
        assert!(ExecutionIdentity::parse(value).is_err());
    }
}
