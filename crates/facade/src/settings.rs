use axum::http::StatusCode;
use engine::actions::Action;
use serde_json::{Value, json};

use crate::{
    State,
    auth::{Failure, internal},
    records,
};

pub(crate) const SLUG: &str = "lince-facade";
pub(crate) const COLUMNS: &str = "lince.kanban.columns";
pub(crate) const CUSTOM: &str = "lince.facade";

pub(crate) fn general_defaults() -> Value {
    json!({"title":"Facade","language":"pt-BR"})
}

pub(crate) fn administrator(user: Option<&store::auth::AuthUser>) -> bool {
    user.is_some_and(|user| user.role == "admin")
}

pub(crate) async fn general(state: &State) -> Result<Value, Failure> {
    let mut value = general_defaults();
    if let Some(record) = store::records::resolve(&state.cell.store.pool, SLUG)
        .await
        .map_err(internal)?
    {
        if let Some(custom) =
            store::records::get_extension(&state.cell.store.pool, &record.uid, CUSTOM)
                .await
                .map_err(internal)?
        {
            if let Some(fields) = custom.as_object() {
                let mut configured = general_defaults();
                configured.as_object_mut().unwrap().extend(fields.clone());
                if validate(CUSTOM, &configured).is_ok() {
                    value = configured;
                }
            }
        }
    }
    Ok(value)
}

pub(crate) fn defaults() -> Value {
    json!({"name":"Padrão", "columns":[{"title":"Pendências","quantity":0},{"title":"Próximos","quantity":-1},{"title":"Em andamento","quantity":-2},{"title":"Revisão","quantity":-3},{"title":"Concluídos","quantity":1}]})
}

pub(crate) async fn ensure(state: &State) -> Result<(), Failure> {
    if store::records::resolve(&state.cell.store.pool, SLUG)
        .await
        .map_err(internal)?
        .is_none()
    {
        state
            .cell
            .engine
            .act(
                Action::CreateRecord {
                    slug: Some(SLUG.into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Facade".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .map_err(internal)?;
    }
    if let Some(record) = store::records::resolve(&state.cell.store.pool, SLUG)
        .await
        .map_err(internal)?
    {
        for (namespace, fds) in [(COLUMNS, defaults()), (CUSTOM, general_defaults())] {
            if store::records::get_extension(&state.cell.store.pool, &record.uid, namespace)
                .await
                .map_err(internal)?
                .is_none()
            {
                state
                    .cell
                    .engine
                    .act(
                        Action::SetExtension {
                            target: record.uid.clone(),
                            namespace: namespace.into(),
                            fds,
                        },
                        None,
                    )
                    .await
                    .map_err(internal)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn validate(namespace: &str, value: &Value) -> Result<(), Failure> {
    let valid = if namespace == COLUMNS {
        value["name"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 200)
            && value["columns"].as_array().is_some_and(|columns| {
                !columns.is_empty()
                    && columns.len() <= 40
                    && columns.iter().enumerate().all(|(index, column)| {
                        column["title"]
                            .as_str()
                            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 100)
                            && column["quantity"].as_f64().is_some_and(|n| n.is_finite())
                            && !columns[..index].iter().any(|other| {
                                other["quantity"].as_f64() == column["quantity"].as_f64()
                            })
                    })
            })
    } else if namespace == CUSTOM {
        value.as_object().is_some_and(|fields| {
            fields
                .keys()
                .all(|key| matches!(key.as_str(), "title" | "language"))
        }) && value["title"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 200)
            && matches!(value["language"].as_str(), Some("pt-BR" | "en"))
    } else {
        false
    };
    if valid {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            if namespace == CUSTOM {
                "Choose an app title and Portuguese or English."
            } else {
                "Choose a name and 1–40 columns with unique numeric values."
            }
            .into(),
        ))
    }
}

pub(crate) async fn snapshot(
    state: &State,
    user: Option<&store::auth::AuthUser>,
) -> Result<Value, Failure> {
    let subject = user.map(|u| u.uid.as_str());
    let facade = store::records::resolve(&state.cell.store.pool, SLUG)
        .await
        .map_err(internal)?;
    let mut configuration = defaults();
    let general = general(state).await?;
    let mut uid = String::new();
    let mut templates = Vec::new();
    if let Some(facade) = facade {
        if let Some(columns) =
            store::records::get_extension(&state.cell.store.pool, &facade.uid, COLUMNS)
                .await
                .map_err(internal)?
        {
            if validate(COLUMNS, &columns).is_ok() {
                configuration = columns;
            }
        }
        if administrator(user) || records::visible(state, subject, &facade.uid).await? {
            uid = facade.uid;
        }
    }
    for (parent, extension) in store::records::all_extensions(&state.cell.store.pool, COLUMNS)
        .await
        .map_err(internal)?
    {
        if validate(COLUMNS, &extension).is_ok()
            && records::visible(state, subject, &parent).await?
        {
            templates.push(
                json!({"uid":parent,"name":extension["name"],"columns":extension["columns"]}),
            );
        }
    }
    templates.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(
        json!({"facadeuid":uid,"customtitle":general["title"],"language":general["language"],"columns":configuration["columns"],"templates":templates,
        "canadmin":!uid.is_empty() && administrator(user),
        "canconfigure":!uid.is_empty() && records::permits(user,"configuration:update") && records::permits(user,"record:update")}),
    )
}
