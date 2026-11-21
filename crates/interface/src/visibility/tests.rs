use nucleus::visibility::Bound;

use crate::theme::Typography;

use super::{
    commands::apply,
    draft::{
        BoundEnd::Upper,
        Side::{Exclude, Include},
        policy,
    },
    *,
};

#[test]
fn draft_reuses_the_policy_until_an_editor_field_changes() {
    use nucleus::visibility::Condition;

    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    let parent = world.spawn_empty().id();
    let panel = mount_policy_draft(
        &mut world,
        parent,
        Context {
            record_uid: nucleus::new_uid("r"),
            data: Data::Record,
            saved: None,
            organs: Vec::new(),
            persons: Vec::new(),
            records: Vec::new(),
        },
        Policy {
            include: vec![Condition::default()],
            exclude: Vec::new(),
        },
        false,
    );
    let first = edited_policy_shared(&mut world, panel).unwrap();
    let unchanged = edited_policy_shared(&mut world, panel).unwrap();
    assert!(Arc::ptr_eq(&first, &unchanged));

    let field = world
        .get::<Panel>(panel)
        .unwrap()
        .fields
        .iter()
        .find(|(_, _, end, _)| matches!(end, Upper))
        .expect("the draft has an upper-bound input")
        .3;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("3");

    let edited = edited_policy_shared(&mut world, panel).unwrap();
    assert!(!Arc::ptr_eq(&first, &edited));
    assert_eq!(edited.include[0].upper.unwrap().value, 3);
}

#[cfg_attr(test, test)]
fn location_editor_saves_user_area_size_and_rejects_invalid_sizes() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let record = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, Data::Place);
    let id = world
        .get::<Panel>(panel)
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .0
        .clone();
    receive(
        &mut world,
        &id,
        Ok(serde_json::to_value(Context {
            record_uid: record,
            data: Data::Place,
            saved: None,
            organs: vec![],
            persons: vec![],
            records: vec![],
        })
        .unwrap()),
    );
    world.resource_mut::<Messages<Request>>().clear();
    apply(&mut world, panel, Intent::Add(Include));
    let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("2500");
    apply(&mut world, panel, Intent::Save);
    let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
    assert!(
        matches!(&requests[0].command, Command::Save { data: Data::Place, policy, .. } if policy.include[0].radius_metres == Some(2500))
    );
    world.get_mut::<Panel>(panel).unwrap().pending = None;
    let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("99");
    apply(&mut world, panel, Intent::Save);
    assert!(world.resource::<Messages<Request>>().is_empty());
    assert!(world.get::<Panel>(panel).unwrap().notice.contains("100000"));
    let field = world.get::<Panel>(panel).unwrap().radius_fields[0].1;
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor_mut()
        .set_text("");
    apply(&mut world, panel, Intent::Save);
    let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
    assert!(
        matches!(&requests[0].command, Command::Save { policy, .. } if policy.include[0].radius_metres.is_none())
    );
}

#[cfg_attr(test, test)]
fn editor_previews_composed_rules_and_saves_the_loaded_revision() {
    let mut world = World::new();
    world.insert_resource(Typography(Handle::default()));
    world.init_resource::<Messages<Request>>();
    let parent = world.spawn_empty().id();
    let record = nucleus::new_uid("r");
    let organ = nucleus::new_uid("r");
    let person = nucleus::new_uid("r");
    let panel = mount(&mut world, parent, &record, Data::LiveLocation);
    let id = world
        .get::<Panel>(panel)
        .unwrap()
        .pending
        .as_ref()
        .unwrap()
        .0
        .clone();
    receive(
        &mut world,
        &id,
        Ok(serde_json::to_value(Context {
            record_uid: record.clone(),
            data: Data::LiveLocation,
            saved: Some(nucleus::visibility::SavedPolicy {
                controller_uid: String::new(),
                revision: 4,
                policy: Policy::default(),
            }),
            records: vec![],
            persons: vec![nucleus::location::Choice {
                uid: person.clone(),
                label: "Passenger Person".into(),
                node_id: None,
            }],
            organs: vec![nucleus::visibility::Organ {
                uid: organ.clone(),
                label: "Passenger".into(),
                proximity: Some(2),
                blocked: false,
            }],
        })
        .unwrap()),
    );
    world.resource_mut::<Messages<Request>>().clear();
    apply(&mut world, panel, Intent::Add(Include));
    apply(&mut world, panel, Intent::Organ(Include, 0, organ.clone()));
    apply(
        &mut world,
        panel,
        Intent::Person(Include, 0, person.clone()),
    );
    let upper = world
        .get::<Panel>(panel)
        .unwrap()
        .fields
        .iter()
        .find(|(side, _, end, _)| *side == Include && matches!(end, Upper))
        .unwrap()
        .3;
    world
        .get_mut::<bevy::text::EditableText>(upper)
        .unwrap()
        .editor_mut()
        .set_text("3");
    apply(&mut world, panel, Intent::Inclusive(Include, 0, Upper));
    apply(&mut world, panel, Intent::Add(Exclude));
    apply(&mut world, panel, Intent::Organ(Exclude, 0, organ.clone()));
    apply(&mut world, panel, Intent::Preview);
    let previewed = policy(world.get::<Panel>(panel).unwrap()).unwrap();
    assert_eq!(previewed.include[0].persons, vec![person]);
    assert_eq!(
        previewed.include[0].upper,
        Some(Bound {
            value: 3,
            inclusive: false
        })
    );
    assert!(!previewed.decide(&organ, Some(2)).allowed);
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0.contains("Denied by these rules") && text.0.contains("exclude [1]"))
    );
    apply(&mut world, panel, Intent::Save);
    let requests: Vec<_> = world.resource_mut::<Messages<Request>>().drain().collect();
    assert_eq!(requests.len(), 1);
    assert!(
        matches!(&requests[0].command, Command::Save { expected_revision: 4, policy: saved, .. } if *saved == previewed)
    );
    receive(
        &mut world,
        &requests[0].id,
        Err("Visibility changed; refresh before saving".into()),
    );
    assert_eq!(
        policy(world.get::<Panel>(panel).unwrap()).unwrap(),
        previewed
    );
    assert!(
        world
            .get::<Panel>(panel)
            .unwrap()
            .notice
            .contains("refresh")
    );
}

#[cfg(feature = "native-presentation")]
crate::native::laboratory_cases! {
    location_editor_saves_user_area_size_and_rejects_invalid_sizes,
    editor_previews_composed_rules_and_saves_the_loaded_revision,
}
