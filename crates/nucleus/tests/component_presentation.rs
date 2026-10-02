use nucleus::component::{ComponentState, RecordMode};
use nucleus::karma::{Consequence, rule_field::RuleConsequence};

#[test]
fn component_state_is_typed_and_round_trips_through_the_rule_editor() {
    for source in [
        "@room: show({\"kind\":\"record\",\"mode\":\"call\"})",
        "@room: show({\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\"}})",
        "@room: show({\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\",\"media\":\"video\"}})",
        "@room: show({\"kind\":\"text\",\"text\":\"Clean room\"})",
        "@room: show({\"kind\":\"karma\",\"search\":\"clean\"})",
        "@room: show({\"kind\":\"frequency\",\"search\":\"daily\"})",
    ] {
        let parsed = RuleConsequence::parse(source).unwrap();
        let restored = RuleConsequence::parse(&parsed.as_text()).unwrap();
        assert_eq!(parsed.consequences, restored.consequences);
    }
    let parsed =
        RuleConsequence::parse("@room: show({\"kind\":\"record\",\"mode\":\"call\"})").unwrap();
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
}

#[test]
fn invalid_component_and_call_configuration_is_refused() {
    for state in [
        "{\"kind\":\"record\",\"mode\":\"started\"}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"call_started\":true}",
        "{\"kind\":\"record\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\"}}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"start_call\":true}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"\",\"person\":\"me\"}}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\"}}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\",\"media\":\"screen\"}}",
        "{\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\",\"extra\":true}}",
        "{\"kind\":\"unknown\"}",
        "{\"kind\":\"text\",\"text\":3}",
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
fn automatic_call_defaults_to_audio_and_exposes_all_record_references() {
    let parsed = RuleConsequence::parse(
        "@room: show({\"kind\":\"record\",\"mode\":\"call\",\"start_call\":{\"thread\":\"conversation\",\"person\":\"me\"}})",
    ).unwrap();
    let Consequence::ShowComponent { component } = &parsed.consequences[0] else {
        panic!()
    };
    assert_eq!(component.records(), ["room", "conversation", "me"]);
    let ComponentState::Record {
        start_call: Some(start),
        ..
    } = component
    else {
        panic!()
    };
    assert_eq!(start.media, nucleus::component::CallMedia::Audio);
}
