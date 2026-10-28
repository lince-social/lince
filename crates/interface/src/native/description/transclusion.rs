use super::*;

pub const MAX_DEPTH: usize = 6;
pub const MAX_VIEWS: usize = 32;
pub const MAX_BODY: usize = 16 * 1024;

#[derive(Component)]
struct Transclusion {
    context: Context,
    reference: String,
    row: Option<serde_json::Value>,
}

pub(super) fn is_view(world: &World, entity: Entity) -> bool {
    world.get::<Transclusion>(entity).is_some()
}

fn root(world: &World, mut entity: Entity) -> Entity {
    while let Some(parent) = world.get::<ChildOf>(entity) {
        if world
            .get::<crate::native::canvas::CanvasItem>(entity)
            .is_some()
        {
            break;
        }
        entity = parent.parent();
    }
    entity
}

fn ancestors(world: &World, mut entity: Entity) -> Vec<String> {
    let mut uids = Vec::new();
    loop {
        if let Some(view) = world.get::<Transclusion>(entity) {
            uids.push(
                view.row
                    .as_ref()
                    .and_then(|row| row["uid"].as_str())
                    .unwrap_or(&view.reference)
                    .into(),
            );
        }
        if let Some(binding) = world.get::<crate::native::protein_area::RecordBinding>(entity)
            && !uids.contains(&binding.uid)
        {
            uids.push(binding.uid.clone());
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            break;
        };
        entity = parent.parent();
    }
    uids
}

pub(super) fn spawn(world: &mut World, parent: Entity, reference: &str, context: &Context) {
    let root = root(world, parent);
    let count = world
        .query::<(Entity, &Transclusion)>()
        .iter(world)
        .filter(|(entity, _)| self::root(world, *entity) == root)
        .count();
    let path = ancestors(world, parent);
    if count >= MAX_VIEWS
        || path.len() >= MAX_DEPTH
        || reference.len() > 8192
        || reference.is_empty()
    {
        crate::native::edit_mode::label(world, parent, "Transclusion limit reached.", 14.0);
        if reference.len() <= 8192 && !reference.is_empty() {
            super::button(
                world,
                parent,
                context.owner,
                "Open linked source",
                Link {
                    reference: reference.into(),
                    context: context.clone(),
                },
            );
        }
        return;
    }
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            Rendered,
            Transclusion {
                context: context.clone(),
                reference: reference.into(),
                row: None,
            },
            ChildOf(parent),
        ))
        .id();
    crate::native::edit_mode::label(world, entity, "Loading transclusion…", 14.0);
    let mut cursor = Some(context.owner);
    let mut scoped = false;
    while let Some(owner) = cursor {
        scoped |= world
            .get::<crate::native::source_view::ReferenceBase>(owner)
            .is_some()
            || world
                .get::<crate::native::protein_area::RecordBinding>(owner)
                .is_some();
        cursor = world.get::<ChildOf>(owner).map(ChildOf::parent);
    }
    if scoped
        || lince_resources::reference::record_uid(reference.trim_start_matches('@'))
        || lince_resources::reference::is_uri(reference)
        || reference.contains('/')
        || reference.contains(".lingua")
        || reference.starts_with('#')
    {
        crate::native::source_view::embed_reference(world, entity, reference, context);
    } else {
        super::live::watch(world, entity, reference, context.source.clone());
    }
}

pub(super) fn received(world: &mut World, entity: Entity, row: Option<&serde_json::Value>) {
    let Some(view) = world.get::<Transclusion>(entity) else {
        return;
    };
    if view.row.as_ref() == row && row.is_some() {
        return;
    }
    let mut context = view.context.clone();
    context.owner = entity;
    world.get_mut::<Transclusion>(entity).unwrap().row = row.cloned();
    if let Some(children) = world.get::<Children>(entity) {
        let children = children.to_vec();
        for child in children {
            world.despawn(child);
        }
    }
    let Some(row) = row else {
        crate::native::edit_mode::label(
            world,
            entity,
            "Record unavailable or access removed.",
            14.0,
        );
        return;
    };
    if let Some(base) = row["base"].as_str() {
        world
            .entity_mut(entity)
            .insert(crate::native::source_view::ReferenceBase(base.into()));
    }
    let uid = row["uid"].as_str().unwrap_or_default();
    let parent = world.get::<ChildOf>(entity).unwrap().parent();
    if ancestors(world, parent)
        .iter()
        .any(|ancestor| ancestor == uid)
    {
        crate::native::edit_mode::label(world, entity, "Transclusion cycle stopped.", 14.0);
        super::button(
            world,
            entity,
            entity,
            "Open linked source",
            Link {
                reference: row["uri"].as_str().unwrap_or(uid).into(),
                context,
            },
        );
        return;
    }
    let Some(body) = row["body"].as_str() else {
        crate::native::edit_mode::label(world, entity, "Description unavailable.", 14.0);
        return;
    };
    if engine::description_assets::description_is_locked(body) {
        crate::native::edit_mode::label(world, entity, "Description is locked.", 14.0);
        return;
    }
    if body.len() > MAX_BODY {
        crate::native::edit_mode::label(
            world,
            entity,
            "Description exceeds the transclusion size limit.",
            14.0,
        );
        return;
    }
    let title = row["head"]
        .as_str()
        .or_else(|| row["title"].as_str())
        .unwrap_or(uid)
        .chars()
        .take(256)
        .collect::<String>();
    let resource =
        serde_json::from_value::<lince_resources::Resource>(row["resource"].clone()).ok();
    if let Some(resource) = &resource {
        crate::native::source_view::embedded_media(world, entity, resource);
    } else {
        super::button(
            world,
            entity,
            entity,
            &format!("{title} · Open Record"),
            Link {
                reference: row["uri"].as_str().unwrap_or(uid).into(),
                context: context.clone(),
            },
        );
    }
    super::spawn(world, entity, body, context);
    if resource.is_none() && row["base"].is_null() {
        crate::native::source_view::attach(world, entity, uid, view_source(world, entity));
    }
}

fn view_source(world: &World, entity: Entity) -> Source {
    world
        .get::<Transclusion>(entity)
        .unwrap()
        .context
        .source
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (World, Entity, Context) {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let root = world.spawn(Node::default()).id();
        let context = Context {
            owner: root,
            source: Source::Local,
        };
        (world, root, context)
    }

    fn texts(world: &mut World) -> String {
        let mut texts = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for span in world.query::<&TextSpan>().iter(world) {
            texts.push_str(&span.0);
        }
        texts
    }

    #[test]
    fn live_updates_replace_contents_and_revocation_removes_the_previous_body() {
        let (mut world, root, context) = world();
        spawn(&mut world, root, "included", &context);
        let entity = world
            .query_filtered::<Entity, With<Transclusion>>()
            .single(&world)
            .unwrap();
        let uid = nucleus::new_uid("r");
        received(
            &mut world,
            entity,
            Some(&serde_json::json!({"uid":uid,"head":"Included","body":"First secret"})),
        );
        assert!(texts(&mut world).contains("First secret"));
        received(
            &mut world,
            entity,
            Some(&serde_json::json!({"uid":uid,"head":"Included","body":"Updated secret"})),
        );
        assert!(!texts(&mut world).contains("First secret"));
        assert!(texts(&mut world).contains("Updated secret"));
        assert_eq!(world.query::<&EditableText>().iter(&world).count(), 0);
        received(&mut world, entity, None);
        assert!(!texts(&mut world).contains("Updated secret"));
        assert!(texts(&mut world).contains("access removed"));
    }

    #[test]
    fn cycles_depth_view_count_and_body_size_are_bounded() {
        let (mut world, root, context) = world();
        spawn(&mut world, root, "first", &context);
        let first = world
            .query_filtered::<Entity, With<Transclusion>>()
            .single(&world)
            .unwrap();
        let uid = nucleus::new_uid("r");
        received(
            &mut world,
            first,
            Some(&serde_json::json!({"uid":uid,"head":"First","body":"![[first]]"})),
        );
        let child = world
            .query_filtered::<Entity, With<Transclusion>>()
            .iter(&world)
            .find(|entity| *entity != first)
            .unwrap();
        received(
            &mut world,
            child,
            Some(&serde_json::json!({"uid":uid,"head":"First","body":"![[first]]"})),
        );
        assert!(texts(&mut world).contains("cycle stopped"));
        received(
            &mut world,
            first,
            Some(&serde_json::json!({"uid":uid,"body":"x".repeat(MAX_BODY + 1)})),
        );
        assert!(texts(&mut world).contains("size limit"));
        received(
            &mut world,
            first,
            Some(&serde_json::json!({"uid":uid,"body":"![[next]]"})),
        );
        for index in 0..MAX_DEPTH + 2 {
            let pending = world
                .query::<(Entity, &Transclusion)>()
                .iter(&world)
                .find(|(_, view)| view.row.is_none())
                .map(|(entity, _)| entity);
            let Some(pending) = pending else {
                break;
            };
            received(
                &mut world,
                pending,
                Some(
                    &serde_json::json!({"uid":nucleus::new_uid("r"),"body":format!("![[next{index}]]")}),
                ),
            );
        }
        assert!(world.query::<&Transclusion>().iter(&world).count() <= MAX_DEPTH);
        assert!(texts(&mut world).contains("limit reached"));
        for _ in 0..MAX_VIEWS + 8 {
            spawn(&mut world, root, "additional", &context);
        }
        assert_eq!(
            world.query::<&Transclusion>().iter(&world).count(),
            MAX_VIEWS
        );
    }
}

pub(super) fn external(world: &mut World, entity: Entity, data: &serde_json::Value) {
    let fragment = data["fragment"].as_str();
    let row = data["records"].as_array().and_then(|records| {
        if let Some(fragment) = fragment {
            records.iter().find(|record| record["uid"] == fragment)
        } else if records.len() == 1 {
            records.first()
        } else {
            None
        }
    });
    received(world, entity, row);
    if row.is_none()
        && let Some(view) = world.get::<Transclusion>(entity)
    {
        let reference = view.reference.clone();
        let context = view.context.clone();
        super::button(
            world,
            entity,
            entity,
            "Open linked document",
            Link { reference, context },
        );
    }
}
