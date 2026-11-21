use super::*;
#[test]
fn mixed_compositions_share_one_part_and_depth_budget() {
    let builtin = |parts: Vec<crate::component::Part>| ComponentState::Composition {
        composition: crate::component::Composition {
            name: "Builtin".into(),
            parts,
            origin: None,
        },
    };
    let part = |id: usize, component| crate::component::Part {
        id: format!("part-{id}"),
        settings: BTreeMap::new(),
        position: [0, 0],
        size: [100, 100],
        component,
        events: vec![],
    };
    let wrap = |component| Component::Composition {
        composition: Composition {
            name: "Wrapper".into(),
            origin: None,
            parts: vec![Part {
                id: "content".into(),
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [100.0, 100.0],
                },
                component,
                events: vec![],
            }],
        },
    };
    let text = ComponentState::Text {
        text: "hello".into(),
    };
    let full = |count| Component::Builtin {
        state: builtin((0..count).map(|id| part(id, text.clone())).collect()),
    };
    assert!(wrap(full(254)).validate(&[], false).is_ok());
    assert!(wrap(full(255)).validate(&[], false).is_err());
    let mut nested = text;
    for _ in 0..6 {
        nested = builtin(vec![part(0, nested)]);
    }
    let nested = Component::Builtin { state: nested };
    assert!(wrap(nested.clone()).validate(&[], false).is_ok());
    assert!(wrap(wrap(nested)).validate(&[], false).is_err());
}

#[test]
fn canvas_contract_rejects_unsafe_geometry_and_requests() {
    assert!(
        Geometry {
            position: [f64::NAN, 0.0],
            size: [1.0, 1.0]
        }
        .validate()
        .is_err()
    );
    assert!(
        Geometry {
            position: [0.0, 0.0],
            size: [0.0, 1.0]
        }
        .validate()
        .is_err()
    );
    assert!(
        Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 257
        }
        .validate()
        .is_err()
    );
    assert!(
        Request::Mutate {
            request_id: "invented".into(),
            expected_revision: 0,
            mutation: Mutation::Remove {
                placement: crate::new_uid("placement")
            }
        }
        .validate()
        .is_err()
    );
}
#[test]
fn schema_rejects_unknown_settings_and_missing_kinds() {
    let descriptor = Descriptor {
        kind: "clock".into(),
        name: "Clock".into(),
        description: String::new(),
        settings: BTreeMap::from([("running".into(), Setting::Boolean { default: false })]),
        max_bindings: 0,
        placeable: true,
        configurable: true,
        composable: false,
        unavailable_reason: None,
    };
    let mut component = Component::Native {
        kind: "clock".into(),
        settings: BTreeMap::from([("running".into(), serde_json::json!(true))]),
        bindings: vec![],
    };
    assert!(
        component
            .validate(std::slice::from_ref(&descriptor), false)
            .is_ok()
    );
    if let Component::Native { settings, .. } = &mut component {
        settings.insert("shell".into(), serde_json::json!("anything"));
    }
    assert!(component.validate(&[descriptor], false).is_err());
}

#[test]
fn native_setting_defaults_respect_typed_bounds() {
    for (setting, valid) in [
        (
            Setting::Integer {
                default: 1,
                min: 0,
                max: 2,
            },
            true,
        ),
        (
            Setting::Integer {
                default: 1,
                min: 2,
                max: 0,
            },
            false,
        ),
        (
            Setting::Number {
                default: f64::NAN,
                min: 0.0,
                max: 1.0,
            },
            false,
        ),
        (
            Setting::Number {
                default: 0.5,
                min: 0.0,
                max: f64::INFINITY,
            },
            false,
        ),
        (
            Setting::Text {
                default: "é".into(),
                max_bytes: 2,
            },
            true,
        ),
        (
            Setting::Text {
                default: "é".into(),
                max_bytes: 1,
            },
            false,
        ),
        (
            Setting::Choice {
                default: "hidden".into(),
                values: vec!["shown".into()],
            },
            false,
        ),
    ] {
        assert_eq!(setting.validate(), valid);
    }
}
