use super::{TestResult, attached, hierarchy, member, placement, room, roundtrip, task};
use crate::{
    canvas::{
        Pinned, Placement,
        room::{MAX_DEPTH, MAX_MEMBERS, MAX_ROOMS, Role, Room, validate},
    },
    new_uid,
};

fn anchors(placements: &mut [Placement]) -> Result<(&mut Placement, &mut Placement), String> {
    match placements {
        [_, desk, computer, ..] => Ok((desk, computer)),
        _ => Err("The Room fixture is missing its desk or computer.".into()),
    }
}

#[test]
fn room_hierarchy_roundtrip_preserves_local_poses_and_rejects_invalid_metadata() -> TestResult {
    let items = hierarchy();
    validate(&items, true)?;
    let restored = roundtrip(&items)?;
    assert_eq!(restored, items);
    let mut metadata = room(&restored).metadata.clone();
    metadata
        .room
        .as_mut()
        .expect("the fixture has a Room")
        .home
        .rotation = [0.0; 4];
    assert!(!metadata.validate());
    metadata = task(&restored).metadata.clone();
    let [x, ..] = &mut member(&mut metadata).local.position;
    *x = f64::NAN;
    assert!(!metadata.validate());
    metadata = room(&restored).metadata.clone();
    metadata.pinned = Some(Pinned {
        anchor: [0.5; 2],
        scale: 1.0,
    });
    assert!(!metadata.validate());
    metadata = room(&restored).metadata.clone();
    metadata.room_member = task(&restored).metadata.room_member.clone();
    assert!(!metadata.validate());
    metadata = task(&restored).metadata.clone();
    metadata.follow_slope = true;
    assert!(!metadata.validate());
    Ok(())
}

#[test]
fn room_hierarchy_rejects_cycles_foreign_workspaces_and_foreign_rooms() -> TestResult {
    let original = hierarchy();
    let mut items = original.clone();
    let (desk, computer) = anchors(&mut items)?;
    member(&mut desk.metadata).parent = Some(computer.id.clone());
    assert!(validate(&items, true).unwrap_err().contains("acyclic"));
    items = original.clone();
    let (_, computer) = anchors(&mut items)?;
    computer.workspace = 2;
    assert!(validate(&items, true).is_err());
    items = original.clone();
    let mut other = placement();
    other.metadata.room = Some(Room::default());
    let (_, computer) = anchors(&mut items)?;
    member(&mut computer.metadata).room = other.id.clone();
    items.push(other);
    assert!(validate(&items, true).is_err());
    items = original;
    let (desk, _) = anchors(&mut items)?;
    member(&mut desk.metadata).role = Role::Content;
    assert!(validate(&items, true).is_err());
    Ok(())
}

#[test]
fn room_inspection_pages_can_omit_parents_but_complete_state_cannot() -> TestResult {
    let items = hierarchy();
    let page = std::slice::from_ref(task(&items));
    validate(page, false)?;
    assert!(validate(page, true).unwrap_err().contains("missing"));
    let mut items = items;
    let (_, computer) = anchors(&mut items)?;
    member(&mut computer.metadata).parent = Some(new_uid("placement"));
    assert!(validate(&items, true).unwrap_err().contains("anchor"));
    Ok(())
}

#[test]
fn room_limits_bound_parent_depth_rooms_and_members() {
    let mut items = hierarchy();
    let room_id = room(&items).id.clone();
    for _ in 0..MAX_DEPTH {
        let parent = items
            .last_mut()
            .expect("the fixture has a nested placement");
        member(&mut parent.metadata).role = Role::Desk;
        let child = attached(&room_id, Some(&parent.id), Role::Desk, [0.0; 3]);
        items.push(child);
    }
    assert!(validate(&items, true).unwrap_err().contains("eight"));
    let mut items = Vec::new();
    for _ in 0..=MAX_ROOMS {
        let mut root = placement();
        root.metadata.room = Some(Room::default());
        items.push(root);
    }
    assert!(validate(&items, true).unwrap_err().contains("sixteen"));
    items.truncate(1);
    let room_id = room(&items).id.clone();
    items.extend((0..=MAX_MEMBERS).map(|_| attached(&room_id, None, Role::Content, [0.0; 3])));
    assert!(validate(&items, true).unwrap_err().contains("256"));
}
