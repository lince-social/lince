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
use gpui_component::table::{Column, ColumnSort, Table, TableDelegate, TableState};

pub struct Record {
    pub id: u32,
    pub quantity: SharedString,
    pub head: SharedString,
    pub body: SharedString,
}

pub struct TableView {
    table: Entity<TableState<MyRecordTableDelegate>>,
}

impl TableView {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = MyRecordTableDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx));

        Self { table }
    }
}

impl Render for TableView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(Table::new(&self.table))
    }
}

pub struct MyRecordTableDelegate {
    data: Vec<Record>,
    columns: Vec<Column>,
}

impl MyRecordTableDelegate {
    pub fn new() -> Self {
        Self {
            data: vec![
                Record {
                    id: 1,
                    quantity: SharedString::new("10.0"),
                    head: SharedString::new("Head"),
                    body: SharedString::new("Body"),
                },
                Record {
                    id: 2,
                    quantity: SharedString::new("20.0"),
                    head: SharedString::new("Head2"),
                    body: SharedString::new("Body2"),
                },
            ],
            columns: vec![
                Column::new("id", "ID").width(60.).sortable().movable(true),
                Column::new("quantity", "Quantity")
                    .width(150.)
                    .sortable()
                    .movable(true),
                Column::new("head", "Head")
                    .width(80.)
                    .sortable()
                    .movable(true),
                Column::new("body", "Body")
                    .width(200.)
                    .sortable()
                    .movable(true),
            ],
        }
    }
}

impl TableDelegate for MyRecordTableDelegate {
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
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.data[row_ix];
        let col = &self.columns[col_ix];

        match col.key.as_ref() {
            "id" => div().bg(rgb(0x808080)).child(row.id.to_string()),
            "head" => div().bg(rgb(0xeeeeee)).child(row.head.clone()),
            "body" => div().bg(rgb(0x303030)).child(row.body.to_string()),
            "quantity" => div().bg(rgb(0x606060)).child(row.quantity.to_string()),
            _ => div().bg(rgb(0x909090)).child("".to_string()),
        }
    }
    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let col = &self.columns[col_ix];

        match col.key.as_ref() {
            "head" => {
                match sort {
                    ColumnSort::Ascending => self.data.sort_by(|a, b| a.head.cmp(&b.head)),
                    ColumnSort::Descending => self.data.sort_by(|a, b| b.head.cmp(&a.head)),
                    ColumnSort::Default => {
                        // Reset to original order or default sort
                        self.data.sort_by(|a, b| a.id.cmp(&b.id));
                    }
                }
            }
            "quantity" => match sort {
                ColumnSort::Ascending => self.data.sort_by(|a, b| a.quantity.cmp(&b.quantity)),
                ColumnSort::Descending => self.data.sort_by(|a, b| b.quantity.cmp(&a.quantity)),
                ColumnSort::Default => self.data.sort_by(|a, b| a.id.cmp(&b.id)),
            },
            _ => {}
        }
    }
}
