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
                    label class="presetPicker" {
                        span class="visuallyHidden" { "Start from workflow preset" }
                        select id="workflow-preset" aria-label="Start from workflow preset" {
                            option value="" { "Use preset..." }
                            option value="donation" { "Donation" }
                            option value="sale" { "Sale" }
                            option value="assignment" { "Assignment" }
                            option value="group" { "Group coordination" }
                            option value="service" { "Service" }
                            option value="information" { "Information" }
                            option value="dependency" { "Dependency plan" }
                            option value="ride" { "Ride" }
                            option value="delivery" { "Delivery" }
                        }
                    }
                    label class="search" {
                        span class="visuallyHidden" { "Search transfers" }
                        span aria-hidden="true" { "⌕" }
                        input id="search" type="search" placeholder="Search" autocomplete="off";
                    }
                    select id="sort" aria-label="Sort transfers" {
                        option value="attention" { "Workflow priority" }
                        option value="name" { "Name" }
                        option value="status" { "Status" }
                    }
                    span id="live-dot" class="liveDot" data-live="false" role="status" aria-label="Reconnecting" title="Connection status" {}
                }
            }

            section class="inboxControls" aria-label="Transfer inbox filters" {
                div id="ownership-filters" class="ownershipFilters" role="group" aria-label="Ownership scope" {
                    button type="button" data-ownership="all" aria-pressed="true" {
                        span { "All" }
                        span class="filterCount" data-count-ownership="all" aria-hidden="true" { "0" }
                    }
                    button type="button" data-ownership="mine" aria-pressed="false" {
                        span { "Mine" }
                        span class="filterCount" data-count-ownership="mine" aria-hidden="true" { "0" }
                    }
                }
                nav id="filters" class="filters" aria-label="Workflow status" {
                    button type="button" data-workflow="all" aria-pressed="true" {
                        span { "Any status" }
                        span class="filterCount" data-count-workflow="all" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="awaiting_me" aria-pressed="false" {
                        span { "Awaiting me" }
                        span class="filterCount" data-count-workflow="awaiting_me" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="awaiting_others" aria-pressed="false" {
                        span { "Awaiting others" }
                        span class="filterCount" data-count-workflow="awaiting_others" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="active" aria-pressed="false" {
                        span { "Active" }
                        span class="filterCount" data-count-workflow="active" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="completed" aria-pressed="false" {
                        span { "Completed" }
                        span class="filterCount" data-count-workflow="completed" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="cancelled_or_broken" aria-pressed="false" {
                        span { "Cancelled / broken" }
                        span class="filterCount" data-count-workflow="cancelled_or_broken" aria-hidden="true" { "0" }
                    }
                    button type="button" data-workflow="discoverable_open" aria-pressed="false" {
                        span { "Discoverable OPEN" }
                        span class="filterCount" data-count-workflow="discoverable_open" aria-hidden="true" { "0" }
                    }
                }
                span id="view-modes" class="viewModes" role="group" aria-label="Overview mode" {
                    button type="button" data-view="list" aria-pressed="true" { "List" }
                    button type="button" data-view="tree" aria-pressed="false" { "Tree" }
                }
            }

            section id="summary" class="summary" aria-label="Transfer summary" {}

            section id="inbox-notice" class="inboxNotice" role="status" aria-live="polite" hidden {
                span id="inbox-notice-message" {}
                button id="retry-transfers" class="secondaryButton" type="button" hidden { "Retry" }
            }

            div id="loading" class="stateMessage" role="status" aria-live="polite" { "Loading transfers" }
            div id="empty" class="stateMessage" hidden {
                p id="empty-message" { "No transfers match this view." }
                button id="empty-create-transfer" class="primaryButton" type="button" { "Create transfer" }
                button id="clear-transfer-filters" class="secondaryButton" type="button" hidden { "Clear filters" }
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
