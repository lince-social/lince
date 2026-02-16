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

use crate::themes::catppuccin_mocha::{blue, surface1};

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

    fn save_cell_changes(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        cx: &mut Context<TableState<Self>>,
    ) {
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
            if let Err(e) =
                application::table::table_patch_row(services, table, row_id, column, value).await
            {
                eprintln!("Failed to save cell: {}", e);
            }
        })
        .detach();
    }

    fn start_edit(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let key = &self.headers[col_ix];
        let value = self.data[row_ix].get(key).map(String::as_str).unwrap_or("");

        self.editing_cell = Some(CellPosition { row_ix, col_ix });
        self.edit_value = Rope::from_str(value);

        // Focus after the cell editor is mounted.
        window.defer(cx, {
            let focus_handle = self.focus_handle.clone();
            move |window, _cx| {
                window.focus(&focus_handle);
            }
        });

        cx.notify();
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

        if is_editing {
            let cell_id = row_ix
                .saturating_mul(self.headers.len())
                .saturating_add(col_ix);
            div()
                .id(("cell_editor", cell_id))
                .p_1p5()
                .w_full()
                .h_full()
                .bg(surface1())
                .border_1()
                .border_color(blue())
                .rounded_xs()
                .focusable()
                .track_focus(&self.focus_handle(cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _event, _window, cx| {
                        cx.stop_propagation();
                    }),
                )
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "enter" => {
                            this.delegate_mut().finish_edit(cx);
                            window.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        "escape" => {
                            this.delegate_mut().cancel_edit(cx);
                            window.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        "backspace" => {
                            let delegate = this.delegate_mut();
                            let len = delegate.edit_value.len_chars();
                            if len > 0 {
                                delegate.edit_value.remove(len - 1..len);
                            }
                            cx.notify();
                            window.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        _ => {}
                    }

                    if let Some(key_char) = &event.keystroke.key_char {
                        if key_char.chars().count() == 1
                            && !event.keystroke.modifiers.control
                            && !event.keystroke.modifiers.platform
                        {
                            let delegate = this.delegate_mut();
                            let len = delegate.edit_value.len_chars();
                            delegate.edit_value.insert(len, key_char);
                            cx.notify();
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    }
                }))
                .child(self.edit_value.to_string())
        } else {
            let cell_id = row_ix
                .saturating_mul(self.headers.len())
                .saturating_add(col_ix);
            div().id(("cell_view", cell_id)).w_full().h_full().child(
                div()
                    .p_1p5()
                    .w_full()
                    .h_full()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(0xffffff11)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.delegate_mut().start_edit(row_ix, col_ix, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(value.to_string()),
            )
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
