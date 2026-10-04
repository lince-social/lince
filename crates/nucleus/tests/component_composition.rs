use nucleus::component::{ComponentState, Composition, Document, Part};

fn composition() -> Composition {
    Composition {
        name: "Habit review".into(),
        origin: None,
        parts: vec![Part {
            settings: Default::default(),
            id: "text".into(),
            events: Vec::new(),
            position: [0, 0],
            size: [640, 480],
            component: ComponentState::Text {
                text: "Review today's habits".into(),
            },
        }],
    }
}

#[test]
fn shared_contract_checks_ids_actions_geometry_depth_and_document_size() {
    let mut draft = composition();
    assert_eq!(
        Document::decode(&Document::encode(draft.clone()).unwrap())
            .unwrap()
            .composition,
        draft
    );
    draft.parts.push(draft.parts[0].clone());
    assert!(draft.validate().is_err());
    draft = composition();
    draft.parts[0].size = [0, 480];
    assert!(draft.validate().is_err());
    assert!(
        ComponentState::Area {
            immunity: Default::default(),
            strength: -1,
        }
        .validate()
        .is_err()
    );
    draft = composition();
    draft.parts[0].component = ComponentState::Button {
        label: "Add a Fact".into(),
        action: serde_json::json!({"action":"capture-entry","target":"r_example","amount":"42"}),
    };
    assert!(draft.validate().is_ok());
    for _ in 0..8 {
        draft = Composition {
            name: "Nested".into(),
            origin: None,
            parts: vec![Part {
                settings: Default::default(),
                id: "nested".into(),
                events: Vec::new(),
                position: [0, 0],
                size: [640, 480],
                component: ComponentState::Composition { composition: draft },
            }],
        };
    }
    assert!(draft.validate().is_err());
    assert!(Document::decode(&" ".repeat(256 * 1024 + 1)).is_err());
}

#[test]
fn activation_has_an_explicit_roundtrip_and_is_an_outward_effect() {
    let parsed =
        nucleus::karma::rule_field::RuleConsequence::parse("@reviewer: activate-fiote").unwrap();
    assert_eq!(parsed.as_text(), "@reviewer: activate-fiote");
    assert_eq!(parsed.consequences[0].kind(), "activate-fiote");
    assert!(parsed.consequences[0].is_outward());
    assert!(!parsed.consequences[0].moves_quantity());
}

#[test]
fn part_settings_roundtrip_and_reject_unbounded_or_structured_values() {
    let mut draft = composition();
    draft.parts[0]
        .settings
        .insert("wrap".into(), serde_json::json!(false));
    draft.parts[0]
        .settings
        .insert("overflow".into(), serde_json::json!("Grow"));
    assert_eq!(
        Document::decode(&Document::encode(draft.clone()).unwrap())
            .unwrap()
            .composition,
        draft
    );
    for value in [
        serde_json::Value::Null,
        serde_json::json!({"action":"delete-record"}),
        serde_json::json!([]),
        serde_json::json!("x".repeat(4097)),
    ] {
        draft.parts[0].settings.insert("bad".into(), value);
        assert!(draft.validate().is_err());
    }
}
