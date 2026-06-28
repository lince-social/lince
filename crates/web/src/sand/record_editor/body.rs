use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="recordEditorApp" data-record-editor-root="" {}
    }
}
