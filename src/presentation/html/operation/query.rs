use maud::html;

pub async fn presentation_html_operation_query() -> String {
    html!(
       input class="daring"
           name="query"
           hx-params="*"
           hx-target="#main"
           hx-post="/operation/query"
           autofocus={}
           {}
    )
    .0
}
