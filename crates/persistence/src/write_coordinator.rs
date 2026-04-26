use crate::connection::sqlite_connect_options;
use crate::repositories::view::{
    delete_view_dependencies_in_connection, sync_view_dependencies_for_view_in_connection,
};
use sqlx::{Connection, SqliteConnection};
use std::{
    collections::BTreeSet,
    io::{Error, ErrorKind},
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug, Clone, PartialEq)]
pub enum SqlParameter {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct WriteOutcome {
    pub rows_affected: u64,
    pub changed_tables: BTreeSet<String>,
    pub last_insert_rowid: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct InvalidationEvent {
    pub changed_tables: BTreeSet<String>,
}

#[derive(Clone)]
pub struct WriteCoordinatorHandle {
    request_tx: mpsc::Sender<WriteRequest>,
    invalidation_tx: broadcast::Sender<InvalidationEvent>,
}

enum WriteRequest {
    ExecuteStatement {
        sql: String,
        params: Vec<SqlParameter>,
        dependency_action: ViewDependencyAction,
        reply: oneshot::Sender<Result<WriteOutcome, Error>>,
    },
    ExecuteStatementReturningId {
        sql: String,
        params: Vec<SqlParameter>,
        dependency_action: ViewDependencyAction,
        reply: oneshot::Sender<Result<WriteOutcome, Error>>,
    },
}

#[derive(Debug, Clone, Copy, Default)]
enum ViewDependencyAction {
    #[default]
    None,
    RefreshInsertedView,
    RefreshView(i64),
    DeleteView(i64),
}

#[derive(Default)]
struct HookState {
    pending_tables: BTreeSet<String>,
    committed: bool,
}

impl WriteCoordinatorHandle {
    pub async fn execute_sql(&self, sql: impl Into<String>) -> Result<WriteOutcome, Error> {
        self.execute_statement(sql.into(), Vec::new()).await
    }

    pub async fn execute_statement(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_with_action(sql, params, ViewDependencyAction::None)
            .await
    }

    pub async fn execute_view_insert(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_with_action(sql, params, ViewDependencyAction::RefreshInsertedView)
            .await
    }

    pub async fn execute_view_update(
        &self,
        view_id: i64,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_with_action(sql, params, ViewDependencyAction::RefreshView(view_id))
            .await
    }

    pub async fn execute_view_delete(
        &self,
        view_id: i64,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_with_action(sql, params, ViewDependencyAction::DeleteView(view_id))
            .await
    }

    pub async fn execute_statement_returning_id(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_returning_id_with_action(sql, params, ViewDependencyAction::None)
            .await
    }

    pub async fn execute_view_insert_returning_id(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
    ) -> Result<WriteOutcome, Error> {
        self.execute_statement_returning_id_with_action(
            sql,
            params,
            ViewDependencyAction::RefreshInsertedView,
        )
        .await
    }

    async fn execute_statement_with_action(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
        dependency_action: ViewDependencyAction,
    ) -> Result<WriteOutcome, Error> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(WriteRequest::ExecuteStatement {
                sql,
                params,
                dependency_action,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::new(ErrorKind::BrokenPipe, "Write coordinator stopped"))?;

        reply_rx
            .await
            .map_err(|_| Error::new(ErrorKind::BrokenPipe, "Write coordinator dropped reply"))?
    }

    async fn execute_statement_returning_id_with_action(
        &self,
        sql: String,
        params: Vec<SqlParameter>,
        dependency_action: ViewDependencyAction,
    ) -> Result<WriteOutcome, Error> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(WriteRequest::ExecuteStatementReturningId {
                sql,
                params,
                dependency_action,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::new(ErrorKind::BrokenPipe, "Write coordinator stopped"))?;

        reply_rx
            .await
            .map_err(|_| Error::new(ErrorKind::BrokenPipe, "Write coordinator dropped reply"))?
    }

    pub fn subscribe_invalidations(&self) -> broadcast::Receiver<InvalidationEvent> {
        self.invalidation_tx.subscribe()
    }
}

pub async fn spawn_write_coordinator() -> Result<WriteCoordinatorHandle, Error> {
    let options = sqlite_connect_options()?;
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(Error::other)?;

    let hook_state = Arc::new(Mutex::new(HookState::default()));
    {
        let mut handle = connection.lock_handle().await.map_err(Error::other)?;

        let update_state = hook_state.clone();
        handle.set_update_hook(move |result| {
            if let Ok(mut state) = update_state.lock() {
                state.pending_tables.insert(result.table.to_string());
            }
        });

        let commit_state = hook_state.clone();
        handle.set_commit_hook(move || {
            if let Ok(mut state) = commit_state.lock() {
                state.committed = true;
            }
            true
        });

        let rollback_state = hook_state.clone();
        handle.set_rollback_hook(move || {
            if let Ok(mut state) = rollback_state.lock() {
                state.pending_tables.clear();
                state.committed = false;
            }
        });
    }

    let (request_tx, mut request_rx) = mpsc::channel::<WriteRequest>(64);
    let (invalidation_tx, _invalidation_rx) = broadcast::channel(256);
    let invalidation_sender = invalidation_tx.clone();

    tokio::spawn(async move {
        while let Some(request) = request_rx.recv().await {
            match request {
                WriteRequest::ExecuteStatement {
                    sql,
                    params,
                    dependency_action,
                    reply,
                } => {
                    let result = execute_statement(
                        &mut connection,
                        &hook_state,
                        &invalidation_sender,
                        sql,
                        params,
                        dependency_action,
                    )
                    .await;
                    let _ = reply.send(result);
                }
                WriteRequest::ExecuteStatementReturningId {
                    sql,
                    params,
                    dependency_action,
                    reply,
                } => {
                    let result = execute_statement_returning_id(
                        &mut connection,
                        &hook_state,
                        &invalidation_sender,
                        sql,
                        params,
                        dependency_action,
                    )
                    .await;
                    let _ = reply.send(result);
                }
            }
        }
    });

    Ok(WriteCoordinatorHandle {
        request_tx,
        invalidation_tx,
    })
}

async fn execute_statement(
    connection: &mut SqliteConnection,
    hook_state: &Arc<Mutex<HookState>>,
    invalidation_tx: &broadcast::Sender<InvalidationEvent>,
    sql: String,
    params: Vec<SqlParameter>,
    dependency_action: ViewDependencyAction,
) -> Result<WriteOutcome, Error> {
    if count_statements(&sql) != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Exactly one SQL statement is allowed per request",
        ));
    }

    reset_hook_state(hook_state)?;

    let mut query = sqlx::query(&sql);
    for param in params {
        query = match param {
            SqlParameter::Null => query.bind(None::<String>),
            SqlParameter::Integer(value) => query.bind(value),
            SqlParameter::Real(value) => query.bind(value),
            SqlParameter::Text(value) => query.bind(value),
        };
    }

    let result = query
        .execute(&mut *connection)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    let rows_affected = result.rows_affected();
    let last_insert_rowid = result.last_insert_rowid();
    let returned_last_insert_rowid = if rows_affected > 0
        && (matches!(dependency_action, ViewDependencyAction::RefreshInsertedView)
            || is_insert_statement(&sql))
    {
        Some(last_insert_rowid)
    } else {
        None
    };

    if rows_affected > 0 {
        apply_view_dependency_action(connection, dependency_action, last_insert_rowid).await?;
    }

    let (committed, mut changed_tables) = take_hook_state(hook_state)?;
    changed_tables.remove("view_dependency");

    if committed && !changed_tables.is_empty() {
        let _ = invalidation_tx.send(InvalidationEvent {
            changed_tables: changed_tables.clone(),
        });
    }

    Ok(WriteOutcome {
        rows_affected,
        changed_tables,
        last_insert_rowid: returned_last_insert_rowid,
    })
}

async fn execute_statement_returning_id(
    connection: &mut SqliteConnection,
    hook_state: &Arc<Mutex<HookState>>,
    invalidation_tx: &broadcast::Sender<InvalidationEvent>,
    sql: String,
    params: Vec<SqlParameter>,
    dependency_action: ViewDependencyAction,
) -> Result<WriteOutcome, Error> {
    if count_statements(&sql) != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Exactly one SQL statement is allowed per request",
        ));
    }

    reset_hook_state(hook_state)?;

    let mut query = sqlx::query_scalar::<_, i64>(&sql);
    for param in params {
        query = match param {
            SqlParameter::Null => query.bind(None::<String>),
            SqlParameter::Integer(value) => query.bind(value),
            SqlParameter::Real(value) => query.bind(value),
            SqlParameter::Text(value) => query.bind(value),
        };
    }

    let returned_id = query
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| Error::new(ErrorKind::InvalidInput, error.to_string()))?;
    let rows_affected = u64::from(returned_id.is_some());

    if let Some(returned_id_value) = returned_id {
        apply_view_dependency_action(connection, dependency_action, returned_id_value).await?;
    }

    let (committed, mut changed_tables) = take_hook_state(hook_state)?;
    changed_tables.remove("view_dependency");

    if committed && !changed_tables.is_empty() {
        let _ = invalidation_tx.send(InvalidationEvent {
            changed_tables: changed_tables.clone(),
        });
    }

    Ok(WriteOutcome {
        rows_affected,
        changed_tables,
        last_insert_rowid: returned_id,
    })
}

fn reset_hook_state(hook_state: &Arc<Mutex<HookState>>) -> Result<(), Error> {
    let mut state = hook_state
        .lock()
        .map_err(|_| Error::other("Hook state mutex poisoned"))?;
    state.pending_tables.clear();
    state.committed = false;
    Ok(())
}

fn take_hook_state(hook_state: &Arc<Mutex<HookState>>) -> Result<(bool, BTreeSet<String>), Error> {
    let mut state = hook_state
        .lock()
        .map_err(|_| Error::other("Hook state mutex poisoned"))?;
    let committed = state.committed;
    let changed_tables = std::mem::take(&mut state.pending_tables);
    state.committed = false;
    Ok((committed, changed_tables))
}

async fn apply_view_dependency_action(
    connection: &mut SqliteConnection,
    action: ViewDependencyAction,
    last_insert_rowid: i64,
) -> Result<(), Error> {
    match action {
        ViewDependencyAction::None => Ok(()),
        ViewDependencyAction::RefreshInsertedView => {
            sync_view_dependencies_for_view_in_connection(connection, last_insert_rowid).await
        }
        ViewDependencyAction::RefreshView(view_id) => {
            sync_view_dependencies_for_view_in_connection(connection, view_id).await
        }
        ViewDependencyAction::DeleteView(view_id) => {
            delete_view_dependencies_in_connection(connection, view_id).await
        }
    }
}

fn count_statements(sql: &str) -> usize {
    let mut count = 0;
    let mut has_content = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                has_content = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_content = true;
            }
            ';' if !in_single && !in_double => {
                if has_content {
                    count += 1;
                    has_content = false;
                }
            }
            ch if ch.is_whitespace() && !in_single && !in_double => {}
            '-' if !in_single && !in_double && chars.peek() == Some(&'-') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            _ => {
                has_content = true;
            }
        }
    }

    if has_content {
        count += 1;
    }

    count
}

fn is_insert_statement(sql: &str) -> bool {
    sql.trim_start()
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("insert"))
}
