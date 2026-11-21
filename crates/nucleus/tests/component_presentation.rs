use nucleus::{
    component::{CallMedia, ComponentState, RecordMode},
    karma::{Consequence, rule_field::RuleConsequence},
};

#[test]
fn component_state_is_typed_and_round_trips_through_the_rule_editor() -> Result<(), String> {
    for source in [
        r#"@room: show({"kind":"record","mode":"call"})"#,
        r#"@room: show({"kind":"record","mode":"call","start_call":{"thread":"conversation","person":"me"}})"#,
        r#"@room: show({"kind":"record","mode":"call","start_call":{"thread":"conversation","person":"me","media":"video"}})"#,
        r#"@room: show({"kind":"text","text":"Clean room"})"#,
        r#"@room: show({"kind":"karma","search":"clean"})"#,
        r#"@room: show({"kind":"frequency","search":"daily"})"#,
    ] {
        let parsed = RuleConsequence::parse(source)?;
        let restored = RuleConsequence::parse(&parsed.as_text())?;
        assert_eq!(parsed.consequences, restored.consequences);
    }
    let parsed = RuleConsequence::parse(r#"@room: show({"kind":"record","mode":"call"})"#)?;
    assert_eq!(
        parsed.consequences,
        vec![Consequence::ShowComponent {
            component: ComponentState::Record {
                record: "room".into(),
                mode: RecordMode::Call,
                start_call: None,
            }
        }]
    );
    Ok(())
}

#[test]
fn invalid_component_and_call_configuration_is_refused() {
    for state in [
        r#"{"kind":"record","mode":"started"}"#,
        r#"{"kind":"record","mode":"call","call_started":true}"#,
        r#"{"kind":"record","start_call":{"thread":"conversation","person":"me"}}"#,
        r#"{"kind":"record","mode":"call","start_call":true}"#,
        r#"{"kind":"record","mode":"call","start_call":{"thread":"","person":"me"}}"#,
        r#"{"kind":"record","mode":"call","start_call":{"thread":"conversation"}}"#,
        r#"{"kind":"record","mode":"call","start_call":{"thread":"conversation","person":"me","media":"screen"}}"#,
        r#"{"kind":"record","mode":"call","start_call":{"thread":"conversation","person":"me","extra":true}}"#,
        r#"{"kind":"unknown"}"#,
        r#"{"kind":"text","text":3}"#,
    ] {
        assert!(RuleConsequence::parse(&format!("@room: show({state})")).is_err());
    }
    assert!(
        ComponentState::Text {
            text: "x".repeat(4097)
        }
        .validate()
        .is_err()
    );
}

#[test]
fn automatic_call_defaults_to_audio_and_preserves_reference_order() -> Result<(), String> {
    let mut parsed = RuleConsequence::parse(
        r#"@room: show({
            "kind":"composition",
            "composition":{
                "name":"Calls",
                "parts":[
                    {"id":"call","size":[1,1],"component":{
                        "kind":"record","record":"room","mode":"call",
                        "start_call":{"thread":"conversation","person":"me"}
                    }},
                    {"id":"notes","size":[1,1],"component":{"kind":"record","record":"notes"}}
                ]
            }
        })"#,
    )?;
    let [Consequence::ShowComponent { component }] = parsed.consequences.as_mut_slice() else {
        return Err("Expected one ShowComponent consequence".into());
    };
    let ComponentState::Composition { composition } = &*component else {
        return Err("Expected a composition".into());
    };
    let [first, _] = composition.parts.as_slice() else {
        return Err("Expected two component parts".into());
    };
    let ComponentState::Record {
        start_call: Some(start),
        ..
    } = &first.component
    else {
        return Err("Expected the first part to start a call".into());
    };
    assert_eq!(start.media, CallMedia::Audio);

    let references = component.records_mut();
    assert_eq!(
        references
            .iter()
            .map(|reference| reference.as_str())
            .collect::<Vec<_>>(),
        ["room", "conversation", "me", "notes"]
    );
    let updated = ["office", "team", "owner", "new-notes"];
    for (reference, updated) in references.into_iter().zip(updated) {
        updated.clone_into(reference);
    }
    assert_eq!(component.records(), updated);
    Ok(())
}
