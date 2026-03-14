use axum::{
    Form, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get, patch, post},
};
use domain::dirty::operation::DatabaseTable;
use injection::cross_cutting::InjectedServices;
use serde::Deserialize;
use std::{io::Error, net::SocketAddr, str::FromStr};

pub mod collection;
pub mod colorscheme;
pub mod datastar;
pub mod karma;
pub mod operation;
pub mod pages;
pub mod section;
pub mod table;
pub mod utils;
pub mod view;

#[derive(Deserialize)]
struct OperationForm {
    operation: String,
}

#[derive(Deserialize)]
struct TableValueForm {
    value: String,
}

#[derive(Deserialize)]
struct CreateRowForm {
    #[serde(flatten)]
    values: std::collections::HashMap<String, String>,
}

pub async fn serve(services: InjectedServices) -> Result<(), Error> {
    let app = Router::new()
        .route("/", get(section::page::presentation_html_section_page))
        .route("/body", get(body))
        .route("/header", get(header))
        .route("/main", get(main))
        .route("/table/{table}/{id}/{column}", patch(patch_table_cell))
        .route("/table/{table}/{id}", delete(delete_table_row))
        .route("/collection/active/{id}", patch(set_active_collection))
        .route(
            "/view/toggle/{collection_id}",
            patch(toggle_collection_views),
        )
        .route(
            "/view/toggle/{collection_id}/{view_id}",
            patch(toggle_single_view),
        )
        .route("/operation", post(run_operation))
        .route(
            "/operation/create/{table}",
            get(open_create_row).post(create_row),
        )
        .with_state(services);
    let address = SocketAddr::from(([127, 0, 0, 1], 6174));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(Error::other)?;
    println!(
        "HTML frontend listening at http://{}",
        listener.local_addr().map_err(Error::other)?
    );
    axum::serve(listener, app).await.map_err(Error::other)
}

async fn body(State(services): State<InjectedServices>) -> impl IntoResponse {
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn header(State(services): State<InjectedServices>) -> impl IntoResponse {
    datastar::patch_elements(
        "header",
        "outer",
        section::header::presentation_html_section_header(services)
            .await
            .to_string(),
    )
}

async fn main(State(services): State<InjectedServices>) -> impl IntoResponse {
    datastar::patch_elements(
        "#main",
        "outer",
        section::main::presentation_html_section_main(services).await,
    )
}

async fn patch_table_cell(
    State(services): State<InjectedServices>,
    Path((table_name, id, column)): Path<(String, String, String)>,
    Form(form): Form<TableValueForm>,
) -> impl IntoResponse {
    let _ =
        application::table::table_patch_row(services.clone(), table_name, id, column, form.value)
            .await;

    datastar::patch_elements(
        "#main",
        "outer",
        section::main::presentation_html_section_main(services).await,
    )
}

async fn delete_table_row(
    State(services): State<InjectedServices>,
    Path((table_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let _ = services.repository.table.delete_by_id(table_name, id).await;
    datastar::patch_elements(
        "#main",
        "outer",
        section::main::presentation_html_section_main(services).await,
    )
}

async fn set_active_collection(
    State(services): State<InjectedServices>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _ = services.repository.collection.set_active(&id).await;
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn toggle_collection_views(
    State(services): State<InjectedServices>,
    Path(collection_id): Path<u32>,
) -> impl IntoResponse {
    let _ = services
        .repository
        .collection
        .toggle_by_collection_id(collection_id)
        .await;
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn toggle_single_view(
    State(services): State<InjectedServices>,
    Path((collection_id, view_id)): Path<(u32, u32)>,
) -> impl IntoResponse {
    let _ = services
        .repository
        .collection
        .toggle_by_view_id(collection_id, view_id)
        .await;
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn run_operation(
    State(services): State<InjectedServices>,
    Form(form): Form<OperationForm>,
) -> impl IntoResponse {
    let result = application::operation::operation_execute(services.clone(), form.operation).await;
    if let Ok(results) = result
        && let Some((table, _)) = results.first()
    {
        let table_name = table.as_table_name().to_string();
        if let Ok(columns) = services
            .repository
            .table
            .get_columns(table_name.clone())
            .await
        {
            return datastar::patch_elements(
                "#body",
                "outer",
                section::body::presentation_html_section_body_home_modal(
                    services.clone(),
                    operation::create::presentation_html_create(table_name, columns)
                        .await
                        .into_string(),
                )
                .await,
            );
        }
    }

    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn open_create_row(
    State(services): State<InjectedServices>,
    Path(table_name): Path<String>,
) -> impl IntoResponse {
    let parsed = DatabaseTable::from_str(&table_name).ok();
    let table_name = parsed
        .map(|table| table.as_table_name().to_string())
        .unwrap_or(table_name);
    let columns = services
        .repository
        .table
        .get_columns(table_name.clone())
        .await
        .unwrap_or_default();
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body_home_modal(
            services.clone(),
            operation::create::presentation_html_create(table_name, columns)
                .await
                .into_string(),
        )
        .await,
    )
}

async fn create_row(
    State(services): State<InjectedServices>,
    Path(table_name): Path<String>,
    Form(form): Form<CreateRowForm>,
) -> impl IntoResponse {
    let parsed = DatabaseTable::from_str(&table_name).ok();
    let table_name = parsed
        .map(|table| table.as_table_name().to_string())
        .unwrap_or(table_name);
    let _ = services
        .repository
        .table
        .insert_row(table_name, form.values)
        .await;
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}
