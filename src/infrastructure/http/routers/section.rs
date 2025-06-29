use axum::{Router, routing::get};

use crate::{
    infrastructure::{
        cross_cutting::InjectedServices,
        http::handlers::section::{
            handler_section_body, handler_section_header, handler_section_main,
        },
    },
    presentation::html::section::{
        nav::presentation_html_section_nav, page::presentation_html_section_page,
    },
};

pub fn section_router(services: InjectedServices) -> Router {
    Router::new()
        .route("/", get(presentation_html_section_page))
        .route("/body", get(handler_section_body))
        .route("/header", get(handler_section_header))
        .route("/nav", get(presentation_html_section_nav))
        .route("/main", get(handler_section_main))
        .with_state(services)
}
