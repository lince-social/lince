use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main #trail-app.trailApp {
            header.hero {
                div.heroCopy {
                    div.eyebrow { "trail" }
                    h1.title { "Trail Relation" }
                    p.copy {
                        "Graph view for copied trail trees. Configure discovery, creation, sync, and node progression from the side panel."
                    }
                }
                div.heroMeta {
                    span #trail-bound-pill.pill { "No trail bound" }
                    span #trail-row-pill.pill { "0 nodes" }
                    span #trail-link-pill.pill { "0 links" }
                    span #trail-status-pill.pill.pillStatus { "Booting" }
                }
            }

            section.panel.graphPanel {
                div.panelHead {
                    div.panelHeadCopy {
                        div.eyebrow { "view" }
                        h2.panelTitle { "Trail Graph" }
                    }
                    div.panelToolbar {
                        div.panelChips {
                            span #trail-sync-pill.pill { "No sync" }
                            span #trail-zoom-pill.pill { "100%" }
                        }
                        div.toolbarButtons {
                            button #trail-zoom-out.button.buttonGhost type="button" { "Zoom out" }
                            button #trail-zoom-in.button.buttonGhost type="button" { "Zoom in" }
                            button #trail-fit.button.buttonGhost type="button" { "Fit" }
                            button #trail-center.button.buttonGhost type="button" { "Center" }
                        }
                    }
                }

                div.graphWorkspace {
                    div #trail-graph-stage.graphStage {
                        svg #trail-graph.graphSvg aria-label="Trail graph" {}
                        div.graphHint {
                            span.pill.graphHintPill { "Wheel to zoom" }
                            span.pill.graphHintPill { "Drag nodes to settle physics" }
                        }
                        div #trail-empty.emptyState hidden {
                            div.emptyBox {
                                div.eyebrow { "waiting" }
                                h3.emptyTitle { "No trail graph yet" }
                                p.copy { "Open a copied trail root from Discover, or create one from an original record." }
                            }
                        }
                    }

                    aside.sidePanel {
                        section.section {
                            div.sectionHead {
                                div {
                                    div.sectionLabel { "binding" }
                                    h3.sectionTitle { "Current Trail" }
                                }
                            }
                            div #trail-binding-summary.selectionBox {
                                div #trail-binding-title.selectionTitle { "No copied trail root bound" }
                                p #trail-binding-copy.copy { "Use Open trail in Discover, or select a graph node and bind it as the trail root." }
                            }
                            p #trail-overwrite-copy.copy { "" }
                            div.actionRow {
                                button #trail-initialize.button.buttonPrimary type="button" disabled { "Reset to start" }
                                button #trail-run-sync.button.buttonGhost type="button" disabled { "Run sync" }
                            }
                            div.actionRow {
                                button #trail-bind-selected.button.buttonGhost type="button" disabled { "Open selected node as trail root" }
                            }
                        }

                        section.section {
                            div.sectionHead {
                                div {
                                    div.sectionLabel { "discover" }
                                    h3.sectionTitle { "Records" }
                                }
                            }
                            p.copy { "Typing in these fields refreshes backend search. Card click selects the original record for creation." }
                            div.sectionGrid {
                                label.field {
                                    span { "Head contains" }
                                    input #trail-search-head.input type="text" autocomplete="off" spellcheck="false" placeholder="record head";
                                }
                                label.field {
                                    span { "Category" }
                                    input #trail-search-category.input type="text" autocomplete="off" spellcheck="false" placeholder="documentation, copy";
                                }
                                label.field.autocompleteHost {
                                    span { "Assignee" }
                                    input #trail-search-assignee.input type="text" autocomplete="off" spellcheck="false" placeholder="name, username, or id";
                                    div #trail-search-assignee-suggestions.suggestionPanel hidden {}
                                }
                            }
                            p #trail-discover-summary.copy { "Loading discover results..." }
                            div.resultShell {
                                div #trail-discover-results.resultList {}
                            }
                        }

                        section.section {
                            div.sectionHead {
                                div {
                                    div.sectionLabel { "create" }
                                    h3.sectionTitle { "New Trail" }
                                }
                            }
                            label.field {
                                span { "Original record" }
                                div.selectionBox {
                                    div #trail-selected-original.selectionTitle { "No original selected" }
                                    p #trail-selected-original-copy.copy { "Choose an original record from Discover." }
                                }
                            }
                            label.field.autocompleteHost {
                                span { "Assignee" }
                                input #trail-create-assignee.input type="text" autocomplete="off" spellcheck="false" placeholder="name, username, or id";
                                div #trail-create-assignee-suggestions.suggestionPanel hidden {}
                            }
                            label.field {
                                span { "Scope" }
                                select #trail-sync-scope.input {
                                    option value="t" selected { "Tree" }
                                    option value="n" { "Node" }
                                    option value="nt" { "Both" }
                                }
                                p #trail-sync-scope-copy.copy { "Tree syncs children only." }
                            }
                            fieldset.choiceFieldset {
                                legend { "Properties to sync" }
                                div.choiceGroup {
                                    label.checkboxField {
                                        input #trail-sync-field-q type="checkbox";
                                        span { "Quantity" }
                                    }
                                    label.checkboxField {
                                        input #trail-sync-field-h type="checkbox" checked;
                                        span { "Head" }
                                    }
                                    label.checkboxField {
                                        input #trail-sync-field-b type="checkbox" checked;
                                        span { "Body" }
                                    }
                                }
                            }
                            button #trail-create-submit.button.buttonPrimary type="button" disabled { "Create trail" }
                        }

                        section.section {
                            div.sectionHead {
                                div {
                                    div.sectionLabel { "physics" }
                                    h3.sectionTitle { "Graph Physics" }
                                }
                            }
                            label.field {
                                span { "Charge" }
                                input #trail-physics-charge.input type="range" min="-420" max="-40" step="10" value="-200";
                            }
                            label.field {
                                span { "Link distance" }
                                input #trail-physics-distance.input type="range" min="40" max="220" step="5" value="110";
                            }
                            label.field {
                                span { "Collision" }
                                input #trail-physics-collision.input type="range" min="16" max="48" step="2" value="24";
                            }
                        }

                        section.section {
                            div.sectionHead {
                                div {
                                    div.sectionLabel { "selection" }
                                    h3.sectionTitle { "Selected Node" }
                                }
                            }
                            div #trail-selection-summary.selectionBox {
                                div #trail-selection-title.selectionTitle { "No node selected" }
                                p #trail-selection-copy.copy { "Click a node in the graph to inspect it and update its trail quantity." }
                            }
                            div.actionRow {
                                button #trail-set-locked.button.buttonGhost type="button" disabled { "Lock" }
                                button #trail-set-ready.button.buttonGhost type="button" disabled { "Ready" }
                                button #trail-set-done.button.buttonPrimary type="button" disabled { "Done" }
                            }
                        }
                    }
                }
            }
        }
    }
}
