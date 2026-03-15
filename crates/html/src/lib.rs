use axum::{
    Form, Router,
    extract::{Path, State},
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, patch, post},
};
use domain::dirty::operation::DatabaseTable;
use futures::stream::{self, Stream};
use injection::cross_cutting::InjectedServices;
use persistence::connection::sqlite_connect_options;
use serde::Deserialize;
use sqlx::{Connection, SqliteConnection};
use std::{
    collections::VecDeque, convert::Infallible, io::Error, net::SocketAddr, str::FromStr,
    sync::Arc, time::Duration,
};
use tokio::sync::broadcast;

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

#[derive(Clone)]
struct HtmlState {
    services: InjectedServices,
    active_context_tx: broadcast::Sender<()>,
}

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
    let (active_context_tx, _active_context_rx) = broadcast::channel(100);
    let state = Arc::new(HtmlState {
        services,
        active_context_tx,
    });
    let app = Router::<Arc<HtmlState>>::new()
        .route("/", get(page))
        .route("/body", get(body))
        .route("/header", get(header))
        .route("/main", get(main))
        .route("/sse/active-context", get(active_context_sse))
        .route(
            "/table/{table}/{id}/{column}",
            get(open_table_cell).patch(patch_table_cell),
        )
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
        .with_state(state);
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

async fn body(State(state): State<Arc<HtmlState>>) -> impl IntoResponse {
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(state.services.clone()).await,
    )
}

async fn page(State(state): State<Arc<HtmlState>>) -> impl IntoResponse {
    section::page::presentation_html_page(state.services.clone()).await
}

async fn header(State(state): State<Arc<HtmlState>>) -> impl IntoResponse {
    datastar::patch_elements(
        "header",
        "outer",
        section::header::presentation_html_section_header(state.services.clone())
            .await
            .to_string(),
    )
}

async fn main(State(state): State<Arc<HtmlState>>) -> impl IntoResponse {
    datastar::patch_elements(
        "#main",
        "outer",
        section::main::presentation_html_section_main(state.services.clone()).await,
    )
}

async fn active_context_sse(
    State(state): State<Arc<HtmlState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    interval.tick().await;
    let mut monitor_connection = open_sqlite_monitor_connection().await;
    let last_data_version = match monitor_connection.as_mut() {
        Some(connection) => sqlite_data_version(connection).await.ok(),
        None => None,
    };
    let services = state.services.clone();
    let stream = stream::unfold(
        ActiveContextStreamState {
            state,
            rx: None,
            pending_events: VecDeque::from(render_active_context_events(services).await),
            monitor_connection,
            last_data_version,
            interval,
        },
        next_active_context_event,
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn patch_table_cell(
    State(state): State<Arc<HtmlState>>,
    Path((table_name, id, column)): Path<(String, String, String)>,
    Form(form): Form<TableValueForm>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let _ =
        application::table::table_patch_row(services.clone(), table_name, id, column, form.value)
            .await;
    notify_active_context(&state);

    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn open_table_cell(
    State(state): State<Arc<HtmlState>>,
    Path((table_name, id, column)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let value = services
        .repository
        .table
        .get_cell_value(table_name.clone(), id.clone(), column.clone())
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body_home_modal(
            services,
            table::editable_row::presentation_html_table_editable_row(
                table_name, id, column, value, None,
            )
            .await
            .into_string(),
        )
        .await,
    )
}

async fn delete_table_row(
    State(state): State<Arc<HtmlState>>,
    Path((table_name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let _ = services.repository.table.delete_by_id(table_name, id).await;
    notify_active_context(&state);
    datastar::patch_elements(
        "#main",
        "outer",
        section::main::presentation_html_section_main(services).await,
    )
}

async fn set_active_collection(
    State(state): State<Arc<HtmlState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let _ = services.repository.collection.set_active(&id).await;
    notify_active_context(&state);
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn toggle_collection_views(
    State(state): State<Arc<HtmlState>>,
    Path(collection_id): Path<u32>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let _ = services
        .repository
        .collection
        .toggle_by_collection_id(collection_id)
        .await;
    notify_active_context(&state);
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn toggle_single_view(
    State(state): State<Arc<HtmlState>>,
    Path((collection_id, view_id)): Path<(u32, u32)>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let _ = services
        .repository
        .collection
        .toggle_by_view_id(collection_id, view_id)
        .await;
    notify_active_context(&state);
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

async fn run_operation(
    State(state): State<Arc<HtmlState>>,
    Form(form): Form<OperationForm>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let result = application::operation::operation_execute(services.clone(), form.operation).await;
    notify_active_context(&state);
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
    State(state): State<Arc<HtmlState>>,
    Path(table_name): Path<String>,
) -> impl IntoResponse {
    let services = state.services.clone();
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
    State(state): State<Arc<HtmlState>>,
    Path(table_name): Path<String>,
    Form(form): Form<CreateRowForm>,
) -> impl IntoResponse {
    let services = state.services.clone();
    let parsed = DatabaseTable::from_str(&table_name).ok();
    let table_name = parsed
        .map(|table| table.as_table_name().to_string())
        .unwrap_or(table_name);
    let _ = services
        .repository
        .table
        .insert_row(table_name, form.values)
        .await;
    notify_active_context(&state);
    datastar::patch_elements(
        "#body",
        "outer",
        section::body::presentation_html_section_body(services).await,
    )
}

fn notify_active_context(state: &Arc<HtmlState>) {
    let _ = state.active_context_tx.send(());
}

async fn render_active_context_events(services: InjectedServices) -> Vec<Event> {
    vec![
        datastar::patch_elements_event(
            "#active-collection",
            "outer",
            collection::presentation_html_collection(services.clone())
                .await
                .into_string(),
        ),
        datastar::patch_elements_event(
            "#main",
            "outer",
            section::main::presentation_html_section_main(services).await,
        ),
    ]
}

struct ActiveContextStreamState {
    state: Arc<HtmlState>,
    rx: Option<broadcast::Receiver<()>>,
    pending_events: VecDeque<Event>,
    monitor_connection: Option<SqliteConnection>,
    last_data_version: Option<i64>,
    interval: tokio::time::Interval,
}

async fn next_active_context_event(
    mut stream_state: ActiveContextStreamState,
) -> Option<(Result<Event, Infallible>, ActiveContextStreamState)> {
    loop {
        if let Some(event) = stream_state.pending_events.pop_front() {
            return Some((Ok(event), stream_state));
        }

        if stream_state.rx.is_none() {
            stream_state.rx = Some(stream_state.state.active_context_tx.subscribe());
        }

        let receiver = stream_state.rx.as_mut()?;
        tokio::select! {
            result = receiver.recv() => match result {
                Ok(()) => {
                    stream_state.pending_events =
                        VecDeque::from(render_active_context_events(stream_state.state.services.clone()).await);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            },
            _ = stream_state.interval.tick() => {
                let Some(connection) = stream_state.monitor_connection.as_mut() else {
                    continue;
                };
                let Ok(current_data_version) = sqlite_data_version(connection).await else {
                    continue;
                };
                if stream_state.last_data_version != Some(current_data_version) {
                    stream_state.last_data_version = Some(current_data_version);
                    stream_state.pending_events =
                        VecDeque::from(render_active_context_events(stream_state.state.services.clone()).await);
                }
            }
        }
    }
}

async fn open_sqlite_monitor_connection() -> Option<SqliteConnection> {
    let options = sqlite_connect_options().ok()?;
    SqliteConnection::connect_with(&options).await.ok()
}

async fn sqlite_data_version(connection: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("PRAGMA data_version")
        .fetch_one(connection)
        .await
}
