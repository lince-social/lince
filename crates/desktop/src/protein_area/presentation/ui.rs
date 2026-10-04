use super::*;
use bevy::text::EditableText;

#[derive(Component)]
struct Fill {
    root: Entity,
    field: String,
}

#[derive(Component)]
struct PreviewValue {
    root: Entity,
    uid: String,
    field: String,
}

#[derive(Clone)]
enum Command {
    Target(Presentation),
    Missing,
    Extra,
    Cancel,
    Apply,
}

fn collect_fills(world: &mut World, root: Entity) -> bool {
    let values: Vec<_> = world
        .query::<(&Fill, &EditableText)>()
        .iter(world)
        .filter(|(fill, _)| fill.root == root)
        .map(|(fill, text)| (fill.field.clone(), text.value().to_string()))
        .collect();
    let mut changed = false;
    if let Some(mut session) = world.get_mut::<Session>(root) {
        for (field, value) in values {
            if session
                .target
                .fills
                .get(&field)
                .map(String::as_str)
                .unwrap_or("")
                == value
            {
                continue;
            }
            changed = true;
            if value.is_empty() {
                session.target.fills.remove(&field);
            } else {
                session.target.fills.insert(field, value);
            }
        }
    }
    changed
}

pub(super) fn inputs(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Session>>()
        .iter(world)
        .collect();
    for root in roots {
        if !collect_fills(world, root) {
            continue;
        }
        let Some(session) = world.get::<Session>(root).cloned() else {
            continue;
        };
        let view = result(&session);
        let labels: Vec<_> = world
            .query::<(Entity, &PreviewValue)>()
            .iter(world)
            .filter(|(_, value)| value.root == root)
            .map(|(entity, value)| (entity, value.uid.clone(), value.field.clone()))
            .collect();
        for (entity, uid, field) in labels {
            let data = session
                .observed
                .rows
                .iter()
                .find(|(record, _)| *record == uid)
                .map(|(_, data)| data)
                .unwrap_or(&Value::Null);
            let value = preview(&session, &view, &uid, &field, data);
            if let Some(mut text) = world.get_mut::<Text>(entity) {
                text.0 = value;
            }
        }
    }
}

fn preview(session: &Session, view: &Presentation, uid: &str, field: &str, data: &Value) -> String {
    let draft = session
        .observed
        .drafts
        .iter()
        .find(|draft| draft.uid == uid && draft.field == field && draft.dirty);
    let value = draft.map(|draft| draft.text.clone()).unwrap_or_else(|| {
        if data[field].is_null() {
            view.fills
                .get(field)
                .cloned()
                .unwrap_or_else(|| "No value in this Protein".into())
        } else {
            super::super::rows::display(&data[field])
        }
    });
    format!("{field}: {}", value.chars().take(256).collect::<String>())
}

impl Action for Command {
    fn apply(&self, world: &mut World, root: Entity) {
        collect_fills(world, root);
        let Some(mut session) = world.get::<Session>(root).cloned() else {
            return;
        };
        match self {
            Self::Target(view) => session.target = view.clone(),
            Self::Missing => session.include_missing = !session.include_missing,
            Self::Extra => session.keep_extra = !session.keep_extra,
            Self::Cancel => {
                world.entity_mut(root).remove::<Session>();
                crate::edit_mode::render_panel(world, root);
                return;
            }
            Self::Apply => {
                let Some(now) = probe(world, session.owner) else {
                    world.entity_mut(root).remove::<Session>();
                    crate::edit_mode::render_panel(world, root);
                    return;
                };
                if now != session.observed {
                    session.observed = now;
                    session.message = "Fields or local edits changed. Review this updated preview and apply again.".into();
                } else if session.observed.drafts.iter().any(|draft| draft.busy) {
                    session.message = "Finish text composition or the pending paste before changing presentation.".into();
                } else {
                    let view = result(&session);
                    if !view.valid() {
                        session.message = "Choose at least one field. Local values may contain at most 4096 characters.".into();
                    } else if set_view(world, session.owner, view) {
                        world.entity_mut(root).remove::<Session>();
                        crate::edit_mode::render_panel(world, root);
                        return;
                    } else {
                        session.message = "Could not apply this presentation. The source may be closed or the workspace may have reached its Area limit.".into();
                    }
                }
            }
        }
        if !matches!(self, Self::Apply) {
            if let Some(now) = probe(world, session.owner) {
                session.observed = now;
            }
            session.message.clear();
        }
        world.entity_mut(root).insert(session);
        crate::edit_mode::render_panel(world, root);
    }
}

fn choices(world: &mut World, owner: Entity) -> Vec<Presentation> {
    let mut choices = vec![
        template("Title Sand", Layout::Sand, &["head"]),
        template("Text Sand", Layout::Sand, &["body"]),
        template("Description Castle", Layout::Castle, &["head", "body"]),
        template(
            "Task Castle",
            Layout::Castle,
            &[
                "head",
                "body",
                "quantity",
                "assignees",
                "start_date",
                "due_date",
                "estimate_min",
            ],
        ),
        template(
            "Record Castle",
            Layout::Castle,
            &Config::records()
                .bindings
                .iter()
                .map(|binding| binding.property.clone())
                .collect::<Vec<_>>(),
        ),
    ];
    if let Some(root) = root(world, owner) {
        let area = world
            .get::<RecordBinding>(owner)
            .map_or(owner, |binding| binding.area);
        let active = world
            .get::<crate::workspace::Workspaces>(root)
            .map(|spaces| spaces.active);
        let mut templates: Vec<_> = world
            .query::<(Entity, &InfluenceArea, &crate::workspace::WorkspaceMember)>()
            .iter(world)
            .filter(|(entity, _, member)| {
                *entity != area
                    && Some(member.0) == active
                    && super::root(world, *entity) == Some(root)
            })
            .filter_map(|(_, area, _)| {
                let config = area.protein.as_ref()?;
                (!config.fiote && config.command.is_none() && !config.relations).then(|| {
                    config.presentation.clone().unwrap_or_else(|| {
                        template(
                            &area.name,
                            if config.record_cards {
                                Layout::Castle
                            } else {
                                Layout::Sand
                            },
                            &config
                                .bindings
                                .iter()
                                .map(|binding| binding.property.clone())
                                .collect::<Vec<_>>(),
                        )
                    })
                })
            })
            .filter(Presentation::valid)
            .collect();
        templates.sort_by(|a, b| a.name.cmp(&b.name));
        choices.extend(templates);
    }
    choices
}

fn caption(fields: &[String]) -> String {
    if fields.is_empty() {
        "None".into()
    } else {
        fields.join(", ")
    }
}

pub(super) fn panel(world: &mut World, root: Entity, panel: Entity) -> bool {
    let Some(session) = world.get::<Session>(root).cloned() else {
        if let Some(crate::customization::Scope::Sand(selected)) =
            world.get::<crate::customization::Scope>(root).copied()
        {
            let mut cursor = Some(selected);
            while let Some(entity) = cursor {
                if probe(world, entity).is_some() {
                    crate::sand_panel::button(
                        world,
                        panel,
                        root,
                        "Change presentation…",
                        Open(entity),
                    );
                    if world
                        .get::<History>(entity)
                        .is_some_and(|history| !history.0.is_empty())
                    {
                        crate::sand_panel::button(
                            world,
                            panel,
                            root,
                            "Undo presentation",
                            Undo(entity),
                        );
                    }
                    break;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
        }
        return false;
    };
    crate::edit_mode::label(world, panel, "Change presentation", 20.0);
    let choices = choices(world, session.owner)
        .into_iter()
        .map(|view| (view.name.clone(), crate::actions![Command::Target(view)]))
        .collect();
    crate::dropdown::spawn(
        world,
        panel,
        root,
        "Present as",
        &session.target.name,
        choices,
    );
    let comparison = compare(&session.observed.fields, &session.target.fields);
    crate::edit_mode::label(
        world,
        panel,
        &format!("Matching: {}", caption(&comparison.matching)),
        14.0,
    );
    crate::edit_mode::label(
        world,
        panel,
        &format!(
            "Missing from this presentation: {}",
            caption(&comparison.missing)
        ),
        14.0,
    );
    crate::edit_mode::label(
        world,
        panel,
        &format!("Extra fields: {}", caption(&comparison.extra)),
        14.0,
    );
    if !comparison.missing.is_empty() {
        crate::sand_panel::button(
            world,
            panel,
            root,
            if session.include_missing {
                "Missing fields: show and fill locally"
            } else {
                "Missing fields: show matching fields only"
            },
            Command::Missing,
        );
        if session.include_missing {
            for field in &comparison.missing {
                let entity = crate::sand_panel::field(
                    world,
                    panel,
                    &format!("{field} · local presentation value"),
                    session
                        .target
                        .fills
                        .get(field)
                        .map(String::as_str)
                        .unwrap_or(""),
                );
                world
                    .get_mut::<EditableText>(entity)
                    .unwrap()
                    .max_characters = Some(4096);
                world.entity_mut(entity).insert(Fill {
                    root,
                    field: field.clone(),
                });
            }
            crate::edit_mode::label(
                world,
                panel,
                "Local values fill fields absent from this Protein. They do not edit the Record.",
                12.0,
            );
        }
    }
    if !comparison.extra.is_empty() {
        crate::sand_panel::button(
            world,
            panel,
            root,
            if session.keep_extra {
                "Extra fields: add to the result"
            } else {
                "Extra fields: hide from the result"
            },
            Command::Extra,
        );
    }
    let result = result(&session);
    let dirty: Vec<_> = session
        .observed
        .drafts
        .iter()
        .filter(|draft| draft.dirty)
        .collect();
    crate::edit_mode::label(
        world,
        panel,
        &format!(
            "{} local edits. Field editors and their history stay available. Record saving continues normally.",
            dirty.len()
        ),
        13.0,
    );
    for draft in dirty.iter().take(32) {
        let effect = if result.fields.contains(&draft.field) {
            "shown"
        } else {
            "hidden; draft retained"
        };
        crate::edit_mode::label(
            world,
            panel,
            &format!(
                "{} · {} · {effect}: {}",
                draft.uid,
                draft.field,
                draft.text.chars().take(160).collect::<String>()
            ),
            12.0,
        );
    }
    crate::edit_mode::label(world, panel, "Preview", 18.0);
    if session.observed.rows.is_empty() {
        crate::edit_mode::label(
            world,
            panel,
            &format!("{} · fields: {}", result.name, caption(&result.fields)),
            14.0,
        );
    }
    for (uid, data) in session.observed.rows.iter().take(3) {
        let card = crate::sand_panel::column(world, panel);
        world.entity_mut(card).insert((
            crate::sand::Square,
            crate::token_style::background(crate::tokens::Token::Surface),
        ));
        world.get_mut::<Node>(card).unwrap().padding = UiRect::all(px(8));
        for field in &result.fields {
            let text = crate::edit_mode::label(
                world,
                card,
                &preview(&session, &result, uid, field, data),
                if field == "head" { 20.0 } else { 14.0 },
            );
            world.entity_mut(text).insert(PreviewValue {
                root,
                uid: uid.clone(),
                field: field.clone(),
            });
        }
    }
    crate::edit_mode::label(
        world,
        panel,
        "Hiding fields changes their presentation. Their Record data and local edits are kept. Undo restores the previous presentation.",
        12.0,
    );
    if !session.message.is_empty() {
        crate::edit_mode::label(world, panel, &session.message, 14.0);
    }
    let buttons = crate::sand_panel::row(world, panel);
    crate::sand_panel::button(world, buttons, root, "Cancel", Command::Cancel);
    crate::sand_panel::button(world, buttons, root, "Apply presentation", Command::Apply);
    true
}

#[cfg(test)]
pub(super) fn apply(world: &mut World, root: Entity) {
    Command::Apply.apply(world, root);
}

#[cfg(test)]
pub(super) fn cancel(world: &mut World, root: Entity) {
    Command::Cancel.apply(world, root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_a_local_fill_updates_the_preview_without_writing_record_data() {
        let (mut app, root, owner, rows) = super::super::tests::fixture();
        let before = app.world().resource::<Runtime>().areas[&owner].data.clone();
        Open(rows[0]).apply(app.world_mut(), root);
        {
            let mut session = app.world_mut().get_mut::<Session>(root).unwrap();
            session.target = template("Task Castle", Layout::Castle, &["head", "due_date"]);
            session.include_missing = true;
        }
        crate::edit_mode::render_panel(app.world_mut(), root);
        let input = app
            .world_mut()
            .query::<(Entity, &Fill)>()
            .iter(app.world())
            .find(|(_, fill)| fill.root == root && fill.field == "due_date")
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("Next week");
        inputs(app.world_mut());
        assert!(
            app.world_mut()
                .query::<(&PreviewValue, &Text)>()
                .iter(app.world())
                .any(|(value, text)| value.field == "due_date" && text.0 == "due_date: Next week")
        );
        assert_eq!(app.world().resource::<Runtime>().areas[&owner].data, before);
        assert!(current(app.world(), rows[0]).is_none());
        cancel(app.world_mut(), root);
        assert!(app.world().get::<Session>(root).is_none());
    }
}
