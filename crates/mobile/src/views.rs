use crate::app::{Intent, Mobile, button, input, label};
use bevy::prelude::*;
use lince_interface::queries::ProteinDraft;

pub const FIELDS: &[(&str, &str)] = &[
    ("quantity", "Quantity"),
    ("due_date", "Due date"),
    ("start_date", "Start date"),
    ("created_at", "Created"),
    ("kind", "Kind"),
    ("slug", "Slug"),
];
pub const PAGE_SIZE: usize = 50;

pub fn query(state: &Mobile, kanban: bool) -> Result<protein::Protein, String> {
    let mut query = match &state.view {
        Some(draft) => draft.compile()?,
        None => {
            let mut query = crate::record::query(None, PAGE_SIZE + 1, "");
            query.order = vec![protein::Order::Asc(
                crate::record::SORTS[state.sort].1.into(),
            )];
            query
        }
    };
    if query.source != protein::Source::Record || query.aggregate.is_some() {
        return Err("Choose a Protein that lists Records without aggregation".into());
    }
    if !state.search.trim().is_empty() {
        query
            .filter
            .push(protein::Predicate::TextContains(state.search.trim().into()));
    }
    if state.negative_only {
        query.filter.push(protein::Predicate::QuantityLt(
            nucleus::DecimalValue::parse_inferred("0").expect("zero"),
        ));
    }
    if kanban {
        query
            .filter
            .push(protein::Predicate::KindEq("plain".into()));
        query
            .filter
            .push(protein::Predicate::ConceptIn("task".into()));
        let columns = &lince_interface::records::KANBAN_COLUMNS;
        let predicate = |quantity: i32| {
            protein::Predicate::QuantityEq(
                nucleus::DecimalValue::parse_inferred(&quantity.to_string())
                    .expect("column quantity"),
            )
        };
        query.filter.push(match columns.get(state.kanban_column) {
            Some((_, _, quantity)) => predicate(*quantity),
            None => protein::Predicate::Not(Box::new(protein::Predicate::Any(
                columns
                    .iter()
                    .map(|(_, _, quantity)| predicate(*quantity))
                    .collect(),
            ))),
        });
    }
    if let Some(fields) = &mut query.fields {
        for field in ["uid", "head", "quantity"] {
            if !fields.iter().any(|key| key == field) {
                fields.push(field.into());
            }
        }
    }
    query.limit = Some(PAGE_SIZE + 1);
    query.include.record_after = state.record_pages.last().cloned();
    protein::validate(&query).map_err(|error| error.to_string())?;
    Ok(query)
}

pub fn draft(state: &Mobile) -> ProteinDraft {
    state.view.clone().unwrap_or_else(|| {
        ProteinDraft::from_protein(
            "My Records".into(),
            String::new(),
            crate::record::query(None, PAGE_SIZE, ""),
        )
    })
}

pub fn apply(world: &mut World, intent: Intent) -> Result<(), String> {
    use crate::app::{act, row, subscriptions};
    use serde_json::{Value, json};
    match intent {
        Intent::ViewSettings => {
            let mut state = world.resource_mut::<Mobile>();
            state.view_settings = !state.view_settings;
        }
        Intent::ToggleViewField(field) => {
            if !FIELDS.iter().any(|(key, _)| *key == field) {
                return Err("Unknown card field".into());
            }
            let mut draft = draft(world.resource::<Mobile>());
            let mut fields: Vec<String> = draft.query["fields"]
                .as_array()
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| FIELDS.iter().map(|(key, _)| (*key).into()).collect());
            if fields.contains(&field) {
                fields.retain(|key| key != &field);
            } else {
                fields.push(field);
            }
            for key in ["uid", "head"] {
                if !fields.iter().any(|field| field == key) {
                    fields.push(key.into());
                }
            }
            draft.query["fields"] = json!(fields);
            draft.compile()?;
            world.resource_mut::<Mobile>().view = Some(draft);
            subscriptions(world)?;
        }
        Intent::LoadView(uid) => {
            let draft = match uid {
                Some(uid) => {
                    let saved =
                        row(world, "saved_views", &uid).ok_or("Saved view is unavailable")?;
                    let query = serde_json::from_value(saved["extension"].clone())
                        .map_err(|error| format!("Saved view: {error}"))?;
                    let draft = ProteinDraft::from_protein(
                        saved["head"].as_str().unwrap_or("View").into(),
                        saved["slug"].as_str().unwrap_or_default().into(),
                        query,
                    );
                    let query = draft.compile()?;
                    if query.source != protein::Source::Record || query.aggregate.is_some() {
                        return Err("Choose a saved view that lists Records".into());
                    }
                    Some(draft)
                }
                None => None,
            };
            let mut state = world.resource_mut::<Mobile>();
            state.view = draft;
            state.search.clear();
            state.negative_only = false;
            state
                .drafts
                .retain(|key, _| !key.starts_with("view/") && key != "list/search");
            state.record_pages.clear();
            subscriptions(world)?;
        }
        Intent::SaveView => {
            let state = world.resource::<Mobile>();
            let draft = draft(state);
            let name = state.draft("view", "name", &draft.name);
            let slug = state.draft("view", "slug", &draft.slug);
            if name.trim().is_empty() || slug.trim().is_empty() {
                return Err("Give the view a name and an identifier".into());
            }
            let mut query = query(state, false)?;
            if state.navigation.current == crate::navigation::Page::Kanban {
                query
                    .filter
                    .push(protein::Predicate::KindEq("plain".into()));
                query
                    .filter
                    .push(protein::Predicate::ConceptIn("task".into()));
            }
            query.limit = None;
            query.include.record_after = None;
            let draft = ProteinDraft::from_protein(name.trim().into(), slug.trim().into(), query);
            let query = draft.compile()?;
            act(
                world,
                engine::actions::Action::SaveProtein {
                    head: draft.name,
                    slug: draft.slug,
                    ast: serde_json::to_value(query).map_err(|error| error.to_string())?,
                },
                None,
            )?;
        }
        _ => return Err("Unknown view control".into()),
    }
    Ok(())
}

pub fn render(world: &mut World, parent: Entity) {
    button(
        world,
        parent,
        "Saved views and fields",
        Intent::ViewSettings,
    );
    if !world.resource::<Mobile>().view_settings {
        return;
    }
    let draft = draft(world.resource::<Mobile>());
    input(
        world,
        parent,
        "view",
        "name",
        "View name",
        &draft.name,
        false,
    );
    input(
        world,
        parent,
        "view",
        "slug",
        "View identifier",
        &draft.slug,
        false,
    );
    label(
        world,
        parent,
        "Choose card details. Title is always shown. Search, quantity filter and sort are saved with the view.",
        14.0,
    );
    for (key, title) in FIELDS {
        let enabled = draft.query["fields"]
            .as_array()
            .is_none_or(|fields| fields.iter().any(|value| value == key));
        button(
            world,
            parent,
            &format!("{} {title}", if enabled { "✓" } else { "+" }),
            Intent::ToggleViewField((*key).into()),
        );
    }
    button(world, parent, "Save view", Intent::SaveView);
    button(world, parent, "Use default view", Intent::LoadView(None));
    let saved = world
        .resource::<Mobile>()
        .rows
        .get("saved_views")
        .cloned()
        .unwrap_or_default();
    for row in saved {
        if let Some(uid) = row["uid"].as_str() {
            button(
                world,
                parent,
                row["head"].as_str().unwrap_or("Saved view"),
                Intent::LoadView(Some(uid.into())),
            );
            button(
                world,
                parent,
                "Delete saved view",
                Intent::Ask(engine::actions::Action::DeleteRecord { target: uid.into() }),
            );
        }
    }
}
