use super::*;

pub(crate) fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    component: &ContentKind,
) -> Result<Entity, String> {
    component.validate(&registry(), false)?;
    let entity = match component {
        ContentKind::Builtin { state } => {
            crate::component_push::spawn(world, root, workspace, position, state)?
        }
        ContentKind::Native {
            kind,
            settings,
            bindings,
        } if kind == "protein" => {
            let query = settings
                .get("query")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| "Invalid Protein query JSON.")?
                .unwrap_or_else(|| crate::protein_castle::ProteinDraft::default().query);
            if record_references(&query)
                .iter()
                .any(|uid| !bindings.contains(uid))
            {
                return Err("List the query's referenced Record UIDs in bindings.".into());
            }
            let draft = crate::protein_castle::ProteinDraft {
                name: settings
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Protein")
                    .into(),
                slug: settings
                    .get("slug")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                query,
            };
            draft.compile()?;
            crate::protein_castle::spawn(world, root, workspace, position, draft)
        }
        ContentKind::Native { kind, settings, .. } if kind == "instinct" => {
            let page = settings
                .get("page")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let instinct = crate::instinct::Instinct {
                page: (!page.is_empty()).then(|| page.into()),
            };
            if !instinct.valid() {
                return Err("Invalid Instinct page.".into());
            }
            crate::instinct::spawn(world, root, workspace, position, instinct)
        }
        ContentKind::Native { kind, settings, .. } => {
            let kind = SandKind::ALL
                .into_iter()
                .find(|candidate| kind_id(*candidate) == *kind)
                .ok_or("This native component cannot be recreated through canvas Actions.")?;
            if settings
                .get("text")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|text| text.chars().count() > 4096)
            {
                return Err("A native text area supports at most 4096 characters.".into());
            }
            let appearance = settings
                .get("appearance")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str::<crate::tokens::TokenOverrides>)
                .transpose()
                .map_err(|_| "Invalid component appearance.")?;
            if appearance
                .as_ref()
                .is_some_and(|appearance| !appearance.validate())
            {
                return Err("Invalid component appearance.".into());
            }
            let mut saved_texts = settings
                .get("texts")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str::<Vec<crate::sand_text::SavedText>>)
                .transpose()
                .map_err(|_| "Invalid native text areas.")?;
            if let Some(texts) = &mut saved_texts {
                if let (Some(text), Some(first)) = (
                    settings.get("text").and_then(serde_json::Value::as_str),
                    texts.first_mut(),
                ) {
                    first.text = text.into();
                }
            }
            if saved_texts.as_ref().is_some_and(|texts| texts.is_empty())
                && settings.get("text").and_then(serde_json::Value::as_str).is_some_and(|text| !text.is_empty())
            {
                saved_texts = None;
            }
            if saved_texts
                .as_ref()
                .is_some_and(|texts| texts.len() > 64 || texts.iter().any(|text| !text.validate()))
            {
                return Err("Invalid native text areas.".into());
            }
            let timer = settings
                .get("timer")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str::<crate::work_timer::LocalTimer>)
                .transpose()
                .map_err(|_| "Invalid timer state.")?;
            let time_castle = settings
                .get("time_castle")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str::<lince_interface::time_castle::Settings>)
                .transpose()
                .map_err(|_| "Invalid Time Castle settings.")?;
            if time_castle.as_ref().is_some_and(|settings| !settings.valid() || kind != SandKind::WorkTimer) {
                return Err("Invalid Time Castle settings.".into());
            }
            if timer.as_ref().is_some_and(|timer| !timer.valid()) {
                return Err("Invalid timer state.".into());
            }
            let todo = settings
                .get("todo")
                .and_then(serde_json::Value::as_str)
                .map(serde_json::from_str::<crate::todo::SavedTodo>)
                .transpose()
                .map_err(|_| "Invalid Todo state.")?;
            if todo.as_ref().is_some_and(|todo| !todo.valid()) {
                return Err("Invalid Todo state.".into());
            }
            let entity = crate::sand_store::spawn_sand(
                world,
                root,
                workspace,
                kind,
                settings
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                position,
            );
            if let Some(saved_texts) = saved_texts {
                let existing: Vec<_> = world
                    .get::<Children>(entity)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|child| world.get::<crate::sand_text::SandText>(*child).is_some())
                    .collect();
                for text in existing {
                    world.despawn(text);
                }
                let mut first = None;
                for text in saved_texts {
                    let text = crate::sand_text::spawn(world, entity, text);
                    first.get_or_insert(text);
                }
                if matches!(
                    kind,
                    SandKind::Text | SandKind::EditableText | SandKind::WorkTimer
                ) {
                    world.get_mut::<StoredSand>(entity).unwrap().content = first;
                }
            }
            if let Some(appearance) = appearance {
                crate::token_style::set_overrides(world, entity, appearance);
            }
            if let Some(timer) = timer {
                world.entity_mut(entity).insert(timer);
            }
            if let Some(settings) = time_castle { world.entity_mut(entity).insert(crate::time_castle::TimeSettings(settings)); }
            if let Some(todo) = todo {
                crate::todo::restore(world, entity, todo);
            }
            let texts: Vec<_> = world
                .get::<Children>(entity)
                .into_iter()
                .flatten()
                .copied()
                .filter(|child| world.get::<crate::sand_text::SandText>(*child).is_some())
                .collect();
            for text in texts {
                for (id, value) in settings.iter().filter(|(id, _)| {
                    !matches!(
                        id.as_str(),
                        "text" | "texts" | "timer" | "time_castle" | "todo" | "appearance"
                    )
                }) {
                    let value =
                        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
                    if !crate::sand_settings::set(world, text, id, Some(value)) {
                        world.despawn(entity);
                        return Err("The native component rejected these settings.".into());
                    }
                }
            }
            entity
        }
        ContentKind::Composition { composition } => crate::canvas_host::composition::spawn(
            world,
            root,
            workspace,
            position,
            composition.clone(),
        )?,
    };
    world.entity_mut(entity).insert((
        Identity(nucleus::new_uid("placement")),
        Content(component.clone()),
    ));
    Ok(entity)
}

pub(crate) fn restore_content(
    world: &mut World,
    entity: Entity,
    component: &ContentKind,
) -> Result<(), String> {
    if let ContentKind::Composition { composition } = component {
        let geometry = world.get::<CanvasItem>(entity).copied();
        composition::populate(world, entity, composition.clone())?;
        if let Some(item) = geometry {
            world.entity_mut(entity).insert(item);
            if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(entity) {
                area.center = item.position.to_array();
                area.size = item.size.as_dvec2().to_array();
            }
        }
    }
    Ok(())
}
