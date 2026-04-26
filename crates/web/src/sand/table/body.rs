use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="tableWidget" data-signals="{ ui: { detailsOpen: false } }" {
            (render_top_line())

            div id="content-shell" class="contentShell" {
                aside
                    id="table-details"
                    class="detailsPanel"
                {
                    (render_details_placeholder())
                }

                aside id="create-panel" class="createPanel" hidden="" {
                    div class="createForm" {
                        select
                            id="create-table-select"
                            class="field field--select"
                            aria-label="Table"
                        {}
                        div id="create-fields" class="createFields" {}
                        button id="create-submit" class="button button--accent" type="button" {
                            "Create"
                        }
                    }
                }

                section id="table-body" class="tablePanel" tabindex="0" aria-label="Table data" {
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
            div class="topLineTitle" { "Table" }
            div class="topLineActions" {
                span id="table-status" class="status" data-tone="idle" { "Waiting" }
                button id="create-open" class="button button--accent" type="button" { "Create" }
            }
        }
    }
}

fn render_details_placeholder() -> Markup {
    html! {
        div class="detailStack" {
            section class="detailCard detailCard--setting" {
                div class="detailCopy" { "Mode" }
                select
                    id="table-mode"
                    class="field field--select"
                    aria-label="Mode"
                {
                    option value="common" { "Common" }
                    option value="helix" { "Helix" }
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "table" }
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
