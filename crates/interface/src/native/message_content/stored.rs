use super::{RecordBinding, StoredPart, attachments};
use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

struct Snapshot {
    binding: RecordBinding,
    uid: String,
    content: Value,
}

impl Snapshot {
    fn same_identity(&self, binding: &RecordBinding, uid: &str) -> bool {
        self.uid == uid
            && self.binding.area == binding.area
            && self.binding.uid == binding.uid
            && self.binding.source == binding.source
    }
}

#[derive(Component)]
struct Source(Arc<Snapshot>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentRef {
    owner: Entity,
    part: usize,
}

impl ContentRef {
    pub(crate) fn update(
        self,
        world: &mut World,
        owner: Entity,
        edit: impl FnOnce(&mut Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let _preparation = nucleus::performance::Span::new("message-content-action-preparation");
        let source = world
            .get::<Source>(self.owner)
            .ok_or("This message content is no longer available.")?
            .0
            .clone();
        if source
            .content
            .get("parts")
            .and_then(Value::as_array)
            .and_then(|parts| parts.get(self.part))
            .is_none()
        {
            return Err("This message part is no longer available.".into());
        }
        let mut content = source.content.clone();
        nucleus::performance::add("message-content-replacements-prepared", 1);
        edit(&mut content["parts"][self.part])?;
        crate::native::protein_area::execute(
            world,
            &source.binding,
            owner,
            engine::actions::Action::SetExtension {
                target: source.uid.clone(),
                namespace: "lince.message-content".into(),
                fds: content,
            },
        )
    }
}

#[derive(Component, Default)]
struct Rows {
    children: crate::native::retained::Children<usize>,
    error: Option<Entity>,
}

#[derive(Component)]
struct Rendered {
    snapshot: Arc<Snapshot>,
    expired: bool,
}

pub(super) fn matches(
    world: &World,
    parent: Entity,
    binding: &RecordBinding,
    data: &Value,
) -> bool {
    world.get::<Source>(parent).is_some_and(|source| {
        data["uid"]
            .as_str()
            .is_some_and(|uid| source.0.same_identity(binding, uid))
            && nucleus::json::same_value_encoding(&source.0.content, &data["content"])
    })
}

pub(super) fn refresh(world: &mut World, parent: Entity, binding: &RecordBinding, data: &Value) {
    let Some(uid) = data["uid"].as_str() else {
        return;
    };
    let _presentation = nucleus::performance::Span::new("message-stored-content-presentation");
    if world
        .get::<Source>(parent)
        .is_some_and(|source| !source.0.same_identity(binding, uid))
    {
        world.entity_mut(parent).despawn_children();
        world.entity_mut(parent).remove::<Rows>();
    }
    let snapshot = Arc::new(Snapshot {
        binding: binding.clone(),
        uid: uid.into(),
        content: data["content"].clone(),
    });
    world.entity_mut(parent).insert(Source(snapshot.clone()));
    nucleus::performance::add("message-content-snapshots-prepared", 1);
    let (parts, invalid) = if snapshot.content.is_null() {
        (Vec::new(), false)
    } else {
        match Vec::<StoredPart>::deserialize(&snapshot.content["parts"]) {
            Ok(parts) => (parts, false),
            Err(_) => (Vec::new(), true),
        }
    };
    let mut rows = world.entity_mut(parent).take::<Rows>().unwrap_or_default();
    rows.children.reconcile(
        world,
        parent,
        parts.iter().take(nucleus::message::MAX_PARTS).enumerate().map(|(index, part)| {
            let expired = matches!(part, StoredPart::Question { question }
                if question.state == nucleus::question::State::Pending
                    && question.expires_ms.is_some_and(|expires| nucleus::operation::now_ms() >= expires));
            (index, (ContentRef { owner: parent, part: index }, part, expired))
        }),
        |world, parent, &(content, part, expired)| {
            let row = world.spawn((Node { flex_direction: FlexDirection::Column, ..default() }, ChildOf(parent))).id();
            render(world, row, content, part, expired, &snapshot);
            row
        },
        |world, row, &(content, part, expired)| render(world, row, content, part, expired, &snapshot),
    );
    let error = rows.error.filter(|entity| {
        world
            .get::<ChildOf>(*entity)
            .is_some_and(|child| child.parent() == parent)
    });
    rows.error = if invalid {
        Some(error.unwrap_or_else(|| {
            crate::native::edit_mode::label(
                world,
                parent,
                "Message contents are unavailable or use an unsupported format.",
                13.0,
            )
        }))
    } else {
        if let Some(error) = error {
            world.despawn(error);
        }
        None
    };
    world.entity_mut(parent).insert(rows);
}

fn render(
    world: &mut World,
    row: Entity,
    content: ContentRef,
    part: &StoredPart,
    expired: bool,
    snapshot: &Arc<Snapshot>,
) {
    if let Some(mut rendered) = world.get_mut::<Rendered>(row)
        && rendered.expired == expired
        && nucleus::json::same_value_encoding(
            &rendered.snapshot.content["parts"][content.part],
            &snapshot.content["parts"][content.part],
        )
    {
        rendered.snapshot = snapshot.clone();
        return;
    }
    let retain_steps = matches!(part, StoredPart::Steps { .. })
        && world
            .get::<Rendered>(row)
            .is_some_and(|old| old.snapshot.content["parts"][content.part]["kind"] == "steps");
    if !retain_steps {
        world.entity_mut(row).despawn_children();
        crate::native::message_progress::clear_steps(world, row);
    }
    nucleus::performance::add("message-stored-parts-rendered", 1);
    match part {
        StoredPart::Question { question } => {
            crate::native::message_questions::view(world, row, content, question, expired)
        }
        StoredPart::Steps { steps } => {
            crate::native::message_progress::steps(world, row, content, steps)
        }
        StoredPart::Text { text } => {
            crate::native::edit_mode::label(world, row, text, 14.0);
        }
        StoredPart::Reference { name, uri } => {
            crate::native::edit_mode::label(world, row, &format!("{name}\nReference: {uri}"), 13.0);
            if let Some(record) = uri.strip_prefix("record:") {
                crate::native::description::button(
                    world,
                    row,
                    row,
                    "Open Record",
                    crate::native::full_record::Open(RecordBinding {
                        uid: record.into(),
                        ..snapshot.binding.clone()
                    }),
                );
            }
        }
        attachment => attachments::create(
            world,
            row,
            &snapshot.binding,
            &snapshot.uid,
            content.part,
            attachment.clone(),
        ),
    }
    world.entity_mut(row).insert(Rendered {
        snapshot: snapshot.clone(),
        expired,
    });
}
