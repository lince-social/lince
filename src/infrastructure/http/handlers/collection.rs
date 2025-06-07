use crate::{
    application::use_cases::collection::set_active::use_case_collection_set_active,
    infrastructure::cross_cutting::InjectedServices,
};
use axum::{
    extract::{Path, State},
    response::Html,
};

pub async fn handler_collection_set_active(
    State(services): State<InjectedServices>,
    Path(id): Path<String>,
) -> Html<String> {
    Html(
        use_case_collection_set_active(services, id)
            .await
            .to_string(),
    )
}
