use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::karma::search::{
        presentation_html_karma_get_condition, presentation_html_karma_get_consequence,
    },
};
use asynk_strim::{Yielder, stream_fn};
use axum::{
    extract::{Json, Query, State},
    response::{IntoResponse, Sse, sse::Event},
};
use datastar::prelude::PatchElements;
use serde::Deserialize;
use serde_json::from_str;
use std::convert::Infallible;

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
        Err(_e) => serde_json::Value::Null,
    };

    let search = signals_value
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(
                    presentation_html_karma_get_condition(services, search).await,
                )
                .write_as_axum_sse_event()))
                .await;
        },
    ))
}

pub async fn handler_karma_post_condition(
    State(services): State<InjectedServices>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let search = body
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(
                    presentation_html_karma_get_condition(services, search).await,
                )
                .write_as_axum_sse_event()))
                .await;
        },
    ))
}

pub async fn handler_karma_get_consequence(
    State(services): State<InjectedServices>,
    Query(wrapper): Query<Wrapper>,
) -> impl IntoResponse {
    let signals_value: serde_json::Value = match from_str(&wrapper.datastar) {
        Ok(v) => v,
        Err(_e) => serde_json::Value::Null,
    };

    let search = signals_value
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(
                    presentation_html_karma_get_consequence(services, search).await,
                )
                .write_as_axum_sse_event()))
                .await;
        },
    ))
}

pub async fn handler_karma_post_consequence(
    State(services): State<InjectedServices>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let search = body
        .get("search")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    Sse::new(stream_fn(
        move |mut yielder: Yielder<Result<Event, Infallible>>| async move {
            yielder
                .yield_item(Ok(PatchElements::new(
                    presentation_html_karma_get_consequence(services, search).await,
                )
                .write_as_axum_sse_event()))
                .await;
        },
    ))
}
