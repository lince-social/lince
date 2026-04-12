use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="tableWidget" data-signals="{ ui: { detailsOpen: false } }" {
            (render_top_line())

            div class="contentShell" {
                aside
                    id="todo-details"
                    class="detailsPanel"
                    data-show="$ui.detailsOpen"
                    style="display: none"
                {
                    (render_blob_settings())
                    div id="table-details" {
                        (render_details_placeholder())
                    }
                }

                div id="todo-blob-layer" class="blobLayer" aria-hidden="true" {}

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
            div class="topLineActions" {
                button
                    id="table-status"
                    class="statusDot"
                    type="button"
                    data-tone="idle"
                    aria-label="Waiting"
                    title="Waiting"
                    data-on:click="$ui.detailsOpen = !$ui.detailsOpen"
                {}
            }
        }
    }
}

fn render_blob_settings() -> Markup {
    html! {
        div class="detailStack detailStack--settings" {
            section class="detailCard detailCard--setting" {
                div class="eyebrow" { "blob" }
                div class="settingRow" {
                    label class="toggleRow" for="blob-enabled" {
                        input id="blob-enabled" type="checkbox" checked;
                        span { "Liquid cursor" }
                    }
                    a class="licenseLink" href="blob.wgsl" target="_blank" rel="noreferrer" {
                        "WGSL source"
                    }
                }

                label class="settingBlock" for="blob-viscosity" {
                    span class="settingLabel" { "Viscosity" }
                    input id="blob-viscosity" type="range" min="0" max="100" value="62";
                }

                label class="settingBlock" for="blob-energy" {
                    span class="settingLabel" { "Energy" }
                    input id="blob-energy" type="range" min="0" max="100" value="56";
                }

                div class="settingBlock" {
                    span class="settingLabel" { "Palette" }
                    div class="colorTools" {
                        input id="blob-color-input" class="colorInput" type="color" value="#51f3d2";
                        button id="blob-add-color" class="button button--ghost" type="button" { "Add" }
                    }
                    div id="blob-palette" class="palette" aria-label="Blob color palette" {}
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
                    "The backend streams HTML fragments into this panel. Only the head column is visible here. Use j / k to move, space to set the focused row quantity to zero, u to undo, and Shift-U to redo."
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
