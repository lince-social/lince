use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main #trail-app.trailApp {
            section.panel.graphPanel {
                div.graphWorkspace {
                    div #trail-graph-stage.graphStage {
                        div.graphOverlay.graphOverlay--title {
                            h1.title { "Trail" }
                        }

                        div.graphHud.graphHud--topRight {
                            div.heroMeta {
                                span #trail-bound-pill.pill { "No trail bound" }
                                span #trail-row-pill.pill { "0 nodes" }
                                span #trail-link-pill.pill { "0 links" }
                                span #trail-view-pill.pill { "No view" }
                                span #trail-status-pill.pill.pillStatus { "Booting" }
                            }
                            div.panelToolbar {
                                button #trail-fit.button.buttonGhost type="button" { "Fit" }
                                button #trail-zoom-pill.pill.button type="button" { "100%" }
                                button #trail-zoom-in.button.buttonGhost type="button" { "+" }
                                button #trail-zoom-out.button.buttonGhost type="button" { "-" }
                            }
                        }

                        div.overlayDeck {
                            div.quantityDock {
                                button #trail-quantity-pass.button.buttonGhost.quantityButton.quantityButton--pass type="button" disabled { "Passed" }
                                button #trail-quantity-far.button.buttonGhost.quantityButton.quantityButton--far type="button" disabled { "Far" }
                                button #trail-quantity-step.button.buttonGhost.quantityButton.quantityButton--step type="button" disabled { "Step" }
                            }

                            div.section.section--dock {
                                div.sectionDockPanel {
                                    div.sectionHead {
                                        div {
                                            div.sectionLabel { "create" }
                                            h3.sectionTitle { "New Trail" }
                                        }
                                    }
                                    label.field {
                                        span { "Original record" }
                                        div.field.autocompleteHost {
                                            input #trail-original-record.input type="text" autocomplete="off" spellcheck="false" placeholder="name or id";
                                            div #trail-original-record-suggestions.suggestionPanel hidden { "" }
                                        }
                                        div.selectionBox {
                                            div #trail-selected-original.selectionTitle { "No original selected" }
                                            p #trail-selected-original-copy.copy { "Select a graph node or type a record above to use it as the original record." }
                                        }
                                    }
                                    label.field.autocompleteHost {
                                        span { "Assignee" }
                                        input #trail-create-assignee.input type="text" autocomplete="off" spellcheck="false" placeholder="name, username, or id";
                                        div #trail-create-assignee-suggestions.suggestionPanel hidden { "" }
                                    }
                                    label.field {
                                        span { "View name" }
                                        input #trail-view-name.input type="text" autocomplete="off" spellcheck="false" placeholder="trail-root-view";
                                    }
                                    label.field {
                                        span { "Selected node quantity" }
                                        div.selectionBox {
                                            div #trail-selected-node.selectionTitle { "No node selected" }
                                            p #trail-selected-node-copy.copy { "Click a graph node, then set its quantity without changing the graph until the stream refreshes." }
                                        }
                                    }
                                    button #trail-create-submit.button.buttonPrimary type="button" disabled { "Create trail" }
                                }
                                button.sectionDockButton type="button" aria-label="Create trail" { "▣" }
                            }

                            div.section.section--dock {
                                div.sectionDockPanel {
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
                                button.sectionDockButton type="button" aria-label="Graph physics" { "◫" }
                            }
                        }

                        svg #trail-graph.graphSvg aria-label="Trail graph" {
                            desc { "Trail graph" }
                        }
                        div #trail-empty.emptyState hidden {
                            div.emptyBox {
                                div.eyebrow { "waiting" }
                                h3.emptyTitle { "No trail graph yet" }
                                p.copy { "Open a copied trail root from Discover, or create one from an original record." }
                            }
                        }
                    }
                }
            }
        }
    }
}
