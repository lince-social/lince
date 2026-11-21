use super::*;

#[test]
fn regular_fact_identities_use_the_fact_prefix_and_canonical_digits() {
    let valid = "f_01Q3DCBD008RR8522CDK0Y63C5";
    assert_eq!(
        String::from(FactId::try_from(valid.to_owned()).unwrap()),
        valid
    );
    for invalid in [
        "r_01Q3DCBD008RR8522CDK0Y63C5",
        "f_01Q3DCBD008RR8522CDK0Y63c5",
        "f_01Q3DCBD008RR8522CDK0Y63I5",
        "f_01Q3DCBD008RR8522CDK0Y63C",
    ] {
        assert!(FactId::try_from(invalid.to_owned()).is_err(), "{invalid}");
    }
}

#[test]
fn delivery_fact_identities_round_trip_and_reject_unknown_operations() {
    for operation in ["configure", "set-mode", "revoke", "enqueue", "retry"] {
        let value = format!("tdf:{operation}:request");
        let fact = FactId::try_from(value.clone()).unwrap();
        let encoded = serde_json::to_string(&fact).unwrap();
        assert_eq!(serde_json::from_str::<FactId>(&encoded).unwrap(), fact);
        assert_eq!(String::from(fact), value);
    }
    let application = "taf:taa:tla_01Q3DCBD008RR8522CDK0Y63C5".to_owned();
    assert_eq!(
        String::from(FactId::try_from(application.clone()).unwrap()),
        application
    );
    for value in [
        "tdf:unknown:request",
        "tdf:enqueue:",
        "tdf:retry:bad\nrequest",
        "taf:taa:tla_unknown",
    ] {
        assert!(FactId::try_from(value.to_owned()).is_err());
    }
}

#[test]
fn semantic_contract_rejects_unknown_fields_kinds_and_versions() {
    assert!(serde_json::from_str::<Version>("\"lince.simulation.v2\"").is_err());
    assert!(serde_json::from_str::<Stop>(r#"{"kind":"horizon-reached","ignored":true}"#).is_err());
    assert!(serde_json::from_str::<Observation>(r#"{"kind":"unknown"}"#).is_err());
    assert!(serde_json::from_str::<Quantity>(r#"{"value":"NaN","unit":null}"#).is_err());
}

#[test]
fn empty_report_variants_reject_unrecognized_fields() {
    fn strict<T: Serialize + serde::de::DeserializeOwned>(variants: &[T]) {
        for variant in variants {
            let mut value = serde_json::to_value(variant).unwrap();
            assert!(serde_json::from_value::<T>(value.clone()).is_ok());
            value
                .as_object_mut()
                .unwrap()
                .insert("ignored".into(), true.into());
            assert!(serde_json::from_value::<T>(value).is_err());
        }
    }
    strict(&[Cause::Timer {}, Cause::Seed {}]);
    strict(&[
        Refusal::Forbidden {},
        Refusal::InvalidAction {},
        Refusal::UnsupportedBoundary {},
    ]);
    strict(&[Observation::Restarted {}]);
    strict(&[
        Predicate::OncePerOccurrence {},
        Predicate::NoUnexpectedRefusals {},
        Predicate::FactChain {},
    ]);
    strict(&[
        Stop::HorizonReached {},
        Stop::EventBudget {},
        Stop::EvidenceBudget {},
        Stop::Cancelled {},
        Stop::Paused {},
    ]);
    strict(&[ReplayStatus::Unverified {}]);
    use crate::projection::{Incomplete, Status};
    strict(&[
        Incomplete::Budget {},
        Incomplete::ExternalEffects {},
        Incomplete::RuleFailure {},
        Incomplete::UnavailableRuntime {},
        Incomplete::PastWindow {},
        Incomplete::UnsupportedFilter {},
    ]);
    strict(&[Status::Updating {}]);
}
