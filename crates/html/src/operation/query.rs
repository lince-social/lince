use maud::html;

pub async fn presentation_html_operation_query() -> String {
    html!(
       input class="daring"
           name="query"
           autofocus={}
           {}
    )
    .0
}
