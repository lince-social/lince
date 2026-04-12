use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="todoWidget" {
            (render_top_line())

            div class="contentShell" {
                div id="todo-blob-layer" class="blobLayer" aria-hidden="true" {}

                aside
                    id="todo-details"
                    class="detailsPanel"
                    hidden
                {
                    (render_blob_settings())
                    (render_details_placeholder())
                }

                section
                    id="todo-list-panel"
                    class="listPanel"
                    tabindex="0"
                    aria-label="Todo items"
                {
                    (render_list_placeholder())
                }
            }
        }
    }
}

fn render_top_line() -> Markup {
    html! {
        header class="topLine" {
            div class="topLineSpacer" {}
            div class="topLineActions" {
                button
                    id="todo-status"
                    class="statusDot"
                    type="button"
                    aria-label="Connecting"
                    title="Connecting"
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
                        input id="blob-enabled" type="checkbox";
                        span { "Liquid cursor" }
                    }
                    a class="licenseLink" href="blob.wgsl" target="_blank" rel="noreferrer" {
                        "WGSL source"
                    }
                }

                div class="settingCopy" {
                    "This uses WebGPU. In Chromium, enable WebGPU Developer Features and Unsafe WebGPU Support."
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
                div id="todo-detail-source" class="detailTitle" { "Waiting for a list stream" }
                div class="detailCopy" {
                    "This widget listens to the normal view SSE stream and renders the active item as a list."
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "active" }
                div id="todo-detail-active" class="detailTitle" { "No active item" }
                div id="todo-detail-preview" class="detailCopy" {
                    "Use j / k or the arrow keys to move, space to set the focused row quantity to zero, u to undo, and Shift-U to redo."
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "stream" }
                div id="todo-detail-endpoint" class="codeBlock" {
                    "Waiting for connection."
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "stats" }
                div class="detailGrid" {
                    span id="todo-detail-count" class="pill" { "items: 0" }
                    span id="todo-detail-source-count" class="pill" { "active: 0" }
                }
            }
        }
    }
}

fn render_list_placeholder() -> Markup {
    html! {
        div class="listFrame" {
            div class="emptyState" {
                div class="stateTitle" { "Waiting for items" }
                div class="stateCopy" {
                    "The normal view SSE stream will populate this list and keep one item active."
                }
            }
        }
    }
}
