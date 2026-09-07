use std::collections::BTreeMap;

use nucleus::karma::{
    CanonicalHash, CompiledFrequency, CompiledSchedule, DefinitionStatus, DurationMs,
    ElapsedSchedule, FREQUENCY_PARAMETER_HASH_DOMAIN, FrequencyActivationCause,
    FrequencyActivationEpoch, FrequencyActivationEpochSchema, FrequencyMutationAction,
    FrequencyMutationEvidence, FrequencyMutationEvidenceSchema, FrequencyParameterValue,
    InactiveGapPolicy, LocalId, MissedPolicy, OverloadPolicy, ProgramMutationAction,
    ProgramMutationEvidence, ProgramMutationEvidenceSchema, RephasePolicy, TimerPolicy,
    TimestampMs, canonical_hash,
};
use serde::Serialize;

#[derive(Serialize)]
struct ProgramMutationVocabulary {
    schemas: Vec<ProgramMutationEvidenceSchema>,
    actions: Vec<ProgramMutationAction>,
    statuses: Vec<DefinitionStatus>,
    evidence: ProgramMutationEvidence,
}

#[derive(Serialize)]
struct FrequencyDurableVocabulary {
    evidence_schemas: Vec<FrequencyMutationEvidenceSchema>,
    actions: Vec<FrequencyMutationAction>,
    activation_schemas: Vec<FrequencyActivationEpochSchema>,
    activation_causes: Vec<FrequencyActivationCause>,
    epoch: FrequencyActivationEpoch,
    evidence: FrequencyMutationEvidence,
}

#[test]
fn program_mutation_evidence_has_a_complete_stable_wire_vocabulary() {
    let previous_head_revision_hash = digest('1');
    let head_revision_hash = digest('2');
    let active_revision_hash = digest('3');
    let fixture = ProgramMutationVocabulary {
        schemas: vec![ProgramMutationEvidenceSchema::V1],
        actions: vec![
            ProgramMutationAction::Create,
            ProgramMutationAction::Revise,
            ProgramMutationAction::Activate,
            ProgramMutationAction::Pause,
        ],
        statuses: vec![
            DefinitionStatus::Draft,
            DefinitionStatus::Proven,
            DefinitionStatus::Active,
            DefinitionStatus::Paused,
        ],
        evidence: ProgramMutationEvidence {
            schema: ProgramMutationEvidenceSchema::V1,
            action: ProgramMutationAction::Activate,
            request_id: "request-program-activate-0001".to_string(),
            program_uid: "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            actor_person_uid: Some("r_01APS3NDEKTSV4RRFFQ69G5FAV".to_string()),
            previous_handle_revision: Some(4),
            handle_revision: 5,
            previous_head_revision_hash: Some(previous_head_revision_hash),
            head_revision_hash,
            previous_active_revision_hash: None,
            active_revision_hash: Some(active_revision_hash),
            status: DefinitionStatus::Active,
        },
    };

    assert_eq!(
        canonical_hash("karma.program-mutation-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:6b2ef408fcbc4b2aec4cc8361169bc0371de76b316cd6c74fa178ab1f5090a58"
    );
}

#[test]
fn program_mutation_evidence_omits_only_absent_optional_history() {
    let evidence = ProgramMutationEvidence {
        schema: ProgramMutationEvidenceSchema::V1,
        action: ProgramMutationAction::Create,
        request_id: "request-program-create-0001".to_string(),
        program_uid: "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
        actor_person_uid: None,
        previous_handle_revision: None,
        handle_revision: 1,
        previous_head_revision_hash: None,
        head_revision_hash: digest('a'),
        previous_active_revision_hash: None,
        active_revision_hash: None,
        status: DefinitionStatus::Proven,
    };

    let value = serde_json::to_value(evidence).unwrap();
    assert_eq!(value["schema"], "karma.program-mutation-evidence.v1");
    assert_eq!(value["action"], "create");
    assert_eq!(value["status"], "proven");
    assert!(value.get("actor_person_uid").is_none());
    assert!(value.get("previous_handle_revision").is_none());
    assert!(value.get("previous_head_revision_hash").is_none());
    assert!(value.get("previous_active_revision_hash").is_none());
    assert!(value.get("active_revision_hash").is_none());
}

#[test]
fn frequency_activation_epoch_and_mutation_vocabulary_are_stable() {
    let epoch = activation_epoch();
    assert_eq!(
        epoch.activation_hash().unwrap().as_str(),
        "sha256:8be9d3f92084172d2391f4d75b83aea15e3b9f368b03f13934ef24ff4ff97732"
    );
    let fixture = FrequencyDurableVocabulary {
        evidence_schemas: vec![FrequencyMutationEvidenceSchema::V1],
        actions: vec![
            FrequencyMutationAction::Create,
            FrequencyMutationAction::Revise,
            FrequencyMutationAction::Activate,
            FrequencyMutationAction::SetParameters,
            FrequencyMutationAction::ResetParameters,
            FrequencyMutationAction::Pause,
        ],
        activation_schemas: vec![FrequencyActivationEpochSchema::V1],
        activation_causes: vec![
            FrequencyActivationCause::ActivateRevision,
            FrequencyActivationCause::SetParameters,
            FrequencyActivationCause::ResetParameters,
        ],
        evidence: FrequencyMutationEvidence {
            schema: FrequencyMutationEvidenceSchema::V1,
            action: FrequencyMutationAction::SetParameters,
            request_id: "request-frequency-parameters-0001".to_string(),
            frequency_uid: "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            actor_person_uid: Some("r_01APS3NDEKTSV4RRFFQ69G5FAV".to_string()),
            previous_handle_revision: Some(4),
            handle_revision: 5,
            previous_head_revision_hash: Some(digest('1')),
            head_revision_hash: digest('2'),
            previous_active_revision_hash: Some(digest('3')),
            active_revision_hash: Some(digest('4')),
            previous_activation_hash: Some(digest('5')),
            activation_hash: Some(digest('6')),
            previous_effective_parameter_hash: Some(digest('7')),
            effective_parameter_hash: Some(digest('8')),
            activated_at: Some(timestamp()),
            status: DefinitionStatus::Active,
        },
        epoch,
    };
    assert_eq!(
        canonical_hash("karma.frequency-durable-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:0616164d01dfc8a0ac3861dfeb4500489edf70bd394c711c03f6e675d61a271f"
    );
}

#[test]
fn frequency_activation_epoch_revalidates_hostile_wire_data() {
    let epoch = activation_epoch();
    let encoded = serde_json::to_vec(&epoch).unwrap();
    assert_eq!(
        serde_json::from_slice::<FrequencyActivationEpoch>(&encoded).unwrap(),
        epoch
    );

    let mut wrong_revision = serde_json::to_value(&epoch).unwrap();
    wrong_revision["activating_handle_revision"] = serde_json::json!(1);
    assert!(serde_json::from_value::<FrequencyActivationEpoch>(wrong_revision).is_err());

    let mut false_parameter_hash = serde_json::to_value(&epoch).unwrap();
    false_parameter_hash["effective_parameters"]["interval"]["value"] = serde_json::json!(4);
    false_parameter_hash["compiled"]["effective_parameters"]["interval"]["value"] =
        serde_json::json!(4);
    assert!(serde_json::from_value::<FrequencyActivationEpoch>(false_parameter_hash).is_err());
}

fn activation_epoch() -> FrequencyActivationEpoch {
    let effective_parameters = BTreeMap::from([(
        LocalId::new("interval").unwrap(),
        FrequencyParameterValue::Duration {
            value: DurationMs::new(3),
        },
    )]);
    let effective_parameter_hash =
        canonical_hash(FREQUENCY_PARAMETER_HASH_DOMAIN, &effective_parameters).unwrap();
    let revision_hash = digest('4');
    let compiled = CompiledFrequency {
        revision_hash: revision_hash.clone(),
        effective_parameter_hash,
        effective_parameters,
        schedule: CompiledSchedule::Elapsed {
            schedule: ElapsedSchedule::new(
                timestamp(),
                3,
                TimerPolicy::new(1, 5, 0).unwrap(),
                MissedPolicy::Replay {
                    max: std::num::NonZeroU32::new(64).unwrap(),
                },
                InactiveGapPolicy::SkipToNextAnchor,
                RephasePolicy::PreserveAnchor,
                OverloadPolicy::PauseAndAsk,
            )
            .unwrap(),
        },
    };
    FrequencyActivationEpoch::new(
        "r_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
        5,
        revision_hash,
        compiled,
        Some(digest('5')),
        FrequencyActivationCause::SetParameters,
        timestamp(),
    )
    .unwrap()
}

fn timestamp() -> TimestampMs {
    TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap()
}

fn digest(digit: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}
