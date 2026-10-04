use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cell::{ClientMessage, ServerMessage};
use nucleus::description_asset::{Kind, Request, Response};
use std::{collections::HashMap, time::Instant};

#[derive(Component)]
struct AssetView {
    record: String,
    asset: String,
    source: Source,
    row: Option<serde_json::Value>,
    request: Option<String>,
}

enum Target {
    View(Entity),
    Insert {
        panel: Entity,
        input: Entity,
        record: String,
        kind: Kind,
        replace: Option<String>,
    },
}

struct Pending {
    target: Target,
    source: Source,
    started: Instant,
}

#[derive(Resource, Default)]
struct Requests(HashMap<String, Pending>);

pub(super) fn spawn(world: &mut World, parent: Entity, reference: &str, context: &Context) {
    let Some((record, asset)) = nucleus::description_asset::parse_reference(reference) else {
        crate::edit_mode::label(world, parent, "Invalid description asset reference.", 14.0);
        return;
    };
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Rendered,
            AssetView {
                record: record.into(),
                asset: asset.into(),
                source: context.source.clone(),
                row: None,
                request: None,
            },
            ChildOf(parent),
        ))
        .id();
    crate::edit_mode::label(world, entity, "Loading drawing or image…", 14.0);
    super::live::watch(world, entity, record, context.source.clone());
}

fn clear(world: &mut World, entity: Entity) {
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.to_vec() {
            world.despawn(child);
        }
    }
}

fn send(
    world: &mut World,
    source: Source,
    target: Target,
    request: Request,
) -> Result<String, String> {
    world.init_resource::<Requests>();
    if world.resource::<Requests>().0.len() >= 64 {
        return Err("Description assets are busy. Try again.".into());
    }
    let sender =
        super::live::sender(world, &source).ok_or("Connect to the Record's Cell first.")?;
    let id = nucleus::new_uid("asset");
    sender
        .try_send(ClientMessage::DescriptionAsset {
            id: id.clone(),
            request,
        })
        .map_err(|error| error.to_string())?;
    world.resource_mut::<Requests>().0.insert(
        id.clone(),
        Pending {
            target,
            source,
            started: Instant::now(),
        },
    );
    Ok(id)
}

pub(super) fn permission(world: &mut World, entity: Entity, row: Option<&serde_json::Value>) {
    let Some(view) = world.get::<AssetView>(entity) else {
        return;
    };
    if view.row.as_ref() == row && row.is_some() {
        return;
    }
    let (record, asset, source) = (view.record.clone(), view.asset.clone(), view.source.clone());
    world.get_mut::<AssetView>(entity).unwrap().row = row.cloned();
    world.get_mut::<AssetView>(entity).unwrap().request = None;
    clear(world, entity);
    if row
        .and_then(|row| row["body"].as_str())
        .is_none_or(engine::description_assets::description_is_locked)
    {
        crate::edit_mode::label(world, entity, "Asset unavailable or access removed.", 14.0);
        return;
    }
    match send(
        world,
        source,
        Target::View(entity),
        Request::Get { record, asset },
    ) {
        Ok(id) => world.get_mut::<AssetView>(entity).unwrap().request = Some(id),
        Err(error) => {
            crate::edit_mode::label(world, entity, &error, 14.0);
        }
    }
}

pub(super) fn insert(
    world: &mut World,
    panel: Entity,
    input: Entity,
    kind: Kind,
    bytes: Vec<u8>,
    replace: Option<String>,
) -> Result<(), String> {
    if !crate::record_binding::can_edit(world, input) {
        return Err("The description is read only or disconnected.".into());
    }
    let binding = world
        .get::<crate::record_binding::TextBinding>(input)
        .ok_or("Choose a Record description.")?;
    if binding.property != "body" {
        return Err("Choose a Record description.".into());
    }
    let record = binding.record.uid.clone();
    let source = binding.record.source.clone();
    send(
        world,
        source,
        Target::Insert {
            panel,
            input,
            record: record.clone(),
            kind,
            replace,
        },
        Request::Put {
            record,
            kind,
            data_base64: STANDARD.encode(bytes),
        },
    )?;
    Ok(())
}

pub(super) fn append(world: &mut World, input: Entity, reference: &str) -> Result<(), String> {
    if !crate::record_binding::can_edit(world, input) {
        return Err("The description is read only or disconnected.".into());
    }
    let mut text = world
        .get_mut::<EditableText>(input)
        .ok_or("Description was closed.")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish typing or pasting before inserting.".into());
    }
    let next = format!("{}\n\n{reference}\n", text.value());
    if text
        .max_characters
        .is_some_and(|limit| next.chars().count() > limit)
    {
        return Err("Description exceeds its text limit.".into());
    }
    text.editor.set_text(&next);
    Ok(())
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    let (id, result) = match message {
        ServerMessage::DescriptionAsset { id, response } => (id, Ok(response)),
        ServerMessage::Error { id, message, .. } => (id, Err(message.as_str())),
        _ => return,
    };
    if world
        .get_resource::<Requests>()
        .and_then(|requests| requests.0.get(id))
        .is_none_or(|pending| &pending.source != source)
    {
        return;
    }
    let pending = world.resource_mut::<Requests>().0.remove(id).unwrap();
    match pending.target {
        Target::Insert {
            panel,
            input,
            record,
            kind,
            replace,
        } => {
            if !world.entities().contains(panel) {
                return;
            }
            let result = match result {
                Ok(Response::Stored { asset }) => {
                    let current = world.get::<crate::record_binding::TextBinding>(input);
                    if current.is_none_or(|binding| {
                        binding.record.uid != record
                            || binding.record.source != pending.source
                            || binding.property != "body"
                    }) {
                        Err("The destination description changed. Insert again.".into())
                    } else {
                        let reference = nucleus::description_asset::reference(&record, asset);
                        let result = if let Some(previous) = replace {
                            replace_image(world, input, &previous, &reference)
                        } else {
                            append(
                                world,
                                input,
                                &format!(
                                    "![{}]({reference})",
                                    if kind == Kind::Drawing {
                                        "Native drawing"
                                    } else {
                                        "Drawing image"
                                    }
                                ),
                            )
                        };
                        if result.is_ok() {
                            crate::drawing::saved_reference(world, panel, reference);
                        }
                        result
                    }
                }
                Ok(_) => Err("Unexpected asset response.".into()),
                Err(error) => Err(error.into()),
            };
            crate::drawing::inserted(world, panel, result);
        }
        Target::View(entity) => {
            if world
                .get::<AssetView>(entity)
                .is_none_or(|view| view.request.as_deref() != Some(id))
            {
                return;
            }
            let result: Result<(), String> = (|| {
                let Response::Data { kind, data_base64 } = result.map_err(str::to_string)? else {
                    return Err("Unexpected asset response.".into());
                };
                if data_base64.len() > nucleus::description_asset::MAX_BYTES.div_ceil(3) * 4 {
                    return Err("Asset exceeds its size limit.".into());
                }
                let bytes = STANDARD
                    .decode(data_base64)
                    .map_err(|error| error.to_string())?;
                if *kind == Kind::Drawing {
                    let drawing: nucleus::drawing::Drawing =
                        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                    drawing.validate()?;
                    crate::drawing::embedded(world, entity, drawing);
                    let view = world.get::<AssetView>(entity).unwrap();
                    let (record, source, reference) = (
                        view.record.clone(),
                        view.source.clone(),
                        nucleus::description_asset::reference(&view.record, &view.asset),
                    );
                    if let Some(input) = editable_input(world, entity, &record, &source) {
                        let embedded = world.get::<Children>(entity).unwrap()[0];
                        super::button(
                            world,
                            embedded,
                            embedded,
                            "Edit drawing…",
                            crate::drawing::EditDescriptionDrawing { input, reference },
                        );
                    }
                } else {
                    let (width, height, rgba) = super::pictures::decode(bytes)?;
                    let handle = crate::media_sand::image(
                        world,
                        crate::media_sand::Pixels {
                            width,
                            height,
                            rgba,
                        },
                    );
                    world.spawn((
                        ImageNode::new(handle),
                        Node {
                            width: px(width as f32),
                            max_width: percent(100),
                            ..default()
                        },
                        ChildOf(entity),
                    ));
                }
                Ok(())
            })();
            if let Err(error) = result {
                crate::edit_mode::label(
                    world,
                    entity,
                    &format!("Asset unavailable: {error}"),
                    14.0,
                );
            }
        }
    }
}

fn editable_input(
    world: &World,
    mut entity: Entity,
    record: &str,
    source: &Source,
) -> Option<Entity> {
    loop {
        if super::transclusion::is_view(world, entity) {
            return None;
        }
        if let Some(editor) = world.get::<Editor>(entity)
            && editor.editable
        {
            let binding = world.get::<crate::record_binding::TextBinding>(editor.input)?;
            return (binding.record.uid == record
                && &binding.record.source == source
                && crate::record_binding::can_edit(world, editor.input))
            .then_some(editor.input);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

fn replaced(source: &str, previous: &str, next: &str) -> Result<String, String> {
    let mut ranges = Vec::new();
    for (event, range) in pulldown_cmark::Parser::new(source).into_offset_iter() {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event
            && dest_url.as_ref() == previous
            && let Some(offset) = source[range.clone()].find(previous)
        {
            ranges.push(range.start + offset..range.start + offset + previous.len());
        }
    }
    if ranges.is_empty() {
        return Err(
            "The drawing reference changed or was removed. Insert it as a new drawing.".into(),
        );
    }
    let mut source = source.to_string();
    for range in ranges.into_iter().rev() {
        source.replace_range(range, next);
    }
    Ok(source)
}

fn replace_image(
    world: &mut World,
    input: Entity,
    previous: &str,
    next: &str,
) -> Result<(), String> {
    if !crate::record_binding::can_edit(world, input) {
        return Err("The description is read only or disconnected.".into());
    }
    let mut text = world
        .get_mut::<EditableText>(input)
        .ok_or("Description was closed.")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish typing or pasting before replacing.".into());
    }
    let source = replaced(&text.value().to_string(), previous, next)?;
    text.editor.set_text(&source);
    Ok(())
}

pub(super) fn expire(world: &mut World) {
    let Some(mut requests) = world.get_resource_mut::<Requests>() else {
        return;
    };
    let expired: Vec<_> = requests
        .0
        .iter()
        .filter(|(_, pending)| pending.started.elapsed().as_secs() > 30)
        .map(|(id, _)| id.clone())
        .collect();
    let pending: Vec<_> = expired
        .into_iter()
        .filter_map(|id| requests.0.remove(&id))
        .collect();
    drop(requests);
    for pending in pending {
        match pending.target {
            Target::Insert { panel, .. } => crate::drawing::inserted(
                world,
                panel,
                Err("Asset request timed out. Try again.".into()),
            ),
            Target::View(entity) => {
                if world.entities().contains(entity) {
                    crate::edit_mode::label(world, entity, "Asset request timed out.", 14.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_an_image_reference_preserves_other_text_and_literal_code() {
        let source = "Local draft\n\n![Drawing](asset:old/hash)\n\n`![Drawing](asset:old/hash)`\n\n![Again](asset:old/hash)";
        let next = replaced(source, "asset:old/hash", "asset:new/hash").unwrap();
        assert_eq!(
            next,
            "Local draft\n\n![Drawing](asset:new/hash)\n\n`![Drawing](asset:old/hash)`\n\n![Again](asset:new/hash)"
        );
        assert!(replaced("Removed", "asset:old/hash", "asset:new/hash").is_err());
    }

    #[test]
    fn revocation_clears_assets_and_ignores_an_older_inflight_response() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<Requests>();
        let record = nucleus::new_uid("r");
        let entity = world
            .spawn(AssetView {
                record,
                asset: "a".repeat(64),
                source: Source::Local,
                row: Some(serde_json::json!({"body":"allowed"})),
                request: Some("old".into()),
            })
            .id();
        let secret = world.spawn((Text::new("old image"), ChildOf(entity))).id();
        world.resource_mut::<Requests>().0.insert(
            "old".into(),
            Pending {
                target: Target::View(entity),
                source: Source::Local,
                started: Instant::now(),
            },
        );
        permission(&mut world, entity, None);
        assert!(!world.entities().contains(secret));
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::DescriptionAsset {
                id: "old".into(),
                response: Response::Data {
                    kind: Kind::Drawing,
                    data_base64: STANDARD
                        .encode(serde_json::to_vec(&nucleus::drawing::Drawing::default()).unwrap()),
                },
            },
        );
        assert_eq!(
            world
                .query::<&crate::drawing::NativeDrawing>()
                .iter(&world)
                .count(),
            0
        );
    }
}
