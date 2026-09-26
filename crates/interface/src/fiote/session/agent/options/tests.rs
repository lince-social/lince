use super::*;

#[test]
fn choices_keep_agent_values_and_group_labels() {
    assert_eq!(
        choices(&json!({"type":"select","options":[
            {"group":"models","name":"Available","options":[{"value":"model-id","name":"Model name"}]}
        ]})),
        vec![("Available · Model name".into(), Value::from("model-id"))]
    );
    assert_eq!(
        choices(&json!({"type":"boolean"})),
        vec![("Off".into(), false.into()), ("On".into(), true.into())]
    );
    assert!(choices(&json!({"type":"unknown"})).is_empty());
}

#[test]
fn settings_show_supported_controls_and_explain_missing_ones() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let owner = world.spawn_empty().id();
    let saved: FioteStatus = serde_json::from_value(json!({
        "session":null,"behavior":{"prompt_parent":null,"run_assigned":false},"instructions":[],"instruction_error":null,
        "fiotes":[],"tasks":[],"agent":null,"agent_info":{"configOptions":[
            {"id":"model","name":"Model","category":"model","type":"select","currentValue":"a","options":[{"value":"a","name":"Model A"},{"value":"b","name":"Model B"}]},
            {"id":"speed","name":"Speed","type":"select","currentValue":"normal","options":[{"value":"normal","name":"Normal"},{"value":"fast","name":"Fast"}]}
        ]},"agent_activity":[],"record":"test","settings":{"enabled":true,"provider":"","model":"","endpoint":"","directory":""},
        "has_key":false,"running":[],"vault_exists":false,"locked":false,
        "login_url":null,"login_pending":false,"providers":[],"requires_credential":false,"tool_connections":[]
    })).unwrap();
    show(&mut world, owner, owner, Some(&saved));
    let text: Vec<_> = world
        .query::<&Text>()
        .iter(&world)
        .map(|text| text.0.clone())
        .collect();
    assert!(text.iter().any(|text| text == "Model A"));
    assert!(text.iter().any(|text| text == "Fast"));
    assert!(
        text.iter()
            .any(|text| text == "Thinking level: not offered by this agent.")
    );
    assert!(
        !text
            .iter()
            .any(|text| text == "Fast / normal: not offered by this agent.")
    );
    assert_eq!(
        world
            .query::<&crate::dropdown::Dropdown>()
            .iter(&world)
            .count(),
        2
    );
}
