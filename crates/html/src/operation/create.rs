use maud::{Markup, html};

pub async fn presentation_html_create(table: String, column_names: Vec<String>) -> Markup {
    html!(form.m_padding.glow
        data-on:submit__prevent=(format!(
            "@post('/operation/create/{}', {{contentType: 'form'}})",
            table
        ))
        {
        p {(table)}
       @for (i, column) in column_names.iter().enumerate() {
            @if column != "id" {
                p {(column)}
                @if i == 1 {
                    input name=(column) autofocus={} {}
                } @else {
                    input name=(column) {}
                }
           }
       }
       button type="submit" style="display: none;" {}
       button type="button" data-on:click=(format!("@get('/body')")) { "Cancel" }
    })
}
