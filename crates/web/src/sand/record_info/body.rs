use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="recordInfoApp" data-record-info-root="" {}
    }
}
