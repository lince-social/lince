use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main
            class="app"
            data-signals="{ rendered: false, rawText: '' }"
            data-effect="window.MarkdownNotes?.sync($rawText, $rendered)"
        {
            div class="toolbar" {
                label class="mode-switch" {
                    input id="mode-toggle" type="checkbox" aria-label="Alternar preview markdown" data-bind:rendered;
                    span class="mode-switch__track" aria-hidden="true" {}
                    span data-text="$rendered ? 'MD' : 'Raw'" { "Raw" }
                }
            }
            textarea
                id="raw-input"
                class="raw"
                spellcheck="false"
                placeholder="# Notas\n\nEscreva em Markdown aqui."
                data-bind:rawText
                data-show="!$rendered"
                style="display: none"
            {}
            article id="preview" class="preview markdownRender" data-show="$rendered" style="display: none" {}
        }
    }
}
