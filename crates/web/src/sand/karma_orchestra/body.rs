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
                            span { "Distinctness" }
                            div.segmented {
                                button data-distinctness="none" class="segment is-active" type="button" { "None" }
                                button data-distinctness="condition" class="segment" type="button" { "Condition" }
                                button data-distinctness="consequence" class="segment" type="button" { "Consequence" }
                                button data-distinctness="both" class="segment" type="button" { "Both" }
                            }
                        }
                        div.field {
                            span { "Physics" }
                            div.physicsHead {
                                span class="muted" { "Live D3 forces" }
                                button #karma-physics-reset.button type="button" { "Reset" }
                            }
                            label.sliderRow for="karma-physics-center-expulsion" {
                                span.sliderLabel { "Center expulsion" }
                                span #karma-physics-center-expulsion-value.sliderValue { "2.3" }
                            }
                            input #karma-physics-center-expulsion.slider type="range" min="0" max="6" step="0.1" value="2.3";

                            label.sliderRow for="karma-physics-condition-pulling" {
                                span.sliderLabel { "Condition pulling" }
                                span #karma-physics-condition-pulling-value.sliderValue { "0.25" }
                            }
                            input #karma-physics-condition-pulling.slider type="range" min="0" max="1" step="0.01" value="0.25";

                            label.sliderRow for="karma-physics-node-repulsion" {
                                span.sliderLabel { "Repulsion node-by-node" }
                                span #karma-physics-node-repulsion-value.sliderValue { "-640" }
                            }
                            input #karma-physics-node-repulsion.slider type="range" min="-1200" max="-40" step="10" value="-640";
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
