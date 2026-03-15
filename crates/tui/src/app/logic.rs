use domain::{
    clean::{collection::Collection, table::Table},
    dirty::view::QueriedView,
};
use injection::cross_cutting::InjectedServices;
use persistence::connection::sqlite_connect_options;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use sqlx::{Connection, SqliteConnection};
use std::{
    cmp,
    collections::HashMap,
    time::{Duration, Instant},
};

use application::operation::operation_execute;

#[derive(Clone)]
pub(super) struct SqlViewTable {
    pub(super) headers: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
}

#[derive(Clone)]
pub(super) struct ActiveViewItem {
    pub(super) id: u32,
    pub(super) name: String,
    pub(super) quantity: i32,
    pub(super) sql_table: Option<SqlViewTable>,
}

#[derive(Clone)]
pub(super) struct CollectionEntry {
    pub(super) collection: Collection,
    pub(super) views: Vec<ActiveViewItem>,
}

#[derive(Clone)]
pub(super) struct ErrorToast {
    pub(super) message: String,
    pub(super) expires_at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    Normal,
    Insert,
}

pub(super) struct TuiState {
    pub(super) collections: Vec<CollectionEntry>,
    pub(super) active_collection_index: usize,
    pub(super) selected_view: usize,
    pub(super) selected_row: usize,
    pub(super) selected_col: usize,
    pub(super) row_offset: usize,
    pub(super) last_visible_rows: usize,
    pub(super) command_input: String,
    pub(super) input_mode: InputMode,
    pub(super) show_help: bool,
    pub(super) show_collection_browser: bool,
    pub(super) error_toasts: Vec<ErrorToast>,
}

impl TuiState {
    pub(super) async fn load(services: InjectedServices) -> std::io::Result<Self> {
        let mut collections_with_views = services.repository.collection.get_all().await?;
        collections_with_views.sort_by_key(|(collection, _)| collection.id);

        let active_collection_index = collections_with_views
            .iter()
            .position(|(collection, _)| collection.quantity == 1)
            .unwrap_or(0);

        let active_views = collections_with_views
            .get(active_collection_index)
            .map(|(_, views)| views.clone())
            .unwrap_or_default();
        let (tables, _) = services
            .repository
            .collection
            .get_active_view_data()
            .await?;
        let sql_rendered = build_sql_rendered_tables(&active_views, tables);

        let mut collections = collections_with_views
            .into_iter()
            .map(|(collection, mut views)| {
                views.sort_by_key(|view| view.id);
                let views = views
                    .into_iter()
                    .filter(|view| !is_special_query(&view.query))
                    .map(|view| ActiveViewItem {
                        id: view.id,
                        name: view.name,
                        quantity: view.quantity,
                        sql_table: if collection.quantity == 1 {
                            sql_rendered.get(&view.id).cloned()
                        } else {
                            None
                        },
                    })
                    .collect::<Vec<_>>();
                CollectionEntry { collection, views }
            })
            .collect::<Vec<_>>();
        collections.sort_by_key(|entry| entry.collection.id);

        let mut state = Self {
            collections,
            active_collection_index,
            selected_view: 0,
            selected_row: 0,
            selected_col: 0,
            row_offset: 0,
            last_visible_rows: 0,
            command_input: String::new(),
            input_mode: InputMode::Normal,
            show_help: false,
            show_collection_browser: false,
            error_toasts: Vec::new(),
        };
        state.clamp_view_selection();
        Ok(state)
    }

    pub(super) async fn reload(&mut self, services: InjectedServices) -> std::io::Result<()> {
        let selected_view_id = self.selected_view().map(|view| view.id);
        let command_input = self.command_input.clone();
        let input_mode = self.input_mode;
        let selected_row = self.selected_row;
        let selected_col = self.selected_col;
        let show_help = self.show_help;
        let row_offset = self.row_offset;
        let last_visible_rows = self.last_visible_rows;
        let show_collection_browser = self.show_collection_browser;
        let error_toasts = self.error_toasts.clone();

        *self = Self::load(services).await?;
        if let Some(selected_view_id) = selected_view_id
            && let Some(index) = self
                .active_views()
                .iter()
                .position(|view| view.id == selected_view_id)
        {
            self.selected_view = index;
        }
        self.command_input = command_input;
        self.input_mode = input_mode;
        self.selected_row = selected_row;
        self.selected_col = selected_col;
        self.show_help = show_help;
        self.row_offset = row_offset;
        self.last_visible_rows = last_visible_rows;
        self.show_collection_browser = show_collection_browser;
        self.error_toasts = error_toasts;
        self.clamp_view_selection();
        self.clamp_cell_focus();
        self.clamp_scroll();
        Ok(())
    }

    pub(super) fn active_collection(&self) -> Option<&CollectionEntry> {
        self.collections.get(self.active_collection_index)
    }

    pub(super) fn active_views(&self) -> &[ActiveViewItem] {
        self.active_collection()
            .map(|entry| entry.views.as_slice())
            .unwrap_or(&[])
    }

    pub(super) fn selected_view(&self) -> Option<&ActiveViewItem> {
        self.active_views().get(self.selected_view)
    }

    fn focused_row_id_operation(&self) -> Option<String> {
        let table = self.selected_view()?.sql_table.as_ref()?;
        let id_index = table.headers.iter().position(|header| header == "id")?;
        let row = table.rows.get(self.selected_row)?;
        row.get(id_index).cloned().filter(|value| !value.is_empty())
    }

    pub(super) fn clamp_view_selection(&mut self) {
        if self.active_views().is_empty() {
            self.selected_view = 0;
        } else {
            self.selected_view = self.selected_view.min(self.active_views().len() - 1);
        }
    }

    pub(super) fn clamp_cell_focus(&mut self) {
        let Some((row_len, col_len)) = self
            .selected_view()
            .and_then(|view| view.sql_table.as_ref())
            .map(|table| (table.rows.len(), table.headers.len()))
        else {
            self.selected_row = 0;
            self.selected_col = 0;
            return;
        };
        self.selected_row = self.selected_row.min(row_len.saturating_sub(1));
        self.selected_col = self.selected_col.min(col_len.saturating_sub(1));
    }

    pub(super) fn clamp_scroll(&mut self) {
        let max_offset = self
            .selected_view()
            .and_then(|view| view.sql_table.as_ref())
            .map(|table| {
                table
                    .rows
                    .len()
                    .saturating_sub(self.last_visible_rows.max(1))
            })
            .unwrap_or(0);
        self.row_offset = self.row_offset.min(max_offset);
        self.ensure_selected_row_visible();
    }

    pub(super) fn previous_view(&mut self) {
        if self.active_views().is_empty() {
            return;
        }
        if self.selected_view == 0 {
            self.selected_view = self.active_views().len() - 1;
        } else {
            self.selected_view -= 1;
        }
        self.selected_row = 0;
        self.selected_col = 0;
        self.row_offset = 0;
    }

    pub(super) fn next_view(&mut self) {
        if self.active_views().is_empty() {
            return;
        }
        self.selected_view = (self.selected_view + 1) % self.active_views().len();
        self.selected_row = 0;
        self.selected_col = 0;
        self.row_offset = 0;
    }

    pub(super) fn previous_cell_column(&mut self) {
        let Some(view) = self.selected_view() else {
            return;
        };
        let Some(table) = view.sql_table.as_ref() else {
            return;
        };
        if table.headers.is_empty() {
            self.selected_col = 0;
            return;
        }
        self.selected_col = if self.selected_col == 0 {
            table.headers.len() - 1
        } else {
            self.selected_col - 1
        };
    }

    pub(super) fn next_cell_column(&mut self) {
        let Some(view) = self.selected_view() else {
            return;
        };
        let Some(table) = view.sql_table.as_ref() else {
            return;
        };
        if table.headers.is_empty() {
            self.selected_col = 0;
            return;
        }
        self.selected_col = (self.selected_col + 1) % table.headers.len();
    }

    pub(super) fn next_cell_row(&mut self) {
        let Some(view) = self.selected_view() else {
            return;
        };
        let Some(table) = view.sql_table.as_ref() else {
            return;
        };
        if table.rows.is_empty() {
            self.selected_row = 0;
            return;
        }
        self.selected_row = (self.selected_row + 1) % table.rows.len();
        self.ensure_selected_row_visible();
    }

    pub(super) fn previous_cell_row(&mut self) {
        let Some(view) = self.selected_view() else {
            return;
        };
        let Some(table) = view.sql_table.as_ref() else {
            return;
        };
        if table.rows.is_empty() {
            self.selected_row = 0;
            return;
        }
        self.selected_row = if self.selected_row == 0 {
            table.rows.len() - 1
        } else {
            self.selected_row - 1
        };
        self.ensure_selected_row_visible();
    }

    pub(super) fn ensure_selected_row_visible(&mut self) {
        if self.last_visible_rows == 0 {
            return;
        }
        if self.selected_row < self.row_offset {
            self.row_offset = self.selected_row;
            return;
        }
        let visible_end = self.row_offset + self.last_visible_rows.saturating_sub(1);
        if self.selected_row > visible_end {
            self.row_offset = self
                .selected_row
                .saturating_sub(self.last_visible_rows.saturating_sub(1));
        }
    }

    pub(super) fn scroll_up(&mut self) {
        self.row_offset = self.row_offset.saturating_sub(1);
    }

    pub(super) fn scroll_down(&mut self) {
        let max_offset = self
            .selected_view()
            .and_then(|view| view.sql_table.as_ref())
            .map(|table| {
                table
                    .rows
                    .len()
                    .saturating_sub(self.last_visible_rows.max(1))
            })
            .unwrap_or(0);
        self.row_offset = cmp::min(self.row_offset + 1, max_offset);
    }

    pub(super) fn page_up(&mut self) {
        self.row_offset = self
            .row_offset
            .saturating_sub(self.last_visible_rows.max(1));
    }

    pub(super) fn page_down(&mut self) {
        let page = self.last_visible_rows.max(1);
        let max_offset = self
            .selected_view()
            .and_then(|view| view.sql_table.as_ref())
            .map(|table| table.rows.len().saturating_sub(page))
            .unwrap_or(0);
        self.row_offset = cmp::min(self.row_offset + page, max_offset);
    }

    pub(super) fn scroll_to_top(&mut self) {
        self.row_offset = 0;
    }

    pub(super) fn scroll_to_bottom(&mut self) {
        self.row_offset = self
            .selected_view()
            .and_then(|view| view.sql_table.as_ref())
            .map(|table| {
                table
                    .rows
                    .len()
                    .saturating_sub(self.last_visible_rows.max(1))
            })
            .unwrap_or(0);
    }

    pub(super) fn header_height(&self) -> u16 {
        if self.show_collection_browser {
            self.collections.len().max(1) as u16 + 2
        } else {
            3
        }
    }

    pub(super) fn push_error(&mut self, message: impl Into<String>) {
        self.error_toasts.push(ErrorToast {
            message: message.into(),
            expires_at: Instant::now() + Duration::from_secs(5),
        });
    }

    pub(super) fn prune_toasts(&mut self) {
        let now = Instant::now();
        self.error_toasts.retain(|toast| toast.expires_at > now);
    }

    pub(super) fn dismiss_latest_toast(&mut self) {
        self.error_toasts.pop();
    }
}

pub(super) async fn handle_key_event_result(
    state: &mut TuiState,
    services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
    if matches!(key.code, KeyCode::Modifier(_)) {
        return Ok(false);
    }

    if is_quit_shortcut(&key) {
        return Ok(true);
    }

    if is_help_shortcut(&key) {
        state.show_help = !state.show_help;
        return Ok(false);
    }

    if key.code == KeyCode::Tab && key.kind == KeyEventKind::Press {
        state.show_collection_browser = !state.show_collection_browser;
        return Ok(false);
    }

    if is_dismiss_toast_shortcut(&key) {
        state.dismiss_latest_toast();
        return Ok(false);
    }

    if is_focused_row_operation_shortcut(&key) {
        execute_focused_row_operation(state, services).await?;
        return Ok(false);
    }

    if state.show_help {
        if key.code == KeyCode::Esc {
            state.show_help = false;
        }
        return Ok(false);
    }

    if is_collection_next_shortcut(&key) {
        cycle_active_collection(state, services, 1).await?;
        return Ok(false);
    }

    if is_collection_previous_shortcut(&key) {
        cycle_active_collection(state, services, -1).await?;
        return Ok(false);
    }

    if is_view_previous_shortcut(&key) {
        state.previous_view();
        return Ok(false);
    }

    if is_view_next_shortcut(&key) {
        state.next_view();
        return Ok(false);
    }

    if is_toggle_view_shortcut(&key) {
        toggle_selected_view(state, services).await?;
        return Ok(false);
    }

    if state.input_mode == InputMode::Normal {
        return handle_normal_mode_key(state, key).await;
    }

    match key.code {
        KeyCode::Up => state.scroll_up(),
        KeyCode::Down => state.scroll_down(),
        KeyCode::PageUp => state.page_up(),
        KeyCode::PageDown => state.page_down(),
        KeyCode::Home => state.scroll_to_top(),
        KeyCode::End => state.scroll_to_bottom(),
        KeyCode::Enter => run_command(state, services).await?,
        KeyCode::Backspace => {
            state.command_input.pop();
        }
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
        }
        KeyCode::Char(ch) if !has_non_shift_modifier(key.modifiers) => {
            state.command_input.push(ch);
        }
        _ => {}
    }

    Ok(false)
}

async fn handle_normal_mode_key(state: &mut TuiState, key: KeyEvent) -> std::io::Result<bool> {
    match key.code {
        KeyCode::Char('i') if !has_non_shift_modifier(key.modifiers) => {
            state.input_mode = InputMode::Insert;
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'h') =>
        {
            state.previous_cell_column();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'l') =>
        {
            state.next_cell_column();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'j') =>
        {
            state.next_cell_row();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'k') =>
        {
            state.previous_cell_row();
        }
        KeyCode::Up => state.scroll_up(),
        KeyCode::Down => state.scroll_down(),
        KeyCode::PageUp => state.page_up(),
        KeyCode::PageDown => state.page_down(),
        KeyCode::Home => state.scroll_to_top(),
        KeyCode::End => state.scroll_to_bottom(),
        _ => {}
    }

    Ok(false)
}

async fn run_command(state: &mut TuiState, services: InjectedServices) -> std::io::Result<()> {
    let command = state.command_input.trim().to_string();
    if command.is_empty() {
        return Ok(());
    }

    match operation_execute(services.clone(), command.clone()).await {
        Ok(_) => {
            state.reload(services).await?;
            state.command_input.clear();
        }
        Err(error) => {
            state.push_error(format!("Operation failed: {error}"));
        }
    }

    Ok(())
}

async fn execute_focused_row_operation(
    state: &mut TuiState,
    services: InjectedServices,
) -> std::io::Result<()> {
    let Some(operation) = state.focused_row_id_operation() else {
        state.push_error("Focused row does not have an id column");
        return Ok(());
    };

    match operation_execute(services.clone(), operation).await {
        Ok(_) => {
            state.reload(services).await?;
        }
        Err(error) => {
            state.push_error(format!("Operation failed: {error}"));
        }
    }

    Ok(())
}

async fn cycle_active_collection(
    state: &mut TuiState,
    services: InjectedServices,
    direction: isize,
) -> std::io::Result<()> {
    if state.collections.is_empty() {
        return Ok(());
    }

    let len = state.collections.len() as isize;
    let next_index = (state.active_collection_index as isize + direction).rem_euclid(len) as usize;
    let next_collection_id = state.collections[next_index].collection.id;
    services
        .repository
        .collection
        .set_active(&next_collection_id.to_string())
        .await?;
    state.reload(services).await?;
    Ok(())
}

async fn toggle_selected_view(
    state: &mut TuiState,
    services: InjectedServices,
) -> std::io::Result<()> {
    let Some(active_collection_id) = state.active_collection().map(|entry| entry.collection.id)
    else {
        return Ok(());
    };
    let Some(selected_view_id) = state.selected_view().map(|view| view.id) else {
        return Ok(());
    };
    services
        .repository
        .collection
        .toggle_by_view_id(active_collection_id, selected_view_id)
        .await?;
    state.reload(services).await?;
    Ok(())
}

fn build_sql_rendered_tables(
    active_views: &[QueriedView],
    tables: Vec<(u32, String, Table)>,
) -> HashMap<u32, SqlViewTable> {
    active_views
        .iter()
        .filter(|view| view.quantity == 1 && !is_special_query(&view.query))
        .zip(tables)
        .map(|(view, (_, _, table))| {
            (
                view.id,
                SqlViewTable {
                    headers: table_headers(&table),
                    rows: table_rows(&table),
                },
            )
        })
        .collect()
}

fn table_headers(table: &Table) -> Vec<String> {
    let mut headers = table
        .first()
        .map(|row| row.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    headers.sort();
    headers
}

fn table_rows(table: &Table) -> Vec<Vec<String>> {
    let headers = table_headers(table);
    table
        .iter()
        .map(|row| {
            headers
                .iter()
                .map(|header| row.get(header).cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn is_special_query(query: &str) -> bool {
    let normalized = query.trim().to_lowercase().replace(['-', ' '], "_");
    matches!(
        normalized.as_str(),
        "karma_orchestra" | "karma_view" | "testing" | "command_buffer"
    ) || normalized.starts_with("create_view_")
        || normalized.starts_with("creation_view_")
        || normalized.starts_with("create_modal_")
        || normalized.starts_with("creation_modal_")
        || normalized.starts_with("cv_")
}

fn is_quit_shortcut(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'q'))
}

fn is_help_shortcut(key: &KeyEvent) -> bool {
    key.code == KeyCode::F(1)
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('/') | KeyCode::Char('?')))
}

fn is_dismiss_toast_shortcut(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches_char(key.code, 'w')
}

fn is_focused_row_operation_shortcut(key: &KeyEvent) -> bool {
    !has_non_shift_modifier(key.modifiers) && matches_char(key.code, 'x')
}

fn is_view_previous_shortcut(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && matches_char(key.code, 'h')
}

fn is_view_next_shortcut(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && matches_char(key.code, 'l')
}

fn is_collection_next_shortcut(key: &KeyEvent) -> bool {
    (key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::SHIFT))
        && matches_char(key.code, 'j')
}

fn is_collection_previous_shortcut(key: &KeyEvent) -> bool {
    (key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::SHIFT))
        && matches_char(key.code, 'k')
}

fn is_toggle_view_shortcut(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Char(' '))
}

fn matches_char(code: KeyCode, expected: char) -> bool {
    matches!(code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&expected))
}

fn has_non_shift_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
        || modifiers.contains(KeyModifiers::ALT)
        || modifiers.contains(KeyModifiers::SUPER)
        || modifiers.contains(KeyModifiers::HYPER)
        || modifiers.contains(KeyModifiers::META)
}

pub(super) async fn open_sqlite_monitor_connection() -> Option<SqliteConnection> {
    let options = sqlite_connect_options().ok()?;
    SqliteConnection::connect_with(&options).await.ok()
}

pub(super) async fn sqlite_data_version(
    connection: &mut SqliteConnection,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("PRAGMA data_version")
        .fetch_one(connection)
        .await
}
