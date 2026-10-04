use super::*;
use crate::sand_panel::tests::app;

fn package(execution: &str) -> Package {
    let mut package = Package {
        format: model::FORMAT.into(),
        manifest: model::Manifest {
            identity: Identity {
                origin: nucleus::new_uid("r"),
                id: nucleus::new_uid("r"),
                version: 1,
            },
            name: "Unavailable Castle".into(),
            kind: model::Kind::Castle,
            author: nucleus::new_uid("r"),
            execution: execution.into(),
            permissions: vec!["record:read".into()],
            licenses: vec![model::License {
                name: "Example license".into(),
                text: "Keep attribution".into(),
            }],
            credits: vec!["Example Author".into()],
            key_id: "key1".into(),
            public_key: "key".into(),
        },
        payload: "opaque source".into(),
        digest: String::new(),
        signature: "signature".into(),
    };
    package.digest = package.content_digest().unwrap();
    package
}

#[test]
fn unsupported_packages_keep_visible_metadata_and_offer_receiving_without_execution() {
    let mut app = app();
    let world = app.world_mut();
    let root = world
        .spawn((
            Library::default(),
            State {
                organ: Some(nucleus::new_uid("r")),
                package: Some(package("future-native.v9")),
                loaded: true,
                ..Default::default()
            },
        ))
        .id();
    let parent = world.spawn(Node::default()).id();
    show(world, root, parent);
    let labels: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(labels.iter().any(|text| text.contains("Example Author")));
    assert!(labels.iter().any(|text| text.contains("Keep attribution")));
    assert!(labels.iter().any(|text| text.contains("future-native.v9")));
    assert!(labels.iter().any(|text| text.contains("Origin Organ:")));
    assert!(
        labels
            .iter()
            .any(|text| text.contains("Requested permissions: record:read"))
    );
    assert!(
        labels
            .iter()
            .any(|text| text == "Receive into local library (private)")
    );
    assert!(!labels.iter().any(|text| text == "Add to canvas"));
    assert!(runnable(world.get::<State>(root).unwrap().package.as_ref().unwrap()).is_err());
}

#[test]
fn changing_catalogues_discards_previous_organ_responses() {
    let mut app = app();
    let root = app
        .world_mut()
        .spawn((
            Library::default(),
            State {
                pending: Some((
                    "new-request".into(),
                    model::Command::List {
                        organ: None,
                        offset: 0,
                    },
                )),
                ..Default::default()
            },
        ))
        .id();
    let response = Response::Package {
        package: package("future-native.v9"),
        public: true,
        origin_verified: true,
    };
    update(
        app.world_mut(),
        root,
        &[ServerMessage::ActionOk {
            id: "old-request".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(serde_json::to_value(response).unwrap()),
        }],
    );
    assert!(app.world().get::<State>(root).unwrap().package.is_none());
    assert_eq!(
        app.world()
            .get::<State>(root)
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .0,
        "new-request"
    );
}

#[test]
fn inspecting_a_received_package_does_not_place_it_before_explicit_enable() {
    let mut app = app();
    let world = app.world_mut();
    let mut package = package(model::DECLARATIVE);
    package.manifest.name = "Package note".into();
    package.payload = nucleus::canvas::Document::encode(
        "Package note".into(),
        nucleus::canvas::Component::Builtin {
            state: nucleus::component::ComponentState::Text {
                text: "Received text".into(),
            },
        },
    )
    .unwrap();
    package.manifest.permissions = model::required_permissions(&package.payload).unwrap();
    package.digest = package.content_digest().unwrap();
    let identity = package.manifest.identity.clone();
    let root = world
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
            Library::default(),
            State {
                loaded: true,
                pending: Some((
                    "inspect".into(),
                    model::Command::Inspect {
                        organ: None,
                        identity: identity.clone(),
                    },
                )),
                ..Default::default()
            },
        ))
        .id();
    let response = Response::Package {
        package,
        public: false,
        origin_verified: true,
    };
    let data = serde_json::to_value(response).unwrap();
    update(
        world,
        root,
        &[ServerMessage::ActionOk {
            id: "inspect".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(data.clone()),
        }],
    );
    assert_eq!(
        world
            .query::<&crate::sand_store::StoredSand>()
            .iter(world)
            .count(),
        0
    );
    world.get_mut::<State>(root).unwrap().pending =
        Some(("enable".into(), model::Command::Enable { identity }));
    update(
        world,
        root,
        &[ServerMessage::ActionOk {
            id: "enable".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(data),
        }],
    );
    assert_eq!(
        world
            .query::<&crate::sand_store::StoredSand>()
            .iter(world)
            .count(),
        1
    );
}

#[test]
fn switching_sources_discards_pending_enable_even_when_the_new_request_cannot_send() {
    let mut app = app();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins((
        crate::workspace::WorkspacePlugin,
        crate::edit_mode::EditModePlugin,
    ));
    let old = package(model::DECLARATIVE);
    let identity = old.manifest.identity.clone();
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            Library::default(),
            State {
                pending: Some(("old-enable".into(), model::Command::Enable { identity })),
                ..Default::default()
            },
        ))
        .id();
    app.update();
    crate::edit_mode::EditAction::Open.apply(app.world_mut(), root);
    let next = nucleus::new_uid("r");
    Command::Source(Some(next.clone())).apply(app.world_mut(), root);
    assert!(app.world().get::<State>(root).unwrap().pending.is_none());
    update(
        app.world_mut(),
        root,
        &[ServerMessage::ActionOk {
            id: "old-enable".into(),
            created: None,
            facts: 0,
            warnings: vec![],
            data: Some(
                serde_json::to_value(Response::Package {
                    package: old,
                    public: false,
                    origin_verified: true,
                })
                .unwrap(),
            ),
        }],
    );
    let state = app.world().get::<State>(root).unwrap();
    assert_eq!(state.organ.as_deref(), Some(next.as_str()));
    assert!(state.package.is_none());
}
