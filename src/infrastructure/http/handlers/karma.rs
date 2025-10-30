use std::convert::Infallible;

use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::karma::search::presentation_html_karma_get_condition,
};
use asynk_strim::{Yielder, stream_fn};
use axum::{
    extract::{Json, Query, State},
    response::{IntoResponse, Sse, sse::Event},
};
use datastar::prelude::PatchElements;
use serde::Deserialize;
use serde_json::from_str;

#[derive(Deserialize)]
pub struct Wrapper {
    datastar: String,
}

#[derive(Deserialize, Debug)]
pub struct LinceSignals {
    pub configuration_open: bool,
    pub search: Option<String>,
}

pub async fn handler_karma_get_condition(
    State(services): State<InjectedServices>,
    Query(wrapper): Query<Wrapper>,
) -> impl IntoResponse {
    let signals_value: serde_json::Value = match from_str(&wrapper.datastar) {
        Ok(v) => v,
        Err(e) => serde_json::Value::Null,
    };

    // extract search from several possible key names
    let search = signals_value
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
        // also accept camelCase
        .or_else(|| {
            signals_value
                .get("Search")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            signals_value
                .get("searchText")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        });

    let a = presentation_html_karma_get_condition(services, search).await;
    dbg!(&a);
    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(a).write_as_axum_sse_event()))
                .await;
        },
    ))
}

pub async fn handler_karma_post_condition(
    State(services): State<InjectedServices>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // extract search from the JSON body
    let search = body
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let a = presentation_html_karma_get_condition(services, search).await;
    println!("test: {a}");
    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(a).write_as_axum_sse_event()))
                .await;
        },
    ))
}
