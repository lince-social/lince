use maud::{html, Markup};

pub(super) fn body() -> Markup {
    html! {
        main class="app" {
            div class="toolbar" {
                label class="mode-switch" {
                    input id="mode-toggle" type="checkbox" aria-label="Alternar preview markdown";
                    span class="mode-switch__track" aria-hidden="true" {}
                    span id="mode-label" { "Raw" }
                }
            }
            textarea
                id="raw-input"
                class="raw"
                spellcheck="false"
                placeholder="# Notas\n\nEscreva em Markdown aqui."
            {}
            article id="preview" class="preview" hidden {}
        }
    }
}
