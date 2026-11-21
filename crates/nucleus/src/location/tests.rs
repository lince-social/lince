use crate::ulid_from;

use super::{Fix, Invitation, InvitationGrant, MAX_FIX_AGE_MS, STALE_AFTER_MS, Status, View};

const CAPTURED_AT_MS: i64 = 1_700_000_000_000;

fn fixture_fix() -> Fix {
    Fix {
        sequence: 1,
        latitude: -90.0,
        longitude: 180.0,
        accuracy_metres: Some(0.0),
        captured_at_ms: CAPTURED_AT_MS,
        capture_age_ms: 0,
    }
}

#[test]
fn invitation_round_trips_and_rejects_an_unrepresentable_grant_timestamp()
-> Result<(), &'static str> {
    let record = format!("r_{}", ulid_from(0, 0));
    let mut invitation = Invitation {
        record_uid: record.clone(),
        authority_node_id: "ab".repeat(32),
        grant: Some(InvitationGrant {
            uid: format!("li_{}", ulid_from(0, 0)),
            issuer: record.clone(),
            recipient: record,
            expires_at_ms: CAPTURED_AT_MS,
            radius_metres: None,
        }),
    };
    assert_eq!(Invitation::parse(&invitation.reference()?)?, invitation);
    let grant = invitation
        .grant
        .as_mut()
        .ok_or("Expected the fixture grant")?;
    grant.expires_at_ms = i64::MAX;
    assert_eq!(
        invitation.validate(),
        Err("Invalid private location invitation scope")
    );
    Ok(())
}

#[test]
fn a_fix_keeps_the_sequence_and_capture_age_boundaries() {
    let mut fix = fixture_fix();
    fix.sequence = i64::MAX.unsigned_abs();
    fix.capture_age_ms = MAX_FIX_AGE_MS.unsigned_abs() - 1;
    assert!(fix.validate().is_ok());
    fix.sequence += 1;
    assert_eq!(fix.validate(), Err("Invalid location sequence"));
    fix.sequence = 0;
    assert_eq!(fix.validate(), Err("Invalid location sequence"));
    fix.sequence = 1;
    fix.capture_age_ms += 1;
    assert_eq!(
        fix.validate(),
        Err("Location capture age exceeds the freshness limit")
    );
    fix.latitude = f64::NAN;
    assert_eq!(
        fix.validate(),
        Err("Invalid location coordinates or accuracy")
    );
}

#[test]
fn lease_expiry_takes_precedence_over_fix_freshness() {
    let max_age = MAX_FIX_AGE_MS.unsigned_abs();
    let mut live = View {
        record_uid: "fixture".into(),
        status: Status::Live,
        session_uid: Some("session".into()),
        expires_at_ms: Some(CAPTURED_AT_MS),
        source_kind: None,
        fix: Some(fixture_fix()),
        age_ms: Some(STALE_AFTER_MS),
        remaining_millis: Some(max_age * 2),
        next_sequence: Some(2),
        radius_metres: Some(100),
    };
    let mut expired = live.clone();
    live.elapse(0);
    assert_eq!(live.status, Status::Live);
    live.elapse(1);
    assert_eq!(live.status, Status::Stale);
    live.elapse(max_age - STALE_AFTER_MS - 1);
    assert_eq!(live.status, Status::Unavailable);
    assert_eq!(live.session_uid.as_deref(), Some("session"));
    assert!(live.fix.is_none());
    assert!(live.radius_metres.is_none());

    expired.age_ms = Some(u64::MAX);
    expired.remaining_millis = Some(1);
    expired.elapse(1);
    assert_eq!(expired.status, Status::Stopped);
    assert_eq!(expired.expires_at_ms, Some(CAPTURED_AT_MS));
    assert_eq!(expired.remaining_millis, Some(0));
    assert!(expired.session_uid.is_none());
    assert!(expired.fix.is_none());
    assert!(expired.age_ms.is_none());
    assert!(expired.next_sequence.is_none());
    assert!(expired.radius_metres.is_none());
}
