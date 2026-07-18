use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="transferApp" {
            header class="toolbar" {
                div class="titleBlock" {
                    div class="eyebrow" { "Commitments" }
                    h1 { "Transfers" }
                }
                div class="toolbarTools" {
                    span id="create-blocker" class="creationBlocker" role="status" hidden {}
                    button id="create-transfer" class="primaryButton" type="button" { "+ New transfer" }
                    label class="search" {
                        span class="visuallyHidden" { "Search transfers" }
                        span aria-hidden="true" { "⌕" }
                        input id="search" type="search" placeholder="Search" autocomplete="off";
                    }
                    select id="sort" aria-label="Sort transfers" {
                        option value="attention" { "Needs attention" }
                        option value="name" { "Name" }
                        option value="status" { "Status" }
                    }
                    span id="live-dot" class="liveDot" data-live="false" role="status" aria-label="Reconnecting" title="Connection status" {}
                }
            }

            nav id="filters" class="filters" aria-label="Transfer status" {
                button type="button" data-filter="all" aria-pressed="true" { "All" }
                button type="button" data-filter="attention" aria-pressed="false" { "Attention" }
                button type="button" data-filter="open" aria-pressed="false" { "Open" }
                button type="button" data-filter="settled" aria-pressed="false" { "Settled" }
            }

            section id="summary" class="summary" aria-label="Transfer summary" {}

            div id="loading" class="stateMessage" { "Loading transfers" }
            div id="empty" class="stateMessage" hidden {
                p id="empty-message" { "No transfers match this view." }
                button id="empty-create-transfer" class="primaryButton" type="button" { "Create transfer" }
            }

            section id="workspace" class="workspace" hidden {
                section class="overview" aria-label="Transfer overview" {
                    div id="transfer-list" class="transferList" {}
                }
                article id="detail" class="detail" aria-label="Selected transfer" tabindex="-1" hidden {}
            }

            section id="creator" class="creator" aria-label="Create transfer" hidden {}
        }
    }
}
