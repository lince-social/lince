use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main { "Trail" }
        section #controlls {
            button data-on:click="@get('/host/trail/')" data-indicator:fetching {
                "Click me"
            }
        }
        section #content data-bind:nodes="" {
            
        }
    }
}
