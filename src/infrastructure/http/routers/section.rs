use axum::{Router, routing::get};

use crate::{
    infrastructure::{
        cross_cutting::InjectedServices,
        http::handlers::section::{handler_section_get_body, main_handler},
    },
    presentation::web::section::{
        header::header, nav::presentation_web_section_nav, page::presentation_web_section_page,
    },
};

pub fn section_router(services: InjectedServices) -> Router {
    Router::new()
        .route("/", get(presentation_web_section_page))
        .route("/body", get(handler_section_get_body))
        .route("/header", get(header))
        .route("/nav", get(presentation_web_section_nav))
        .route("/main", get(main_handler))
        .with_state(services)
}
