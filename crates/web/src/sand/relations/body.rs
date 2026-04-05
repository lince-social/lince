use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="app" data-lince-bridge-root {
            header class="hero panel" {
                div class="heroCopy" {
                    div class="eyebrow" { "origin" }
                    h1 id="title" class="title" { "Relations" }
                    p id="copy" class="copy" {
                        "A directed graph for parent and child records. Category and parent-head filters rewrite the generated view, while physics stays local."
                    }
                }
                div class="heroMeta" {
                    span id="mode-pill" class="pill pill--mode" { "view" }
                    span id="origin-pill" class="pill" { "Waiting for origin" }
                    span id="status-pill" class="pill pill--status" { "Booting" }
                }
            }

            section class="layout" {
                section class="panel graphPanel" {
                    div class="panelHead" {
                        div class="panelHeadCopy" {
                            div class="eyebrow" { "view" }
                            h2 class="panelTitle" { "Graph" }
                        }
                        div class="panelChips" {
                            span id="row-pill" class="pill" { "0 nodes" }
                            span id="link-pill" class="pill" { "0 links" }
                            span id="filter-pill" class="pill" { "0 filters" }
                        }
                    }
                    div class="graphStage" {
                        canvas id="graph" aria-label="Relations graph" {}
                        div id="empty-state" class="emptyState" hidden="" {
                            div class="emptyState__box" {
                                div class="emptyState__eyebrow" { "Waiting" }
                                h3 class="emptyState__title" { "No nodes yet" }
                                p class="emptyState__copy" {
                                    "Open the widget stream or clear the current filters."
                                }
                            }
                        }
                    }
                }

                aside class="sidebar" {
                    section id="editor" class="panel editorPanel" hidden="" {
                        div class="sectionHead" {
                            div {
                                div class="eyebrow" { "origin" }
                                h2 class="panelTitle" { "Filters & Configuration" }
                            }
                            a class="linkChip" href="LICENSE.txt" target="_blank" rel="noreferrer" { "License" }
                        }

                        div class="section" {
                            div class="sectionLabel" { "origin" }
                            p id="origin-copy" class="mutedCopy" {
                                "The widget uses the host origin. Change it on the board card, not here."
                            }
                            pre id="origin-text" class="codeBlock" { "Waiting for host metadata..." }
                        }

                        div class="section" {
                            div class="sectionHead" {
                                div {
                                    div class="sectionLabel" { "filters" }
                                    h3 class="sectionTitle" { "Persistent view filters" }
                                }
                                button id="clear-filters" class="button button--ghost" type="button" { "Clear" }
                            }
                            label class="fieldLabel" for="parent-head-query" { "Parent head" }
                            input
                                id="parent-head-query"
                                class="input"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="Match parent head text"
                            ;
                            div class="sectionLabel" { "categories" }
                            p class="mutedCopy" {
                                "These categories are written into the generated SQL view."
                            }
                            div class="actionRow actionRow--split" {
                                input
                                    id="category-input"
                                    class="input"
                                    type="text"
                                    autocomplete="off"
                                    spellcheck="false"
                                    placeholder="Add category"
                                ;
                                button id="category-add" class="button button--ghost" type="button" { "Add category" }
                            }
                            div id="selected-category-list" class="chipList" {}
                            div class="sectionLabel" { "available categories" }
                            div id="category-filter-list" class="checkList" {}
                            div class="actionRow" {
                                button id="apply-filters" class="button button--primary" type="button" { "Apply view" }
                            }
                        }

                        div class="section" {
                            div class="sectionHead" {
                                div {
                                    div class="sectionLabel" { "configuration" }
                                    h3 class="sectionTitle" { "Local physics" }
                                }
                                button id="reset-physics" class="button button--ghost" type="button" { "Reset" }
                            }
                            label class="sliderRow" for="physics-charge" {
                                span class="sliderLabel" { "Charge" }
                                span id="physics-charge-value" class="sliderValue" { "-220" }
                            }
                            input id="physics-charge" class="slider" type="range" min="-600" max="-20" step="10" value="-220";

                            label class="sliderRow" for="physics-distance" {
                                span class="sliderLabel" { "Link distance" }
                                span id="physics-distance-value" class="sliderValue" { "110" }
                            }
                            input id="physics-distance" class="slider" type="range" min="30" max="240" step="5" value="110";

                            label class="sliderRow" for="physics-collision" {
                                span class="sliderLabel" { "Collision radius" }
                                span id="physics-collision-value" class="sliderValue" { "24" }
                            }
                            input id="physics-collision" class="slider" type="range" min="10" max="60" step="1" value="24";

                            label class="sliderRow" for="physics-center" {
                                span class="sliderLabel" { "Center force" }
                                span id="physics-center-value" class="sliderValue" { "0.18" }
                            }
                            input id="physics-center" class="slider" type="range" min="0" max="1" step="0.01" value="0.18";
                        }
                    }

                    section id="details" class="panel detailPanel" {
                        div class="sectionHead" {
                            div {
                                div class="eyebrow" { "selection" }
                                h2 class="panelTitle" { "Parent Link" }
                            }
                            span id="selection-pill" class="pill" { "None" }
                        }

                        div id="selection-empty" class="selectionEmpty" {
                            p class="mutedCopy" {
                                "Select a node. Set parent replaces only its parent link. Remove parent clears only that parent link and keeps its children untouched."
                            }
                        }

                        div id="selection-content" hidden="" {
                            pre id="selection-summary" class="codeBlock" {}

                            label class="fieldLabel" for="parent-select" { "Parent" }
                            select id="parent-select" class="select" {}

                            div class="actionRow actionRow--split" {
                                button id="connect-parent" class="button button--primary" type="button" { "Set parent" }
                                button id="disconnect-parent" class="button button--ghost" type="button" { "Remove parent" }
                            }

                            div class="section" {
                                div class="sectionLabel" { "Children" }
                                div id="child-list" class="chipList" {}
                            }
                        }
                    }
                }
            }
        }
    }
}
