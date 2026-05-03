use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main #karma-orchestra-app.karmaOrchestra {
            div.stageShell {
                div #karma-stage.stage {
                    div.topHud {
                        h1.title { "Karma Orchestra" }
                        div.pills {
                            span #karma-status.pill { "Booting" }
                            span #karma-view-pill.pill { "No view" }
                            span #karma-count-pill.pill { "0 rules" }
                            span #karma-loop-pill.pill { "0 loops" }
                        }
                    }

                    button #karma-state-ball.stateBall type="button" aria-label="Open adjustments" {}
                    button #karma-view-button.viewButton type="button" { "Views" }

                    svg #karma-graph.graph aria-label="Karma Orchestra graph" {
                        defs {
                            marker #arrow viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse" {
                                path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" {}
                            }
                        }
                    }

                    div #karma-empty.emptyState {
                        div.emptyCard {
                            div.eyebrow { "waiting" }
                            h2 { "Pick or create a Karma Orchestra View" }
                            p { "Normal SQL Views are rejected. Use the Views button inside the canvas." }
                        }
                    }

                    aside #karma-view-modal.viewModal hidden {
                        div.modalHead {
                            div {
                                div.eyebrow { "view" }
                                h2 { "Karma Orchestra Views" }
                            }
                            button #karma-view-close.button type="button" { "Close" }
                        }
                        div #karma-view-list.viewList {}
                        label.field {
                            span { "New View name" }
                            input #karma-view-name.input type="text" value="Karma Orchestra" autocomplete="off";
                        }
                        button #karma-create-view.button.primary type="button" { "Create and use" }
                    }

                    aside #karma-adjustments.adjustments hidden {
                        div.modalHead {
                            div {
                                div.eyebrow { "adjust" }
                                h2 { "Layout" }
                            }
                            button #karma-adjust-close.button type="button" { "Close" }
                        }
                        div.field {
                            span { "Formation" }
                            div.segmented {
                                button data-layout-mode="list" class="segment is-active" type="button" { "List" }
                                button data-layout-mode="circle" class="segment" type="button" { "Circle" }
                            }
                        }
                        div.field {
                            span { "Distinctness" }
                            div.toggleGrid {
                                label.toggleRow {
                                    span { "Unique conditions" }
                                    input #karma-distinct-condition type="checkbox";
                                }
                                label.toggleRow {
                                    span { "Unique consequences" }
                                    input #karma-distinct-consequence type="checkbox";
                                }
                            }
                        }
                        div.field {
                            span { "Physics" }
                            div.physicsHead {
                                span class="muted" { "Live D3 forces" }
                                button #karma-physics-reset.button type="button" { "Reset" }
                            }
                            div #karma-physics-center-expulsion-field {
                                label.sliderRow for="karma-physics-center-expulsion" {
                                    span.sliderLabel { "Center expulsion" }
                                    span #karma-physics-center-expulsion-value.sliderValue { "2.3" }
                                }
                                input #karma-physics-center-expulsion.slider type="range" min="0" max="6" step="0.1" value="2.3";
                            }

                            label.sliderRow for="karma-physics-link-distance-input" {
                                span.sliderLabel { "Link distance" }
                                input #karma-physics-link-distance-input.input.repulsionValue type="number" min="40" max="320" step="1" value="180";
                            }
                            input #karma-physics-link-distance.slider type="range" min="40" max="320" step="1" value="180";

                            label.sliderRow for="karma-physics-node-repulsion-input" {
                                span.sliderLabel { "Repulsion node-by-node" }
                                input #karma-physics-node-repulsion-input.input.repulsionValue type="number" min="0" step="10" value="520";
                            }
                            input #karma-physics-node-repulsion.slider type="range" min="0" max="1200" step="10" value="520";
                        }
                        div.field {
                            span { "Colors" }
                            div.colorGrid {
                                label.colorRow {
                                    span { "Conditions" }
                                    input #karma-condition-color.input type="color" value="#f1ece2";
                                }
                                label.colorRow {
                                    span { "Consequences" }
                                    input #karma-consequence-color.input type="color" value="#6f2e2b";
                                }
                                label.colorRow {
                                    span { "Inactive" }
                                    input #karma-inactive-color.input type="color" value="#9b9b9b";
                                }
                            }
                        }
                        div.summaryGrid {
                            div { span { "Rules" } strong #karma-summary-rules { "0" } }
                            div { span { "Conditions" } strong #karma-summary-conditions { "0" } }
                            div { span { "Consequences" } strong #karma-summary-consequences { "0" } }
                            div { span { "Loops" } strong #karma-summary-loops { "0" } }
                        }
                    }
                }
            }
        }
    }
}
