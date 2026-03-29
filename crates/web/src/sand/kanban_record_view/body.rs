use maud::{html, Markup};

pub(super) fn body() -> Markup {
    html! {
        .widget
            id="app"
            data-lince-bridge-root
            data-signals="{ queryOpen: false, activeSheet: '', focusSheetOpen: false, focusMarkdown: false }"
            data-on:kanban-open-filter="$activeSheet = 'filter'"
            data-on:kanban-open-create="$activeSheet = 'create'"
            data-on:kanban-open-edit="$focusSheetOpen = false; $activeSheet = 'edit'"
            data-on:kanban-close-sheets="$activeSheet = ''"
            data-on:kanban-open-focus="$activeSheet = ''; $focusMarkdown = false; $focusSheetOpen = true"
            data-on:kanban-close-focus="$focusMarkdown = false; $focusSheetOpen = false"
        {
            .widgetSurface {
                .panel {
                    .header {
                        #kanban-header-meta.headerMeta {
                            .headerTitle id="kanban-header-title" { "Kanban Record View" }
                            button.headerSubButton type="button" id="kanban-query-toggle" data-on:click="$queryOpen = !$queryOpen" { "Waiting for widget contract..." }
                            pre.headerQuery id="kanban-query-copy" data-show="$queryOpen" style="display: none" {}
                        }
                        .headerActions {
                            span.status id="kanban-connection-status" { "Waiting" }
                            #kanban-toolbar-state {}
                            .toolbarGroup {
                                button.toolbarBtn type="button" data-set-default-body-mode="head" { "All head" }
                                button.toolbarBtn type="button" data-set-default-body-mode="compact" { "All compact" }
                                button.toolbarBtn type="button" data-set-default-body-mode="full" { "All full" }
                            }
                            button.toolbarBtn type="button" id="kanban-toggle-updates" { "Pause updates" }
                            button.toolbarBtn type="button" id="kanban-open-filters" data-on:click="$activeSheet = 'filter'" { "Filters" }
                            button.toolbarBtn.toolbarBtn--accent type="button" id="kanban-open-create" data-on:click="$activeSheet = 'create'" { "New task" }
                            button.toolbarBtn.toolbarBtn--paused type="button" id="kanban-toggle-stream" { "Disconnect widget" }
                            button.toolbarBtn.toolbarBtn--accent type="button" id="kanban-reconnect" { "Reconnect" }
                        }
                    }
                    p.small id="kanban-status-copy" {
                        "Waiting for the instance-aware contract and stream."
                    }
                }
                #kanban-active-filters {}
                #kanban-state-panel.panel hidden {
                    .header {
                        .headerMeta {
                            h2.warnTitle id="kanban-state-title" { "" }
                            p.small id="kanban-state-copy" { "" }
                        }
                    }
                    p.small id="kanban-state-detail" { "" }
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
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Task detail" }
                            .headerSub { "Focused record detail loaded from the host service." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-focus="true" data-on:click="$focusMarkdown = false; $focusSheetOpen = false" { "Close" }
                        }
                    }
                    #kanban-focus-card {}
                }
            }
        }
    }
}
