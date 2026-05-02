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
