use maud::{Markup, html};

pub async fn presentation_web_create(table: String, column_names: Vec<String>) -> Markup {
    html!(form
        hx-post=(format!("/operation/create/{}", table))
        hx-swap="none"
        {
        p {(table)}
       @for column in column_names {
           p {(column)}
          input name=(column) {}
       }
       button type="submit" style="display: none;" {}
    })
}
