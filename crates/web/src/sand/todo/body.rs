use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="tableWidget" data-signals="{ ui: { detailsOpen: false } }" {
            (render_top_line())

            div class="contentShell" {
                aside
                    id="table-details"
                    class="detailsPanel"
                    data-show="$ui.detailsOpen"
                    style="display: none"
                {
                    (render_details_placeholder())
                }

                section id="table-body" class="tablePanel" tabindex="0" aria-label="Todo data" {
                    (render_body_placeholder())
                }
            }

            div id="table-stream-bootstrap" hidden="" {}
        }
    }
}

fn render_top_line() -> Markup {
    html! {
        header class="topLine" {
            div class="topLineTitle" { "Todo" }
            div class="topLineActions" {
                span id="table-status" class="status" data-tone="idle" { "Waiting" }
                button
                    class="button button--accent"
                    type="button"
                    data-on:click="$ui.detailsOpen = !$ui.detailsOpen"
                    data-text="$ui.detailsOpen ? 'Hide details' : 'Details'"
                {
                    "Details"
                }
            }
        }
    }
}

fn render_details_placeholder() -> Markup {
    html! {
        div class="detailStack" {
            section class="detailCard" {
                div class="eyebrow" { "todo" }
                div class="detailTitle" { "Head-only list" }
                div class="detailCopy" {
                    "The backend streams HTML fragments into this panel. Only the head column is visible here. Use j / k to move and space to set the focused row quantity to zero."
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "todo" }
                div class="detailTitle" { "Waiting for a snapshot" }
                div class="detailCopy" {
                    "The backend will stream HTML fragments into this panel after the first view snapshot arrives."
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "metrics" }
                div class="detailGrid" {
                    span class="pill" { "server: loading" }
                    span class="pill" { "view: loading" }
                    span class="pill" { "rows: 0" }
                    span class="pill" { "columns: 0" }
                    span class="pill" { "kind: loading" }
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "sql" }
                pre class="codeBlock" { "Waiting for the first snapshot." }
            }
        }
    }
}

fn render_body_placeholder() -> Markup {
    html! {
        div class="tableFrame" {
            div class="emptyState" {
                div class="stateTitle" { "Opening stream" }
                div class="stateCopy" {
                    "The table markup is rendered on the backend and patched into this area as HTML fragments."
                }
            }
        }
    }
}
