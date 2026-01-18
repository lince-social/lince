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
use std::collections::HashMap;

pub type Row = HashMap<String, String>;
pub type Table = Vec<Row>;

pub struct GenericTableDelegate {
    data: Table,
    headers: Vec<String>,
    columns: Vec<Column>,
}

impl GenericTableDelegate {
    pub fn new(data: Table) -> Self {
        let mut headers: Vec<String> = data
            .first()
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default();

        headers.sort();

        let columns = headers
            .iter()
            .map(|h| Column::new(h.clone(), h.clone()).sortable().movable(true))
            .collect();

        Self {
            data,
            headers,
            columns,
        }
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
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let key = &self.headers[col_ix];
        let value = self.data[row_ix]
            .get(key)
            .map(String::as_str)
            .unwrap_or("NULL");

        div().p_1p5().child(value.to_string())
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
}
