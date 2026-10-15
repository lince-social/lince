use super::*;

pub(super) fn create(world: &mut World, room: Entity, role: model::Role) -> Result<Entity, String> {
    let (root, workspace) = scope(world, room).ok_or("Select a placed room.")?;
    if role == model::Role::Content {
        return Err("Choose a floor, wall, desk or computer anchor.".into());
    }
    let room_id = world
        .get::<Identity>(room)
        .ok_or("The room identity is unavailable.")?
        .0
        .clone();
    let count = world
        .query::<(&Member, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(_, parent, member)| parent.parent() == root && member.0 == workspace)
        .count();
    if count >= model::MAX_MEMBERS {
        return Err("A workspace supports at most 256 room members.".into());
    }
    let size = world
        .get::<crate::native::canvas::CanvasItem>(room)
        .ok_or("The room surface is unavailable.")?
        .size;
    let width = size.x.max(32.0);
    let depth = size.y.max(32.0);
    let (size, position, upright) = match role {
        model::Role::Floor => ([width, depth], [0.0, 0.0, 0.0], false),
        model::Role::Wall => ([width, 300.0], [0.0, 150.0, -f64::from(depth) * 0.5], true),
        model::Role::Desk => (
            [300.0, 160.0],
            [f64::from(width) * 0.25, 75.0, f64::from(depth) * 0.2],
            false,
        ),
        model::Role::Computer => (
            [100.0, 70.0],
            [f64::from(width) * 0.25, 140.0, f64::from(depth) * 0.2],
            true,
        ),
        model::Role::Content => unreachable!(),
    };
    let local = api::Pose {
        position,
        rotation: if upright {
            DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2).to_array()
        } else {
            model::identity_pose().rotation
        },
    };
    let id = nucleus::new_uid("placement");
    let text = crate::native::sand_text::SavedText {
        area: crate::native::sand_text::SandText {
            size: [size[0].min(216.0), 24.0],
            ..crate::native::sand_text::SandText::new(true)
        },
        text: format!("{role:?}"),
    };
    let mut metadata = api::Metadata::default();
    metadata.room_member = Some(model::Member {
        room: room_id,
        parent: None,
        local,
        role,
    });
    metadata.spatial.depth = Some(4.0);
    metadata.spatial.world_pinned = true;
    metadata.material = Some(nucleus::canvas::appearance::Material {
        lit: true,
        roughness: 0.8,
        color: if role == model::Role::Floor {
            [0.28, 0.3, 0.34, 0.12]
        } else if role == model::Role::Wall {
            [0.45, 0.48, 0.52, 0.12]
        } else {
            [0.45, 0.48, 0.52, 1.0]
        },
        texture: if role == model::Role::Floor {
            nucleus::canvas::appearance::Texture::Tiles
        } else {
            nucleus::canvas::appearance::Texture::Plain
        },
        repeat: [4.0, 3.0],
        ..default()
    });
    let receipt = transact(
        world,
        root,
        api::Mutation::AddWithMetadata {
            placement: id.clone(),
            workspace,
            component: api::Component::Native {
                kind: "square".into(),
                settings: std::collections::BTreeMap::from([(
                    "texts".into(),
                    serde_json::Value::String(
                        serde_json::to_string(&[text]).map_err(|error| error.to_string())?,
                    ),
                )]),
                bindings: Vec::new(),
            },
            geometry: api::Geometry {
                position: world
                    .get::<crate::native::canvas::CanvasItem>(room)
                    .unwrap()
                    .position
                    .to_array(),
                size: size.map(f64::from),
            },
            metadata,
        },
    )?;
    let entity = find(world, root, &id).ok_or("The proxy placement was not created.")?;
    if world.get::<History>(root).is_none() {
        world.entity_mut(root).insert(History::default());
    }
    let mut history = world.get_mut::<History>(root).unwrap();
    history.0.push(vec![receipt]);
    if history.0.len() > 8 {
        history.0.remove(0);
    }
    if let Some(mut inspection) = world.get_mut::<crate::native::inspection::Inspection>(root) {
        inspection.selected = Some(entity);
    }
    Ok(entity)
}
