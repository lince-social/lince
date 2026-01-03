use gpui::*;
use gpui_component::table::*;

struct MyData {
    id: usize,
    name: String,
    age: u32,
    email: String,
}

struct MyTableDelegate {
    data: Vec<MyData>,
    columns: Vec<Column>,
}

impl MyTableDelegate {
    fn new() -> Self {
        Self {
            data: vec![
                MyData {
                    id: 1,
                    name: "John".to_string(),
                    age: 30,
                    email: "john@example.com".to_string(),
                },
                MyData {
                    id: 2,
                    name: "Jane".to_string(),
                    age: 25,
                    email: "jane@example.com".to_string(),
                },
            ],
            columns: vec![
                Column::new("id", "ID").width(60.).sortable().movable(true),
                Column::new("name", "Name")
                    .width(150.)
                    .sortable()
                    .movable(true),
                Column::new("age", "Age")
                    .width(80.)
                    .sortable()
                    .movable(true),
                Column::new("email", "Email")
                    .width(200.)
                    .sortable()
                    .movable(true),
            ],
        }
    }
}

impl TableDelegate for MyTableDelegate {
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
            "name" => div().bg(rgb(0xeeeeee)).child(row.name.clone()),
            "age" => div().bg(rgb(0x303030)).child(row.age.to_string()),
            "email" => div().bg(rgb(0x606060)).child(row.email.clone()),
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
            "name" => {
                match sort {
                    ColumnSort::Ascending => self.data.sort_by(|a, b| a.name.cmp(&b.name)),
                    ColumnSort::Descending => self.data.sort_by(|a, b| b.name.cmp(&a.name)),
                    ColumnSort::Default => {
                        // Reset to original order or default sort
                        self.data.sort_by(|a, b| a.id.cmp(&b.id));
                    }
                }
            }
            "age" => match sort {
                ColumnSort::Ascending => self.data.sort_by(|a, b| a.age.cmp(&b.age)),
                ColumnSort::Descending => self.data.sort_by(|a, b| b.age.cmp(&a.age)),
                ColumnSort::Default => self.data.sort_by(|a, b| a.id.cmp(&b.id)),
            },
            _ => {}
        }
    }
}
