use domain::{
    clean::{collection::Collection, table::Table},
    dirty::{
        operation::{
            DatabaseTable, OperationActions, OperationInstructionReceiver, ParsedOperation,
        },
        view::QueriedView,
    },
};
use injection::cross_cutting::InjectedServices;
use persistence::connection::sqlite_connect_options;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use sqlx::{Connection, SqliteConnection};
use std::{
    cmp,
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use application::operation::operation_execute;
use application::table::table_patch_row;

#[derive(Clone)]
pub(super) struct SqlViewTable {
    pub(super) source_table: String,
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
    Operation,
    CellInsert,
    Visual,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct CellKey {
    pub(super) view_id: u32,
    pub(super) row: usize,
    pub(super) col: usize,
}

#[derive(Clone)]
pub(super) struct CreationFieldState {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) cursor: usize,
}

#[derive(Clone)]
pub(super) struct CreationModalState {
    pub(super) table: DatabaseTable,
    pub(super) fields: Vec<CreationFieldState>,
    pub(super) active_field: usize,
}

pub(super) struct TuiState {
    pub(super) collections: Vec<CollectionEntry>,
    pub(super) active_collection_index: usize,
    pub(super) selected_view: usize,
    pub(super) selected_view_by_collection: HashMap<u32, u32>,
    pub(super) selected_row: usize,
    pub(super) selected_col: usize,
    pub(super) row_offset: usize,
    pub(super) last_visible_rows: usize,
    pub(super) command_input: String,
    pub(super) input_mode: InputMode,
    pub(super) edited_cells: HashMap<CellKey, String>,
    pub(super) cell_cursor: usize,
    pub(super) visual_anchor: Option<usize>,
    pub(super) show_help: bool,
    pub(super) show_collection_browser: bool,
    pub(super) creation_modal: Option<CreationModalState>,
    pub(super) create_shortcut_buffer: Option<String>,
    pub(super) create_shortcut_selection: usize,
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
            selected_view_by_collection: HashMap::new(),
            selected_row: 0,
            selected_col: 0,
            row_offset: 0,
            last_visible_rows: 0,
            command_input: String::new(),
            input_mode: InputMode::Normal,
            edited_cells: HashMap::new(),
            cell_cursor: 0,
            visual_anchor: None,
            show_help: false,
            show_collection_browser: false,
            creation_modal: None,
            create_shortcut_buffer: None,
            create_shortcut_selection: 0,
            error_toasts: Vec::new(),
        };
        state.clamp_view_selection();
        state.remember_selected_view_for_active_collection();
        Ok(state)
    }

    pub(super) async fn reload(&mut self, services: InjectedServices) -> std::io::Result<()> {
        self.remember_selected_view_for_active_collection();
        let selected_view_by_collection = self.selected_view_by_collection.clone();
        let command_input = self.command_input.clone();
        let input_mode = self.input_mode;
        let selected_row = self.selected_row;
        let selected_col = self.selected_col;
        let show_help = self.show_help;
        let row_offset = self.row_offset;
        let last_visible_rows = self.last_visible_rows;
        let show_collection_browser = self.show_collection_browser;
        let creation_modal = self.creation_modal.clone();
        let create_shortcut_buffer = self.create_shortcut_buffer.clone();
        let create_shortcut_selection = self.create_shortcut_selection;
        let error_toasts = self.error_toasts.clone();

        *self = Self::load(services).await?;
        self.selected_view_by_collection = selected_view_by_collection;
        self.restore_selected_view_for_active_collection();
        self.command_input = command_input;
        self.input_mode = if input_mode == InputMode::Operation {
            InputMode::Operation
        } else {
            InputMode::Normal
        };
        self.selected_row = selected_row;
        self.selected_col = selected_col;
        self.show_help = show_help;
        self.row_offset = row_offset;
        self.last_visible_rows = last_visible_rows;
        self.show_collection_browser = show_collection_browser;
        self.creation_modal = creation_modal;
        self.create_shortcut_buffer = create_shortcut_buffer;
        self.create_shortcut_selection = create_shortcut_selection;
        self.error_toasts = error_toasts;
        self.clamp_view_selection();
        self.clamp_cell_focus();
        self.clamp_cell_cursor();
        self.clear_visual_selection();
        self.clamp_create_shortcut_selection();
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

    fn focused_cell_column_name(&self) -> Option<String> {
        let table = self.selected_view()?.sql_table.as_ref()?;
        table.headers.get(self.selected_col).cloned()
    }

    fn focused_cell_row_id(&self) -> Option<String> {
        let table = self.selected_view()?.sql_table.as_ref()?;
        let id_index = table.headers.iter().position(|header| header == "id")?;
        let row = table.rows.get(self.selected_row)?;
        row.get(id_index).cloned().filter(|value| !value.is_empty())
    }

    fn original_selected_cell_text(&self) -> Option<String> {
        let key = self.selected_cell_key()?;
        self.selected_view()?
            .sql_table
            .as_ref()?
            .rows
            .get(key.row)?
            .get(key.col)
            .cloned()
    }

    fn focused_cell_source_table(&self) -> Option<String> {
        self.selected_view()?
            .sql_table
            .as_ref()
            .map(|table| table.source_table.clone())
    }

    pub(super) fn has_pending_focused_cell_edit(&self) -> bool {
        self.selected_cell_key()
            .map(|key| self.edited_cells.contains_key(&key))
            .unwrap_or(false)
    }

    pub(super) fn footer_input_text(&self) -> String {
        if self.input_mode == InputMode::Operation {
            return self.command_input.clone();
        }
        String::new()
    }

    pub(super) fn show_footer_cursor(&self) -> bool {
        self.input_mode == InputMode::Operation
    }

    pub(super) fn footer_cursor_offset(&self) -> usize {
        if self.input_mode == InputMode::Operation {
            return self.command_input.chars().count();
        }
        0
    }

    pub(super) fn begin_create_shortcut(&mut self) {
        self.create_shortcut_buffer = Some(String::new());
        self.create_shortcut_selection = 0;
        self.clear_visual_selection();
    }

    pub(super) fn creation_modal(&self) -> Option<&CreationModalState> {
        self.creation_modal.as_ref()
    }

    pub(super) fn set_creation_modal(&mut self, modal: Option<CreationModalState>) {
        self.creation_modal = modal;
        self.create_shortcut_buffer = None;
        self.create_shortcut_selection = 0;
        self.input_mode = InputMode::Normal;
        self.show_help = false;
        self.clear_visual_selection();
    }

    pub(super) fn create_shortcut_filter(&self) -> Option<&str> {
        self.create_shortcut_buffer.as_deref()
    }

    pub(super) fn create_shortcut_options(&self) -> Vec<(u32, DatabaseTable, String)> {
        let filter = self.create_shortcut_filter().unwrap_or("");
        (0..=13)
            .filter_map(|id| {
                let table = DatabaseTable::from_id(id)?;
                let id_text = id.to_string();
                if !filter.is_empty() && !id_text.starts_with(filter) {
                    return None;
                }
                Some((id, table, table.as_table_name().replace('_', " ")))
            })
            .collect()
    }

    fn clamp_create_shortcut_selection(&mut self) {
        let option_len = self.create_shortcut_options().len();
        if option_len == 0 {
            self.create_shortcut_selection = 0;
        } else {
            self.create_shortcut_selection = self.create_shortcut_selection.min(option_len - 1);
        }
    }

    pub(super) fn move_create_shortcut_selection(&mut self, delta: isize) {
        let option_len = self.create_shortcut_options().len();
        if option_len == 0 {
            self.create_shortcut_selection = 0;
            return;
        }
        let next =
            (self.create_shortcut_selection as isize + delta).rem_euclid(option_len as isize);
        self.create_shortcut_selection = next as usize;
    }

    pub(super) fn selected_create_shortcut_instruction(&self) -> Option<ParsedOperation> {
        let options = self.create_shortcut_options();
        let (_, table, _) = options.get(self.create_shortcut_selection)?;
        Some(ParsedOperation::new(OperationActions::Create, Some(*table)))
    }

    pub(super) fn rendered_selected_view_table(&self) -> Option<SqlViewTable> {
        let view = self.selected_view()?;
        let mut table = view.sql_table.clone()?;

        for (row_index, row) in table.rows.iter_mut().enumerate() {
            for (col_index, value) in row.iter_mut().enumerate() {
                let key = CellKey {
                    view_id: view.id,
                    row: row_index,
                    col: col_index,
                };
                if let Some(edited) = self.edited_cells.get(&key) {
                    *value = edited.clone();
                }
            }
        }

        Some(table)
    }

    pub(super) fn cell_selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.visual_anchor?;
        let cursor = self.cell_cursor.min(self.selected_cell_char_len());
        if anchor == cursor {
            None
        } else {
            Some((anchor.min(cursor), anchor.max(cursor)))
        }
    }

    pub(super) fn show_cell_cursor(&self) -> bool {
        matches!(self.input_mode, InputMode::CellInsert | InputMode::Visual)
    }

    pub(super) fn clear_visual_selection(&mut self) {
        self.visual_anchor = None;
    }

    pub(super) fn clamp_view_selection(&mut self) {
        if self.active_views().is_empty() {
            self.selected_view = 0;
        } else {
            self.selected_view = self.selected_view.min(self.active_views().len() - 1);
        }
        self.clamp_cell_cursor();
        self.clear_visual_selection();
        self.remember_selected_view_for_active_collection();
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
        self.clamp_cell_cursor();
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
        self.clamp_cell_cursor();
        self.clear_visual_selection();
        self.remember_selected_view_for_active_collection();
    }

    pub(super) fn next_view(&mut self) {
        if self.active_views().is_empty() {
            return;
        }
        self.selected_view = (self.selected_view + 1) % self.active_views().len();
        self.selected_row = 0;
        self.selected_col = 0;
        self.row_offset = 0;
        self.clamp_cell_cursor();
        self.clear_visual_selection();
        self.remember_selected_view_for_active_collection();
    }

    fn remember_selected_view_for_active_collection(&mut self) {
        let Some(collection_id) = self.active_collection().map(|entry| entry.collection.id) else {
            return;
        };
        let Some(view_id) = self.selected_view().map(|view| view.id) else {
            self.selected_view_by_collection.remove(&collection_id);
            return;
        };
        self.selected_view_by_collection
            .insert(collection_id, view_id);
    }

    fn restore_selected_view_for_active_collection(&mut self) {
        let Some(collection_id) = self.active_collection().map(|entry| entry.collection.id) else {
            return;
        };
        let Some(selected_view_id) = self
            .selected_view_by_collection
            .get(&collection_id)
            .copied()
        else {
            self.clamp_view_selection();
            return;
        };
        if let Some(index) = self
            .active_views()
            .iter()
            .position(|view| view.id == selected_view_id)
        {
            self.selected_view = index;
        }
        self.clamp_view_selection();
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
        self.clamp_cell_cursor();
        self.clear_visual_selection();
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
        self.clamp_cell_cursor();
        self.clear_visual_selection();
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
        self.clamp_cell_cursor();
        self.clear_visual_selection();
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
        self.clamp_cell_cursor();
        self.clear_visual_selection();
    }

    pub(super) fn half_page_down(&mut self) {
        let Some(view) = self.selected_view() else {
            return;
        };
        let Some(table) = view.sql_table.as_ref() else {
            return;
        };
        if table.rows.is_empty() {
            return;
        }
        let distance = (self.last_visible_rows.max(2) / 2).max(1);
        self.selected_row = cmp::min(self.selected_row + distance, table.rows.len() - 1);
        self.ensure_selected_row_visible();
        self.clamp_cell_cursor();
        self.clear_visual_selection();
    }

    pub(super) fn half_page_up(&mut self) {
        let distance = (self.last_visible_rows.max(2) / 2).max(1);
        self.selected_row = self.selected_row.saturating_sub(distance);
        self.ensure_selected_row_visible();
        self.clamp_cell_cursor();
        self.clear_visual_selection();
    }

    pub(super) fn enter_operation_mode(&mut self) {
        self.input_mode = InputMode::Operation;
        self.clear_visual_selection();
    }

    pub(super) fn enter_cell_insert_mode(&mut self) -> bool {
        if self.selected_cell_key().is_none() {
            return false;
        }
        self.input_mode = InputMode::CellInsert;
        self.clamp_cell_cursor();
        self.clear_visual_selection();
        true
    }

    pub(super) fn enter_visual_mode(&mut self) -> bool {
        if self.selected_cell_key().is_none() {
            return false;
        }
        self.input_mode = InputMode::Visual;
        self.clamp_cell_cursor();
        self.visual_anchor = Some(self.cell_cursor);
        true
    }

    pub(super) fn exit_to_normal_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.clear_visual_selection();
    }

    pub(super) fn selected_cell_key(&self) -> Option<CellKey> {
        let view = self.selected_view()?;
        let table = view.sql_table.as_ref()?;
        if self.selected_row >= table.rows.len() || self.selected_col >= table.headers.len() {
            return None;
        }
        Some(CellKey {
            view_id: view.id,
            row: self.selected_row,
            col: self.selected_col,
        })
    }

    pub(super) fn selected_cell_text(&self) -> Option<String> {
        let key = self.selected_cell_key()?;
        if let Some(edited) = self.edited_cells.get(&key) {
            return Some(edited.clone());
        }

        self.selected_view()?
            .sql_table
            .as_ref()?
            .rows
            .get(key.row)?
            .get(key.col)
            .cloned()
    }

    pub(super) fn selected_cell_char_len(&self) -> usize {
        self.selected_cell_text()
            .map(|text| text.chars().count())
            .unwrap_or(0)
    }

    pub(super) fn clamp_cell_cursor(&mut self) {
        self.cell_cursor = self.cell_cursor.min(self.selected_cell_char_len());
        if let Some(anchor) = self.visual_anchor {
            self.visual_anchor = Some(anchor.min(self.selected_cell_char_len()));
        }
    }

    pub(super) fn move_cell_cursor_left(&mut self) {
        self.cell_cursor = self.cell_cursor.saturating_sub(1);
        self.clamp_cell_cursor();
    }

    pub(super) fn move_cell_cursor_right(&mut self) {
        self.cell_cursor = cmp::min(self.cell_cursor + 1, self.selected_cell_char_len());
    }

    pub(super) fn move_cell_cursor_up(&mut self) {
        self.move_cell_cursor_vertically(-1);
    }

    pub(super) fn move_cell_cursor_down(&mut self) {
        self.move_cell_cursor_vertically(1);
    }

    fn move_cell_cursor_vertically(&mut self, delta: isize) {
        let Some(text) = self.selected_cell_text() else {
            return;
        };
        let (line_starts, total_chars) = line_starts(&text);
        if line_starts.is_empty() {
            self.cell_cursor = 0;
            return;
        }

        let cursor = self.cell_cursor.min(total_chars);
        let line_index = line_index_for_cursor(&line_starts, cursor);
        let line_start = line_starts[line_index];
        let line_end = line_end_for_index(&line_starts, total_chars, line_index);
        let column = cursor.min(line_end).saturating_sub(line_start);
        let target_line = (line_index as isize + delta).clamp(0, line_starts.len() as isize - 1);
        let target_index = target_line as usize;
        let target_start = line_starts[target_index];
        let target_end = line_end_for_index(&line_starts, total_chars, target_index);
        self.cell_cursor = target_start + column.min(target_end.saturating_sub(target_start));
    }

    pub(super) fn insert_char_in_selected_cell(&mut self, ch: char) {
        self.replace_selection_if_present();
        let Some(mut chars) = self
            .selected_cell_text()
            .map(|text| text.chars().collect::<Vec<_>>())
        else {
            return;
        };
        let cursor = self.cell_cursor.min(chars.len());
        chars.insert(cursor, ch);
        self.set_selected_cell_text(chars.iter().collect());
        self.cell_cursor = cursor + 1;
    }

    pub(super) fn backspace_in_selected_cell(&mut self) {
        if self.delete_selected_text() {
            return;
        }

        let Some(mut chars) = self
            .selected_cell_text()
            .map(|text| text.chars().collect::<Vec<_>>())
        else {
            return;
        };
        let cursor = self.cell_cursor.min(chars.len());
        if cursor == 0 {
            return;
        }
        chars.remove(cursor - 1);
        self.set_selected_cell_text(chars.iter().collect());
        self.cell_cursor = cursor - 1;
    }

    pub(super) fn delete_selected_text(&mut self) -> bool {
        let Some((start, end)) = self.cell_selection_range() else {
            return false;
        };
        let Some(mut chars) = self
            .selected_cell_text()
            .map(|text| text.chars().collect::<Vec<_>>())
        else {
            return false;
        };
        chars.drain(start..end);
        self.set_selected_cell_text(chars.iter().collect());
        self.cell_cursor = start;
        self.clear_visual_selection();
        true
    }

    fn replace_selection_if_present(&mut self) {
        let _ = self.delete_selected_text();
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.cell_selection_range()?;
        let text = self.selected_cell_text()?;
        Some(text.chars().skip(start).take(end - start).collect())
    }

    fn set_selected_cell_text(&mut self, value: String) {
        let Some(key) = self.selected_cell_key() else {
            return;
        };
        self.edited_cells.insert(key, value);
        self.clamp_cell_cursor();
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
            2
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

impl CreationModalState {
    fn new(table: DatabaseTable, fields: Vec<String>) -> Self {
        let fields = fields
            .into_iter()
            .filter(|field| field.to_lowercase() != "id")
            .map(|name| CreationFieldState {
                name,
                value: String::new(),
                cursor: 0,
            })
            .collect::<Vec<_>>();
        Self {
            table,
            fields,
            active_field: 0,
        }
    }

    pub(super) fn table_name(&self) -> &'static str {
        self.table.as_table_name()
    }

    fn active_field_mut(&mut self) -> Option<&mut CreationFieldState> {
        self.fields.get_mut(self.active_field)
    }

    fn move_field(&mut self, delta: isize) {
        if self.fields.is_empty() {
            self.active_field = 0;
            return;
        }
        let last = self.fields.len().saturating_sub(1) as isize;
        self.active_field = (self.active_field as isize + delta).clamp(0, last) as usize;
    }

    fn move_cursor_left(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor = field.cursor.saturating_sub(1);
        }
    }

    fn move_cursor_right(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor = cmp::min(field.cursor + 1, field.value.chars().count());
        }
    }

    fn move_cursor_home(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor = 0;
        }
    }

    fn move_cursor_end(&mut self) {
        if let Some(field) = self.active_field_mut() {
            field.cursor = field.value.chars().count();
        }
    }

    fn insert_char(&mut self, ch: char) {
        let Some(field) = self.active_field_mut() else {
            return;
        };
        let cursor = field.cursor.min(field.value.chars().count());
        let byte_index = field
            .value
            .char_indices()
            .nth(cursor)
            .map(|(index, _)| index)
            .unwrap_or(field.value.len());
        field.value.insert(byte_index, ch);
        field.cursor = cursor + 1;
    }

    fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    fn backspace(&mut self) {
        let Some(field) = self.active_field_mut() else {
            return;
        };
        if field.cursor == 0 {
            return;
        }
        let start_char = field.cursor - 1;
        let start = field
            .value
            .char_indices()
            .nth(start_char)
            .map(|(index, _)| index)
            .unwrap_or(0);
        let end = field
            .value
            .char_indices()
            .nth(field.cursor)
            .map(|(index, _)| index)
            .unwrap_or(field.value.len());
        field.value.replace_range(start..end, "");
        field.cursor = start_char;
    }

    fn values(&self) -> HashMap<String, String> {
        self.fields
            .iter()
            .filter_map(|field| {
                let trimmed = field.value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some((field.name.clone(), trimmed))
                }
            })
            .collect()
    }

    pub(super) fn field_value_with_cursor(&self, field_index: usize) -> String {
        let Some(field) = self.fields.get(field_index) else {
            return String::new();
        };
        if self.active_field != field_index {
            return field.value.clone();
        }
        let cursor = field.cursor.min(field.value.chars().count());
        let before = field.value.chars().take(cursor).collect::<String>();
        let after = field.value.chars().skip(cursor).collect::<String>();
        format!("{before}|{after}")
    }
}

#[derive(Default)]
struct TuiInstructionBuffer {
    instructions: Vec<ParsedOperation>,
}

impl TuiInstructionBuffer {
    fn into_vec(self) -> Vec<ParsedOperation> {
        self.instructions
    }
}

impl OperationInstructionReceiver for TuiInstructionBuffer {
    fn receive_operation_instruction(&mut self, instruction: ParsedOperation) {
        self.instructions.push(instruction);
    }
}

pub(super) async fn handle_key_event_result(
    state: &mut TuiState,
    services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
    if matches!(key.kind, KeyEventKind::Release) || matches!(key.code, KeyCode::Modifier(_)) {
        return Ok(false);
    }

    if is_quit_shortcut(&key) {
        return Ok(true);
    }

    if state.creation_modal.is_some() {
        handle_creation_modal_key(state, services, key).await?;
        return Ok(false);
    }

    if state.create_shortcut_buffer.is_some() {
        if let Some(instruction) = handle_create_shortcut_key(state, key) {
            handle_received_instructions(state, services, vec![instruction]).await?;
        }
        return Ok(false);
    }

    if state.input_mode == InputMode::Normal && is_create_shortcut_start(&key) {
        state.begin_create_shortcut();
        return Ok(false);
    }

    if is_dismiss_toast_shortcut(&key) {
        if state.has_pending_focused_cell_edit() {
            save_focused_cell_if_needed(state, services).await?;
            if matches!(state.input_mode, InputMode::CellInsert | InputMode::Visual) {
                state.exit_to_normal_mode();
            }
            return Ok(false);
        }
        state.dismiss_latest_toast();
        return Ok(false);
    }

    if is_help_shortcut(&key) {
        state.show_help = !state.show_help;
        return Ok(false);
    }

    if key.code == KeyCode::Tab {
        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            state.show_collection_browser = !state.show_collection_browser;
        }
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

    match state.input_mode {
        InputMode::Normal => handle_normal_mode_key(state, key).await?,
        InputMode::Operation => handle_operation_mode_key(state, services, key).await?,
        InputMode::CellInsert => handle_cell_insert_mode_key(state, services, key).await?,
        InputMode::Visual => handle_visual_mode_key(state, services, key).await?,
    };

    Ok(false)
}

async fn handle_normal_mode_key(state: &mut TuiState, key: KeyEvent) -> std::io::Result<bool> {
    match key.code {
        KeyCode::Char('i') if !has_non_shift_modifier(key.modifiers) => {
            if !state.enter_cell_insert_mode() {
                state.push_error("No focused cell available");
            }
        }
        KeyCode::Char('o') if !has_non_shift_modifier(key.modifiers) => {
            state.enter_operation_mode();
        }
        KeyCode::Char('v') if !has_non_shift_modifier(key.modifiers) => {
            if !state.enter_visual_mode() {
                state.push_error("No focused cell available");
            }
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
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.half_page_down();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.half_page_up();
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

async fn handle_operation_mode_key(
    state: &mut TuiState,
    services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
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
            state.exit_to_normal_mode();
        }
        KeyCode::Char(ch) if !has_non_shift_modifier(key.modifiers) => {
            state.command_input.push(ch);
        }
        _ => {}
    }

    Ok(false)
}

async fn handle_cell_insert_mode_key(
    state: &mut TuiState,
    _services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
    match key.code {
        KeyCode::Left => state.move_cell_cursor_left(),
        KeyCode::Right => state.move_cell_cursor_right(),
        KeyCode::Up => state.move_cell_cursor_up(),
        KeyCode::Down => state.move_cell_cursor_down(),
        KeyCode::Backspace => state.backspace_in_selected_cell(),
        KeyCode::Esc => state.exit_to_normal_mode(),
        KeyCode::Char(ch) if !has_non_shift_modifier(key.modifiers) => {
            state.insert_char_in_selected_cell(ch);
        }
        _ => {}
    }

    Ok(false)
}

async fn handle_visual_mode_key(
    state: &mut TuiState,
    _services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
    match key.code {
        KeyCode::Char('v') | KeyCode::Esc if !has_non_shift_modifier(key.modifiers) => {
            state.exit_to_normal_mode();
        }
        KeyCode::Left => state.move_cell_cursor_left(),
        KeyCode::Right => state.move_cell_cursor_right(),
        KeyCode::Up => state.move_cell_cursor_up(),
        KeyCode::Down => state.move_cell_cursor_down(),
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'h') =>
        {
            state.move_cell_cursor_left();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'l') =>
        {
            state.move_cell_cursor_right();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'j') =>
        {
            state.move_cell_cursor_down();
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'k') =>
        {
            state.move_cell_cursor_up();
        }
        KeyCode::Char('y') if !has_non_shift_modifier(key.modifiers) => {
            match state.selected_text() {
                Some(selected_text) => match copy_to_clipboard(&selected_text) {
                    Ok(()) => {}
                    Err(error) => state.push_error(format!("Clipboard copy failed: {error}")),
                },
                None => state.push_error("No selected text to copy"),
            }
            state.exit_to_normal_mode();
        }
        KeyCode::Char('d') if !has_non_shift_modifier(key.modifiers) => {
            if !state.delete_selected_text() {
                state.push_error("No selected text to delete");
            }
            state.exit_to_normal_mode();
        }
        _ => {}
    }

    Ok(false)
}

async fn save_focused_cell_if_needed(
    state: &mut TuiState,
    services: InjectedServices,
) -> std::io::Result<bool> {
    let Some(key) = state.selected_cell_key() else {
        return Ok(false);
    };
    let Some(edited_value) = state.edited_cells.get(&key).cloned() else {
        return Ok(false);
    };
    let Some(original_value) = state.original_selected_cell_text() else {
        return Ok(false);
    };

    if edited_value == original_value {
        state.edited_cells.remove(&key);
        return Ok(false);
    }

    let source_table = state.focused_cell_source_table().ok_or_else(|| {
        std::io::Error::other("Focused cell is not backed by a writable source table")
    })?;
    let row_id = state.focused_cell_row_id().ok_or_else(|| {
        std::io::Error::other("Focused row does not expose an id column for saving")
    })?;
    let column_name = state
        .focused_cell_column_name()
        .ok_or_else(|| std::io::Error::other("Focused column could not be resolved for saving"))?;

    table_patch_row(
        services.clone(),
        source_table,
        row_id,
        column_name,
        edited_value,
    )
    .await?;
    state.reload(services).await?;
    Ok(true)
}

async fn handle_creation_modal_key(
    state: &mut TuiState,
    services: InjectedServices,
    key: KeyEvent,
) -> std::io::Result<bool> {
    if key.code == KeyCode::Esc {
        state.set_creation_modal(None);
        return Ok(false);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches_char(key.code, 'w') {
        submit_creation_modal(state, services).await?;
        return Ok(false);
    }

    let Some(modal) = state.creation_modal.as_mut() else {
        return Ok(false);
    };

    match key.code {
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => modal.move_field(-1),
        KeyCode::BackTab => modal.move_field(-1),
        KeyCode::Tab | KeyCode::Down => modal.move_field(1),
        KeyCode::Up => modal.move_field(-1),
        KeyCode::Left => modal.move_cursor_left(),
        KeyCode::Right => modal.move_cursor_right(),
        KeyCode::Home => modal.move_cursor_home(),
        KeyCode::End => modal.move_cursor_end(),
        KeyCode::Backspace => modal.backspace(),
        KeyCode::Enter => modal.insert_newline(),
        KeyCode::Char(ch) if !has_non_shift_modifier(key.modifiers) => modal.insert_char(ch),
        _ => {}
    }

    Ok(false)
}

fn handle_create_shortcut_key(state: &mut TuiState, key: KeyEvent) -> Option<ParsedOperation> {
    let mut buffer = state.create_shortcut_buffer.take()?;

    match key.code {
        KeyCode::Esc => None,
        KeyCode::Backspace => {
            buffer.pop();
            state.create_shortcut_buffer = Some(buffer);
            state.create_shortcut_selection = 0;
            state.clamp_create_shortcut_selection();
            None
        }
        KeyCode::Enter => {
            state.create_shortcut_buffer = Some(buffer);
            let instruction = state.selected_create_shortcut_instruction();
            if instruction.is_none() {
                let filter = state
                    .create_shortcut_filter()
                    .unwrap_or_default()
                    .to_string();
                state.push_error(format!("No create target matches: {filter}"));
            } else {
                state.create_shortcut_buffer = None;
                state.create_shortcut_selection = 0;
            }
            instruction
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() && !has_non_shift_modifier(key.modifiers) => {
            buffer.push(ch);
            state.create_shortcut_buffer = Some(buffer);
            state.create_shortcut_selection = 0;
            state.clamp_create_shortcut_selection();
            None
        }
        KeyCode::Up | KeyCode::BackTab => {
            state.create_shortcut_buffer = Some(buffer);
            state.move_create_shortcut_selection(-1);
            None
        }
        KeyCode::Down | KeyCode::Tab => {
            state.create_shortcut_buffer = Some(buffer);
            state.move_create_shortcut_selection(1);
            None
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'k') =>
        {
            state.create_shortcut_buffer = Some(buffer);
            state.move_create_shortcut_selection(-1);
            None
        }
        KeyCode::Char(ch)
            if !has_non_shift_modifier(key.modifiers) && ch.eq_ignore_ascii_case(&'j') =>
        {
            state.create_shortcut_buffer = Some(buffer);
            state.move_create_shortcut_selection(1);
            None
        }
        _ => {
            state.create_shortcut_buffer = Some(buffer);
            None
        }
    }
}

async fn run_command(state: &mut TuiState, services: InjectedServices) -> std::io::Result<()> {
    let command = state.command_input.trim().to_string();
    if command.is_empty() {
        return Ok(());
    }

    match operation_execute(services.clone(), command.clone()).await {
        Ok(instructions) => {
            state.reload(services.clone()).await?;
            state.command_input.clear();
            handle_received_instructions(state, services, instructions).await?;
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
        Ok(instructions) => {
            state.reload(services.clone()).await?;
            handle_received_instructions(state, services, instructions).await?;
        }
        Err(error) => {
            state.push_error(format!("Operation failed: {error}"));
        }
    }

    Ok(())
}

async fn handle_received_instructions(
    state: &mut TuiState,
    services: InjectedServices,
    instructions: Vec<ParsedOperation>,
) -> std::io::Result<()> {
    let mut instruction_buffer = TuiInstructionBuffer::default();
    for instruction in instructions {
        instruction_buffer.receive_operation_instruction(instruction);
    }

    for instruction in instruction_buffer.into_vec() {
        match instruction.action {
            OperationActions::Create => {
                let Some(table) = instruction.table else {
                    state.push_error("Create action is missing a table");
                    continue;
                };
                let columns = services
                    .repository
                    .table
                    .get_columns(table.as_table_name().to_string())
                    .await?;
                state.set_creation_modal(Some(CreationModalState::new(table, columns)));
            }
            _ => {}
        }
    }

    Ok(())
}

async fn submit_creation_modal(
    state: &mut TuiState,
    services: InjectedServices,
) -> std::io::Result<()> {
    let Some(modal) = state.creation_modal() else {
        return Ok(());
    };
    let table_name = modal.table_name().to_string();
    let values = modal.values();

    application::write::table_insert_row(services.clone(), table_name, values).await?;
    state.set_creation_modal(None);
    state.reload(services).await?;
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
    application::write::set_active_collection(services.clone(), &next_collection_id.to_string())
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
    application::write::toggle_view(services.clone(), active_collection_id, selected_view_id)
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
        .map(|(view, (_, source_table, table))| {
            (
                view.id,
                SqlViewTable {
                    source_table,
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

fn is_create_shortcut_start(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && matches_char(key.code, 't')
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

fn line_starts(text: &str) -> (Vec<usize>, usize) {
    let chars = text.chars().collect::<Vec<_>>();
    let mut starts = vec![0];
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '\n' {
            starts.push(index + 1);
        }
    }
    (starts, chars.len())
}

fn line_index_for_cursor(line_starts: &[usize], cursor: usize) -> usize {
    line_starts
        .partition_point(|start| *start <= cursor)
        .saturating_sub(1)
}

fn line_end_for_index(line_starts: &[usize], total_chars: usize, index: usize) -> usize {
    line_starts
        .get(index + 1)
        .copied()
        .map(|next_start| next_start.saturating_sub(1))
        .unwrap_or(total_chars)
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    let commands = [
        ("wl-copy", Vec::<&str>::new()),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ];
    let mut last_error = None;

    for (program, args) in commands {
        let spawned = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };

        if let Some(stdin) = child.stdin.as_mut()
            && let Err(error) = stdin.write_all(text.as_bytes())
        {
            last_error = Some(error);
            continue;
        }

        match child.wait() {
            Ok(status) if status.success() => return Ok(()),
            Ok(_) => {
                last_error = Some(std::io::Error::other(format!(
                    "{program} exited with a non-zero status"
                )));
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| std::io::Error::other("no clipboard command available")))
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
