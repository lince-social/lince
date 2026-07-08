use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="kanban-app" class="kanbanWidget" data-kanban-root="" {
            header class="kanbanTopLine" {
                div class="kanbanTitle" { "Kanban" }
                div class="kanbanTopActions" {
                    span
                        id="kanban-status"
                        class="kanbanStatus"
                        data-tone="idle"
                        role="status"
                        aria-label="Waiting"
                        title="Waiting"
                    {}
                    button id="kanban-info-open" class="kanbanButton kanbanButton--ghost" type="button" {
                        "Info"
                    }
                }
            }

            aside id="kanban-details" class="kanbanDetails" hidden="" aria-hidden="true" {
                div class="kanbanDetailHeader" {
                    div class="kanbanDetailTitle" { "Board info" }
                    button id="kanban-info-close" class="kanbanButton kanbanButton--ghost" type="button" {
                        "Close"
                    }
                }
                div class="kanbanDetailGrid" {}
            }

            section id="kanban-board" class="kanbanBoard" aria-label="Kanban board" {
                div class="kanbanEmpty" { "Opening…" }
            }

            div id="kanban-toasts" class="kanbanToasts" aria-live="polite" aria-atomic="true" {}
        }
    }
}
