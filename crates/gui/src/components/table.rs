// Record tem que ter propriedades do mundo real? Ou deve-se fazer uma gambiarra pelos usuários? tentar overreach de modelagem de banco
// pode ter um backfire porqua estamos complicando demais? se o usuário fizer uma query que pega tudo e se deparar com uma montanha de colunas
// vai ser meio assustador.
// Tmbém tem o fato de que nem todos os cadastros vao possuir certas propriedades, vai estar mal normalizdo com varias celulas vazias.
//
// Propriedades que se pode adicionar aos records:
// Localização atual
// Custos? O que que é um custo? É a contribuição necessária para ter isso. Pode ser outro record? Como? Com proposta de transferencia
// A Proposta de Transferencia é a relação entre um cadastro e seu custo.

use gpui::*;
use gpui_component::table::*;
use injection::cross_cutting::InjectedServices;
use ropey::Rope;
use std::collections::HashMap;

use crate::{
    app::{Backspace, Enter},
    themes::catppuccin_mocha::{base, blue, overlay0, sapphire, surface1},
};

pub type Row = HashMap<String, String>;
pub type Table = Vec<Row>;

#[derive(Clone, Debug, PartialEq)]
struct CellPosition {
    row_ix: usize,
    col_ix: usize,
}

pub struct GenericTableDelegate {
    data: Table,
    headers: Vec<String>,
    columns: Vec<Column>,
    // track column widths locally so resize events are preserved
    col_widths: Vec<f32>,
    table_name: String,
    services: InjectedServices,
    // Track editing state
    editing_cell: Option<CellPosition>,
    edit_value: Rope,
    focus_handle: FocusHandle,
}

impl Focusable for GenericTableDelegate {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl GenericTableDelegate {
    pub fn new(data: Table, table_name: String, services: InjectedServices, cx: &mut App) -> Self {
        let mut headers: Vec<String> = data
            .first()
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default();

        headers.sort();

        let columns = headers
            .iter()
            .map(|h| {
                Column::new(h.clone(), h.clone())
                    .sortable()
                    .movable(true)
                    .resizable(true)
            })
            .collect::<Vec<_>>();

        // default column width when none has been set yet
        let col_widths = vec![150.0; headers.len()];

        Self {
            data,
            headers,
            columns,
            col_widths,
            table_name,
            services,
            editing_cell: None,
            edit_value: Rope::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn save_cell_changes(&mut self, row_ix: usize, col_ix: usize, cx: &mut Context<TableState<Self>>) {
        let key = self.headers[col_ix].clone();
        let row_id = self.data[row_ix]
            .get("id")
            .map(|s| s.to_string())
            .unwrap_or_else(|| row_ix.to_string());

        let services = self.services.clone();
        let table = self.table_name.clone();
        let column = key.clone();
        let value = self.edit_value.to_string();

        // Update local data
        if let Some(row) = self.data.get_mut(row_ix) {
            row.insert(key, value.clone());
        }

        // Save to database
        cx.spawn(async move |_this, _cx| {
            if let Err(e) = application::table::table_patch_row(services, table, row_id, column, value).await {
                eprintln!("Failed to save cell: {}", e);
            }
        })
        .detach();
    }

    fn start_edit(&mut self, row_ix: usize, col_ix: usize, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        eprintln!("DEBUG: start_edit called for row={}, col={}", row_ix, col_ix);
        let key = &self.headers[col_ix];
        let value = self.data[row_ix]
            .get(key)
            .map(String::as_str)
            .unwrap_or("");

        eprintln!("DEBUG: Setting editing_cell, current value={:?}", value);
        self.editing_cell = Some(CellPosition { row_ix, col_ix });
        self.edit_value = Rope::from_str(value);
        
        // Focus the modal so keyboard input goes there instead of Operation bar
        window.focus(&self.focus_handle);
        
        cx.notify();
        eprintln!("DEBUG: start_edit completed, editing_cell={:?}", self.editing_cell);
    }

    fn finish_edit(&mut self, cx: &mut Context<TableState<Self>>) {
        if let Some(pos) = self.editing_cell.clone() {
            self.save_cell_changes(pos.row_ix, pos.col_ix, cx);
        }
        self.editing_cell = None;
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<TableState<Self>>) {
        self.editing_cell = None;
        cx.notify();
    }
}

impl TableDelegate for GenericTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.data.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let key = &self.headers[col_ix];
        let value = self.data[row_ix]
            .get(key)
            .map(String::as_str)
            .unwrap_or("NULL");

        let current_pos = CellPosition { row_ix, col_ix };
        let is_editing = self.editing_cell.as_ref() == Some(&current_pos);

        eprintln!("DEBUG: render_td row={} col={} is_editing={} editing_cell={:?}", 
                  row_ix, col_ix, is_editing, self.editing_cell);

        if is_editing {
            // Show modal editor
            let modal_content = div()
                .absolute()
                .top(px(50.))
                .left(px(50.))
                .right(px(50.))
                .bottom(px(50.))
                .bg(base())
                .border_1()
                .border_color(blue())
                .rounded_md()
                .shadow_lg()
                .flex()
                .flex_col()
                .p_4()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(18.))
                                .font_weight(FontWeight::BOLD)
                                .child("Edit Cell")
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .bg(overlay0())
                                .rounded_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(surface1()))
                                .child("✕")
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                                    this.delegate_mut().cancel_edit(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .bg(surface1())
                        .border_1()
                        .border_color(blue())
                        .rounded_xs()
                        .p_3()
                        .text_size(px(14.))
                        .line_height(px(20.))
                        .child(self.edit_value.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .justify_end()
                        .child(
                            div()
                                .px_4()
                                .py_2()
                                .bg(overlay0())
                                .rounded_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(surface1()))
                                .child("Cancel")
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                                    this.delegate_mut().cancel_edit(cx);
                                })),
                        )
                        .child(
                            div()
                                .px_4()
                                .py_2()
                                .bg(blue())
                                .text_color(base())
                                .rounded_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(sapphire()))
                                .child("Save")
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                                    this.delegate_mut().finish_edit(cx);
                                })),
                        ),
                );

            let backdrop = div()
                .absolute()
                .inset_0()
                .bg(gpui::rgba(0x00000088))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                    this.delegate_mut().cancel_edit(cx);
                }));

            div()
                .id(("cell_editor", row_ix * 1000 + col_ix))  // Give unique ID for focus management
                .relative()
                .p_1p5()
                .w_full()
                .h_full()
                .child(value.to_string())
                .child(backdrop)
                .child(modal_content)
                .focusable()  // Make the modal focusable first
                .track_focus(&self.focus_handle(cx))  // Then track focus
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                    if let Some(key_char) = &event.keystroke.key_char {
                        if key_char.len() == 1 && !event.keystroke.modifiers.control {
                            let delegate = this.delegate_mut();
                            let len = delegate.edit_value.len_chars();
                            delegate.edit_value.insert(len, key_char);
                            cx.notify();
                        }
                    }
                }))
                .on_action(cx.listener(|this, _event: &Backspace, _window, cx| {
                    let delegate = this.delegate_mut();
                    let len = delegate.edit_value.len_chars();
                    if len > 0 {
                        delegate.edit_value.remove(len - 1..len);
                    }
                    cx.notify();
                }))
                .on_action(cx.listener(|this, _event: &Enter, _window, cx| {
                    this.delegate_mut().finish_edit(cx);
                }))
        } else {
            // View mode - cell is clickable
            // We need to capture the click before the table component does
            div()
                .id(("cell_view", row_ix * 1000 + col_ix))
                .relative()
                .w_full()
                .h_full()
                .child(
                    // Inner clickable div that captures events
                    div()
                        .p_1p5()
                        .w_full()
                        .h_full()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgba(0xffffff11)))
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            eprintln!("DEBUG: Cell clicked (on_click) - row={}, col={}", row_ix, col_ix);
                            this.delegate_mut().start_edit(row_ix, col_ix, window, cx);
                        }))
                        .child(value.to_string())
                )
                .focusable()  // Make focusable to match the editing branch type
                .track_focus(&self.focus_handle(cx))  // Track focus to match the editing branch type
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let key = self.headers[col_ix].clone();

        match sort {
            ColumnSort::Ascending => {
                self.data.sort_by(|a, b| a.get(&key).cmp(&b.get(&key)));
            }
            ColumnSort::Descending => {
                self.data.sort_by(|a, b| b.get(&key).cmp(&a.get(&key)));
            }
            ColumnSort::Default => {}
        }
    }

    // called when the table reports a column has been moved via drag & drop
    // Implemented as `move_column` to match the `TableDelegate` trait.
    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let len = self.headers.len();
        if col_ix >= len || to_ix >= len || col_ix == to_ix {
            return;
        }

        // move header label
        let header = self.headers.remove(col_ix);
        self.headers.insert(to_ix, header);

        // move column metadata
        let column = self.columns.remove(col_ix);
        self.columns.insert(to_ix, column);

        // keep width vector in sync
        if !self.col_widths.is_empty() {
            let w = self.col_widths.remove(col_ix);
            self.col_widths.insert(to_ix, w);
        }
    }
}

