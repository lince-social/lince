use super::*;
use nucleus::plugins::{Manifest, Package};

fn catalog() -> Catalog {
    let fields = vec![
        Field {
            id: "server".into(),
            name: "Server".into(),
            kind: FieldType::Text,
            required: true,
            default: json!("local"),
        },
        Field {
            id: "limit".into(),
            name: "Limit".into(),
            kind: FieldType::Integer,
            required: true,
            default: json!(20),
        },
        Field {
            id: "enabled".into(),
            name: "Enabled".into(),
            kind: FieldType::Boolean,
            required: true,
            default: json!(true),
        },
        Field {
            id: "options".into(),
            name: "Options".into(),
            kind: FieldType::Json,
            required: true,
            default: json!({"mode":"notes"}),
        },
    ];
    Catalog {
        packages: vec![Package {
            manifest: Manifest {
                format: nucleus::plugins::FORMAT.into(),
                id: "test".into(),
                name: "Test package".into(),
                version: 1,
                compatibility: nucleus::plugins::Compatibility::native(),
                executable: "runner".into(),
                arguments: Vec::new(),
                contributions: vec![
                    Contribution::Sync {
                        id: "feed".into(),
                        name: "Test feed".into(),
                        settings: fields,
                        incoming: true,
                        outgoing: true,
                        replay_safe: true,
                        schema: "note".into(),
                        document: None,
                    },
                    Contribution::Schema {
                        id: "note".into(),
                        name: "Test note".into(),
                        namespace: "plugin.test.note".into(),
                        fields: Vec::new(),
                    },
                    Contribution::Action {
                        id: "hello".into(),
                        name: "Hello".into(),
                        arguments: vec![
                            Field {
                                id: "name".into(),
                                name: "Name".into(),
                                kind: FieldType::Text,
                                required: true,
                                default: json!("Lince"),
                            },
                            Field {
                                id: "optional".into(),
                                name: "Optional number".into(),
                                kind: FieldType::Integer,
                                required: false,
                                default: Value::Null,
                            },
                        ],
                    },
                ],
                licenses: vec![nucleus::sand_package::License {
                    name: "Test license".into(),
                    text: "License text".into(),
                }],
                credits: vec!["Author".into()],
            },
            digest: "digest".into(),
            enabled: true,
        }],
        diagnostics: Vec::new(),
        bindings: Vec::new(),
        proteins: vec![("selection".into(), "Selected Records".into())],
        deliveries: Vec::new(),
    }
}

#[test]
fn generic_forms_keep_edits_when_status_refreshes_and_validate_field_types() {
    let mut app = panel::tests::app();
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    populate(world, parent);
    let owner = world
        .query_filtered::<Entity, With<View>>()
        .single(world)
        .unwrap();
    render(world, owner, catalog());
    open(world, owner, "test/feed", None);
    Command::Direction(Direction::Both).apply(world, owner);
    Command::Protein("selection".into()).apply(world, owner);
    let (settings, binding) = snapshot(world, owner).unwrap();
    assert_eq!(
        settings,
        json!({"server":"local","limit":20,"enabled":true,"options":{"mode":"notes"}})
    );
    let binding = binding.unwrap();
    assert_eq!(binding.direction, Direction::Both);
    assert_eq!(binding.protein_uid.as_deref(), Some("selection"));
    let limit = world
        .get::<View>(owner)
        .unwrap()
        .form
        .as_ref()
        .unwrap()
        .fields[1]
        .1;
    world
        .get_mut::<bevy::text::EditableText>(limit)
        .unwrap()
        .editor
        .set_text("30");
    render(world, owner, catalog());
    assert_eq!(snapshot(world, owner).unwrap().0["limit"], 30);
    world
        .get_mut::<bevy::text::EditableText>(limit)
        .unwrap()
        .editor
        .set_text("invalid");
    assert!(snapshot(world, owner).unwrap_err().contains("integer"));
    open(world, owner, "test/hello", None);
    let (arguments, binding) = snapshot(world, owner).unwrap();
    assert_eq!(arguments, json!({"name":"Lince"}));
    assert!(binding.is_none());
    let mut binding = snapshot_binding();
    binding.incoming_filter = json!([{"all":[{"text_contains":"rust"},{"kind_eq":"plain"}]}]);
    open(world, owner, "test/feed", Some(binding.clone()));
    assert_eq!(
        snapshot(world, owner).unwrap().1.unwrap().incoming_filter,
        binding.incoming_filter
    );
}

fn snapshot_binding() -> Binding {
    Binding {
        uid: nucleus::new_uid("r"),
        provider: "test/feed".into(),
        name: "Saved feed".into(),
        settings: json!({"server":"local","limit":20,"enabled":true,"options":{"mode":"notes"}}),
        protein_uid: None,
        incoming_filter: json!([]),
        direction: Direction::Incoming,
        interval_seconds: 60,
        enabled: false,
        revision: 1,
    }
}

#[test]
fn document_forms_have_independent_paths_and_no_receive_filter() {
    let mut app = panel::tests::app();
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    populate(world, parent);
    let owner = world
        .query_filtered::<Entity, With<View>>()
        .single(world)
        .unwrap();
    let mut exports = catalog();
    let Contribution::Sync {
        settings,
        incoming,
        schema,
        document,
        ..
    } = &mut exports.packages[0].manifest.contributions[0]
    else {
        panic!("Missing provider");
    };
    *incoming = false;
    *schema = String::new();
    *document = Some(nucleus::plugins::DocumentFormat {
        source: nucleus::plugins::SnapshotSource::Schedule,
        extension: "ics".into(),
    });
    *settings = [("directory", "/tmp/personal"), ("filename", "schedule.ics")]
        .into_iter()
        .map(|(id, value)| Field {
            id: id.into(),
            name: id.into(),
            kind: FieldType::Text,
            required: true,
            default: json!(value),
        })
        .collect();
    render(world, owner, exports);
    open(world, owner, "test/feed", None);
    let first = snapshot(world, owner).unwrap().1.unwrap();
    assert_eq!(first.direction, Direction::Outgoing);
    assert_eq!(first.settings["directory"], "/tmp/personal");
    assert!(
        world
            .get::<View>(owner)
            .unwrap()
            .form
            .as_ref()
            .unwrap()
            .filter
            .is_none()
    );
    open(world, owner, "test/feed", None);
    let second = snapshot(world, owner).unwrap().1.unwrap();
    assert_ne!(first.uid, second.uid);
}

#[test]
fn public_preview_cannot_be_used_after_settings_change() {
    let mut app = panel::tests::app();
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    populate(world, parent);
    let owner = world
        .query_filtered::<Entity, With<View>>()
        .single(world)
        .unwrap();
    render(world, owner, catalog());
    open(world, owner, "test/feed", None);
    Command::Direction(Direction::Outgoing).apply(world, owner);
    let binding = snapshot(world, owner).unwrap().1.unwrap();
    world
        .get_mut::<View>(owner)
        .unwrap()
        .form
        .as_mut()
        .unwrap()
        .preview = Some((
        serde_json::to_value(binding).unwrap(),
        "preview-token".into(),
    ));
    let server = world
        .get::<View>(owner)
        .unwrap()
        .form
        .as_ref()
        .unwrap()
        .fields[0]
        .1;
    world
        .get_mut::<bevy::text::EditableText>(server)
        .unwrap()
        .editor
        .set_text("changed");
    Command::Save.apply(world, owner);
    let status = world.get::<View>(owner).unwrap().status;
    assert_eq!(
        world.get::<Text>(status).unwrap().0,
        "Settings changed; preview again"
    );
    assert!(world.get::<Job>(owner).is_none());
}

#[test]
fn unsafe_delivery_retries_are_hidden_while_local_resolution_remains_available() {
    let mut app = panel::tests::app();
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    populate(world, parent);
    let owner = world
        .query_filtered::<Entity, With<View>>()
        .single(world)
        .unwrap();
    let mut data = catalog();
    let Contribution::Sync { replay_safe, .. } = &mut data.packages[0].manifest.contributions[0]
    else {
        panic!("Missing provider");
    };
    *replay_safe = false;
    let binding = snapshot_binding();
    data.deliveries.push(nucleus::plugins::Delivery {
        uid: "delivery".into(),
        binding: binding.uid.clone(),
        record: "record".into(),
        state: "uncertain".into(),
        result: "Remote outcome unknown".into(),
    });
    data.bindings.push(nucleus::plugins::BindingStatus {
        binding,
        status: "Paused".into(),
        pending: 0,
        failed: 1,
    });
    render(world, owner, data.clone());
    let captions: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        captions
            .iter()
            .any(|text| text == "Resolve locally without resending")
    );
    assert!(
        !captions
            .iter()
            .any(|text| text == "Retry same prepared publication")
    );
    let Contribution::Sync { replay_safe, .. } = &mut data.packages[0].manifest.contributions[0]
    else {
        panic!("Missing provider");
    };
    *replay_safe = true;
    render(world, owner, data);
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Retry same prepared publication")
    );
}
