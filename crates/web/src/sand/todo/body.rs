use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="todoWidget" {
            (render_top_line())

            div class="contentShell" {
                aside
                    id="todo-details"
                    class="detailsPanel"
                    hidden
                {
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
                    "Open the stream to see the current item."
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
