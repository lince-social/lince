use {
    maud::{Markup, html},
    serde_json::json,
};

fn default_signals() -> String {
    json!({
        "queryOpen": false,
        "activeSheet": "",
        "focusSheetOpen": false,
        "focusMarkdown": false,
        "focusLayout": "full",
        "shell": {
            "title": "Kanban Record View",
            "queryText": "",
            "queryLabel": "Waiting for widget contract...",
            "queryDisabled": true,
            "statusLabel": "Waiting",
            "statusCopy": "Waiting for the instance-aware contract and stream.",
            "statusClass": "status",
            "warningVisible": false,
            "warningTitle": "",
            "warningCopy": "",
            "warningDetail": "",
            "toggleUpdatesLabel": "Pause updates",
            "toggleStreamLabel": "Disconnect widget",
            "toggleStreamAccent": false,
            "toggleStreamPaused": true,
            "reconnectDisabled": true,
        },
        "ui": {
            "lanes": {
                "backlog": { "collapsed": false, "width": 260 },
                "next": { "collapsed": false, "width": 260 },
                "wip": { "collapsed": false, "width": 260 },
                "review": { "collapsed": false, "width": 260 },
                "done": { "collapsed": false, "width": 260 },
            },
            "uiVersion": 2,
            "defaultBodyMode": "full",
            "cardModes": {},
            "focusedRecordId": null,
        },
    })
    .to_string()
}

pub(super) fn body() -> Markup {
    html! {
            .widget
            id="app"
            data-lince-bridge-root
            data-signals=(default_signals())
            data-on:kanban-open-filter="$activeSheet = 'filter'"
            data-on:kanban-open-create="$activeSheet = 'create'"
            data-on:kanban-open-edit="$focusSheetOpen = false; $activeSheet = 'edit'"
            data-on:kanban-close-sheets="$activeSheet = ''"
            data-on:kanban-open-focus="$activeSheet = ''; $focusLayout = 'full'; $focusMarkdown = true; $focusSheetOpen = true"
            data-on:kanban-close-focus="$focusLayout = 'full'; $focusMarkdown = false; $focusSheetOpen = false"
            data-on-signal-patch="window.KanbanWidget?.syncSheetFromSignals?.(patch); window.KanbanWidget?.syncUiFromSignals(patch); window.KanbanWidget?.persistUiFromSignals(patch)"
            data-on-signal-patch-filter="{include: /^(ui(\\.|$)|activeSheet$)/}"
        {
            .widgetSurface {
                .panel {
                    .header {
                        #kanban-header-meta.headerMeta {
                            .headerTitle id="kanban-header-title" data-text="$shell.title" { "Kanban Record View" }
                            button.headerSubButton
                                type="button"
                                id="kanban-query-toggle"
                                data-on:click="$queryOpen = !$queryOpen"
                                data-text="$shell.queryLabel"
                                data-attr:disabled="$shell.queryDisabled"
                            { "Waiting for widget contract..." }
                            pre.headerQuery id="kanban-query-copy" data-show="$queryOpen && !!$shell.queryText" data-text="$shell.queryText" style="display: none" {}
                        }
                        .headerActions {
                            span.status id="kanban-connection-status" data-attr:class="$shell.statusClass" data-text="$shell.statusLabel" { "Waiting" }
                            #kanban-toolbar-state {}
                            .toolbarGroup {
                                button.toolbarBtn type="button" data-on:click="$ui.defaultBodyMode = 'head'; $ui.cardModes = {}" { "All head" }
                                button.toolbarBtn type="button" data-on:click="$ui.defaultBodyMode = 'compact'; $ui.cardModes = {}" { "All compact" }
                                button.toolbarBtn type="button" data-on:click="$ui.defaultBodyMode = 'full'; $ui.cardModes = {}" { "All full" }
                            }
                            button.toolbarBtn type="button" id="kanban-toggle-updates" data-text="$shell.toggleUpdatesLabel" { "Pause updates" }
                            button.toolbarBtn type="button" id="kanban-open-filters" data-on:click="$activeSheet = 'filter'" { "Filters" }
                            button.toolbarBtn.toolbarBtn--accent type="button" id="kanban-open-create" data-on:click="$activeSheet = 'create'" { "New task" }
                            button.toolbarBtn.toolbarBtn--paused
                                type="button"
                                id="kanban-toggle-stream"
                                data-class:toolbarBtn--accent="$shell.toggleStreamAccent"
                                data-class:toolbarBtn--paused="$shell.toggleStreamPaused"
                                data-text="$shell.toggleStreamLabel"
                            { "Disconnect widget" }
                            button.toolbarBtn.toolbarBtn--accent
                                type="button"
                                id="kanban-reconnect"
                                data-attr:disabled="$shell.reconnectDisabled"
                            { "Reconnect" }
                        }
                    }
                    p.small id="kanban-status-copy" data-text="$shell.statusCopy" {
                        "Waiting for the instance-aware contract and stream."
                    }
                }
                #kanban-active-filters {}
                #kanban-state-panel.panel data-show="$shell.warningVisible" style="display: none" {
                    .header {
                        .headerMeta {
                            h2.warnTitle id="kanban-state-title" data-text="$shell.warningTitle" { "" }
                            p.small id="kanban-state-copy" data-text="$shell.warningCopy" { "" }
                        }
                    }
                    p.small id="kanban-state-detail" data-text="$shell.warningDetail" { "" }
                }
                #kanban-empty-or-error {}
                .boardWrap {
                    #kanban-columns.board {}
                }
            }
            .sheetOverlay id="kanban-filter-sheet" data-show="$activeSheet === 'filter'" style="display: none" {
                button.sheetBackdrop type="button" data-close-sheet="filter" data-on:click="$activeSheet = ''" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Filters" }
                            .headerSub { "Each active row is AND. Multi-value rows use OR." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-clear-filters="true" { "Clear all" }
                            button.toolbarBtn type="button" data-close-sheet="filter" data-on:click="$activeSheet = ''" { "Close" }
                        }
                    }
                    #kanban-filter-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-create-sheet" data-show="$activeSheet === 'create'" style="display: none" {
                button.sheetBackdrop type="button" data-close-sheet="create" data-on:click="$activeSheet = ''" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Create task" }
                            .headerSub { "A record becomes Kanban-ready when task_type is set." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-sheet="create" data-on:click="$activeSheet = ''" { "Close" }
                        }
                    }
                    #kanban-create-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-edit-sheet" data-show="$activeSheet === 'edit'" style="display: none" {
                button.sheetBackdrop type="button" data-close-sheet="edit" data-on:click="$activeSheet = ''" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Edit task" }
                            .headerSub { "Task metadata and relations are saved through the Kanban actions." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-sheet="edit" data-on:click="$activeSheet = ''" { "Close" }
                        }
                    }
                    #kanban-edit-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-focus-sheet" data-show="$focusSheetOpen" style="display: none" {
                button.sheetBackdrop type="button" data-close-focus="true" data-on:click="$focusMarkdown = false; $focusSheetOpen = false" {}
                .sheetPanel
                    data-class:focus-layout-full="$focusLayout === 'full'"
                    data-class:focus-layout-side="$focusLayout === 'side'"
                {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Task detail" }
                            .headerSub { "Focused record detail loads from the host service." }
                        }
                        .headerActions {
                            button.toolbarBtn
                                type="button"
                                data-show="$focusLayout === 'full'"
                                data-on:click="$focusLayout = 'side'"
                            { "Side mode" }
                            button.toolbarBtn
                                type="button"
                                data-show="$focusLayout === 'side'"
                                data-on:click="$focusLayout = 'full'"
                            { "Full mode" }
                            button.toolbarBtn.toolbarBtn--accent
                                type="button"
                                data-on:click="$focusLayout = 'full'; $focusMarkdown = false; $focusSheetOpen = false; $activeSheet = ''"
                            { "Back to kanban" }
                        }
                    }
                    #kanban-focus-action-panel.panel hidden {}
                        #kanban-focus-card {}
                    }
                }
            }
        }
    }
