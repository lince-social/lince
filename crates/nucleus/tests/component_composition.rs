use nucleus::{
    component::{ComponentState, Composition, Document, EventBinding, Part},
    karma::rule_field::RuleConsequence,
};
use serde_json::{Value, json};

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

fn only_part(draft: &mut Composition) -> Result<&mut Part, String> {
    let [part] = draft.parts.as_mut_slice() else {
        return Err("Expected one part in the fixture".into());
    };
    Ok(part)
}

#[test]
fn shared_contract_checks_ids_actions_geometry_depth_and_document_size() -> Result<(), String> {
    let mut draft = composition();
    assert_eq!(
        Document::decode(&Document::encode(draft.clone())?)?.composition,
        draft
    );
    let duplicate = only_part(&mut draft)?.clone();
    draft.parts.push(duplicate);
    assert!(draft.validate().is_err());
    draft = composition();
    only_part(&mut draft)?.size = [0, 480];
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
    only_part(&mut draft)?.component = ComponentState::Button {
        label: "Add a Fact".into(),
        action: json!({"action":"capture-entry","target":"r_example","amount":"42"}),
    };
    draft.validate()?;
    only_part(&mut draft)?.events.push(EventBinding {
        event: "click".into(),
        action: json!({"action": false}),
    });
    assert!(draft.validate().is_err());
    only_part(&mut draft)?.events.clear();
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
    Ok(())
}

#[test]
fn activation_has_an_explicit_roundtrip_and_is_an_outward_effect() -> Result<(), String> {
    let parsed = RuleConsequence::parse("@reviewer: activate-fiote")?;
    assert_eq!(parsed.as_text(), "@reviewer: activate-fiote");
    let [consequence] = parsed.consequences.as_slice() else {
        return Err("Expected one activation consequence".into());
    };
    assert_eq!(consequence.kind(), "activate-fiote");
    assert!(consequence.is_outward());
    assert!(!consequence.moves_quantity());
    Ok(())
}

#[test]
fn part_settings_roundtrip_and_reject_unbounded_or_structured_values() -> Result<(), String> {
    let mut draft = composition();
    only_part(&mut draft)?.settings.extend([
        ("wrap".into(), json!(false)),
        ("overflow".into(), json!("Grow")),
    ]);
    assert_eq!(
        Document::decode(&Document::encode(draft.clone())?)?.composition,
        draft
    );
    for value in [
        Value::Null,
        json!({"action":"delete-record"}),
        json!([]),
        json!("x".repeat(4097)),
    ] {
        only_part(&mut draft)?.settings.insert("bad".into(), value);
        assert!(draft.validate().is_err());
    }
    Ok(())
}
