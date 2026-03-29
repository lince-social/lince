use maud::{html, Markup};

pub(super) fn body() -> Markup {
    html! {
        .widget
        id="app"
        data-lince-bridge-root
        data-signals="{ queryOpen: false, activeSheet: '', focusSheetOpen: false, focusMarkdown: false, focusActionOpen: false, focusActionKind: '', focusActionRecordId: 0, focusActionCommentId: 0, focusActionResourceRefId: 0, focusActionHeader: '', focusActionDescription: '', focusActionBody: '', focusActionResourcePath: '', focusActionResourceKind: 'image', focusActionTitle: '', focusActionNote: '', focusActionMessage: '' }"
        data-effect="window.KanbanWidget?.syncActiveSheet($activeSheet)"
        data-on:kanban-open-filter="$activeSheet = 'filter'; $focusActionOpen = false"
        data-on:kanban-open-create="$activeSheet = 'create'; $focusActionOpen = false"
        data-on:kanban-open-edit="$focusSheetOpen = false; $activeSheet = 'edit'; $focusActionOpen = false"
        data-on:kanban-close-sheets="$activeSheet = ''; $focusActionOpen = false"
        data-on:kanban-open-focus="$activeSheet = ''; $focusMarkdown = true; $focusSheetOpen = true; $focusActionOpen = false; $focusActionKind = ''; $focusActionMessage = ''"
        data-on:kanban-close-focus="$focusMarkdown = false; $focusSheetOpen = false; $focusActionOpen = false; $focusActionKind = ''; $focusActionMessage = ''"
        data-on:kanban-open-focus-action="$focusActionOpen = true; $focusActionKind = evt.detail.kind || ''; $focusActionRecordId = evt.detail.recordId || 0; $focusActionCommentId = evt.detail.commentId || 0; $focusActionResourceRefId = evt.detail.resourceRefId || 0; $focusActionHeader = evt.detail.header || ''; $focusActionDescription = evt.detail.description || ''; $focusActionBody = evt.detail.body || ''; $focusActionResourcePath = evt.detail.resourcePath || ''; $focusActionResourceKind = evt.detail.resourceKind || 'image'; $focusActionTitle = evt.detail.title || ''; $focusActionNote = evt.detail.note || ''; $focusActionMessage = '';"
        data-on:kanban-close-focus-action="$focusActionOpen = false; $focusActionKind = ''; $focusActionRecordId = 0; $focusActionCommentId = 0; $focusActionResourceRefId = 0; $focusActionHeader = ''; $focusActionDescription = ''; $focusActionBody = ''; $focusActionResourcePath = ''; $focusActionResourceKind = 'image'; $focusActionTitle = ''; $focusActionNote = ''; $focusActionMessage = '';"
        data-on:kanban-focus-action-message="$focusActionMessage = evt.detail || ''"
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
                #kanban-focus-action-panel.panel data-show="$focusActionOpen" style="display: none" {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle data-text="$focusActionHeader" { "Action" }
                            .headerSub data-text="$focusActionDescription" { "" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-on:click="window.KanbanWidget?.closeFocusAction()" { "Close" }
                        }
                    }
                    p.small data-show="$focusActionMessage" data-text="$focusActionMessage" style="display: none" { "" }
                    form data-focus-action-form="" data-show="$focusActionKind === 'create-comment' || $focusActionKind === 'edit-comment'" style="display: none" {
                        .fieldBlock {
                            label.fieldLabel for="focus-action-body" { "Comment body" }
                            textarea.textarea id="focus-action-body" data-focus-action-field="body" data-bind:focusActionBody spellcheck="true" placeholder="Write a comment" {}
                        }
                        .sheetActions {
                            button.toolbarBtn type="button" data-on:click="window.KanbanWidget?.closeFocusAction()" { "Cancel" }
                            button.toolbarBtn.toolbarBtn--accent type="submit" data-show="$focusActionKind === 'create-comment'" style="display: none" { "Add comment" }
                            button.toolbarBtn.toolbarBtn--accent type="submit" data-show="$focusActionKind === 'edit-comment'" style="display: none" { "Save comment" }
                        }
                    }
                    form data-focus-action-form="" data-show="$focusActionKind === 'create-resource-ref'" style="display: none" {
                        .formGrid {
                            .fieldBlock {
                                label.fieldLabel for="focus-action-resource-path" { "Resource path" }
                                input.field id="focus-action-resource-path" data-focus-action-field="resourcePath" data-bind:focusActionResourcePath type="text" placeholder="bucket/path/to/file.png";
                            }
                            .fieldBlock {
                                label.fieldLabel for="focus-action-resource-kind" { "Resource kind" }
                                input.field id="focus-action-resource-kind" data-focus-action-field="resourceKind" data-bind:focusActionResourceKind type="text" placeholder="image";
                            }
                            .fieldBlock {
                                label.fieldLabel for="focus-action-resource-title" { "Title" }
                                input.field id="focus-action-resource-title" data-focus-action-field="title" data-bind:focusActionTitle type="text" placeholder="Optional title";
                            }
                        }
                        .sheetActions {
                            button.toolbarBtn type="button" data-on:click="window.KanbanWidget?.closeFocusAction()" { "Cancel" }
                            button.toolbarBtn.toolbarBtn--accent type="submit" { "Link resource" }
                        }
                    }
                    form data-focus-action-form="" data-show="$focusActionKind === 'start-worklog'" style="display: none" {
                        .fieldBlock {
                            label.fieldLabel for="focus-action-note" { "Worklog note" }
                            textarea.textarea id="focus-action-note" data-focus-action-field="note" data-bind:focusActionNote spellcheck="true" placeholder="Optional note" {}
                        }
                        .sheetActions {
                            button.toolbarBtn type="button" data-on:click="window.KanbanWidget?.closeFocusAction()" { "Cancel" }
                            button.toolbarBtn.toolbarBtn--accent type="submit" { "Start" }
                        }
                    }
                    form data-focus-action-form="" data-show="$focusActionKind === 'delete-comment' || $focusActionKind === 'delete-resource-ref' || $focusActionKind === 'delete-record'" style="display: none" {
                        p data-text="$focusActionBody" { "" }
                        .sheetActions {
                            button.toolbarBtn type="button" data-on:click="window.KanbanWidget?.closeFocusAction()" { "Cancel" }
                            button.toolbarBtn.toolbarBtn--danger type="submit" data-show="$focusActionKind === 'delete-record'" style="display: none" { "Delete record" }
                            button.toolbarBtn.toolbarBtn--danger type="submit" data-show="$focusActionKind !== 'delete-record'" style="display: none" { "Remove" }
                        }
                    }
                }
                #kanban-focus-card {}
            }
        }
    }
}
