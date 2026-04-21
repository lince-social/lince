use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="app relationWidget" data-lince-bridge-root {
            section class="panel graphPanel" {
                div class="graphWorkspace" {
                    div class="graphStage" {
                        div class="graphOverlay graphOverlay--title" {
                            h1 id="title" class="title graphTitle" title="Relation" { "Relation" }
                        }

                        div class="graphOverlay graphOverlay--status" {
                            button
                                id="panel-toggle"
                                class="statusBall"
                                type="button"
                                aria-label="Open controls"
                                title="Open controls"
                            {}
                        }

                        div class="graphHud graphHud--topRight" {
                            div class="panelToolbar" {
                                button id="zoom-out" class="button button--ghost" type="button" aria-label="Zoom out" { "-" }
                                button id="zoom-fit" class="button button--ghost" type="button" { "Fit" }
                                button id="zoom-in" class="button button--ghost" type="button" aria-label="Zoom in" { "+" }
                                button id="create-open" class="button button--primary" type="button" { "Create" }
                            }
                        }

                        canvas id="graph" aria-label="Relation graph" {}
                        div id="empty-state" class="emptyState" hidden="" {
                            div class="emptyState__box" {
                                div class="emptyState__eyebrow" { "waiting" }
                                h3 class="emptyState__title" { "No nodes yet" }
                                p class="emptyState__copy" {
                                    "Open the widget stream or clear the current filters."
                                }
                            }
                        }
                    }

                    aside id="controls-panel" class="sidePanel sidePanel--controls" hidden="" {
                        div class="sidePanelHead" {
                            div {
                                div class="eyebrow" { "controls" }
                                h2 class="panelTitle" { "Relation" }
                            }
                            div class="sidePanelActions" {
                                button id="panel-close" class="button button--ghost" type="button" { "Close" }
                            }
                        }

                        div id="controls-resizer" class="sidePanelResizer sidePanelResizer--controls" aria-hidden="true" {}

                        div class="sidePanelBody" {
                            section class="sideSection" {
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
                                div class="sectionLabel" { "Categories" }
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
                                    button id="category-add" class="button button--ghost" type="button" { "Add" }
                                }
                                div id="selected-category-list" class="chipList" {}
                                div class="sectionLabel" { "Available categories" }
                                div id="category-filter-list" class="checkList" {}
                                div class="actionRow" {
                                    button id="apply-filters" class="button button--primary" type="button" { "Apply view" }
                                }
                            }

                            section class="sideSection" {
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

                            section class="sideSection" {
                                div class="sectionHead" {
                                    div {
                                        div class="sectionLabel" { "view" }
                                        h3 class="sectionTitle" { "Graph view" }
                                    }
                                    div class="pillRow" {
                                        span id="mode-pill" class="pill pill--mode" { "view" }
                                        span id="origin-pill" class="pill pill--origin" { "local / view none" }
                                    }
                                }
                                div class="pillRow" {
                                    span id="row-pill" class="pill" { "0 nodes" }
                                    span id="link-pill" class="pill" { "0 links" }
                                    span id="filter-pill" class="pill" { "0 filters" }
                                }
                                div class="section" {
                                    div class="sectionLabel" { "view name" }
                                    p id="origin-copy" class="mutedCopy" {
                                        "The current view name, source binding, and stream origin live here."
                                    }
                                    pre id="origin-text" class="codeBlock" { "Waiting for host metadata..." }
                                    div class="sectionLabel" { "view sql" }
                                    pre id="view-sql" class="codeBlock" { "Waiting for view SQL..." }
                                }
                            }
                        }
                    }

                    aside id="record-panel" class="sidePanel sidePanel--record" hidden="" {
                        div class="sidePanelHead" {
                            div {
                                div class="eyebrow" { "selected" }
                                h2 class="panelTitle" { "Details" }
                            }
                            div class="sidePanelActions" {
                                button id="record-save" class="button button--primary" type="button" { "Save" }
                                button id="record-delete" class="button button--danger" type="button" { "Delete" }
                                button id="record-close" class="button button--ghost" type="button" { "Close" }
                            }
                        }

                        div id="record-resizer" class="sidePanelResizer sidePanelResizer--record" aria-hidden="true" {}

                        div class="sidePanelBody sidePanelBody--record" {
                            div class="recordEditor" {
                                div class="fieldGroup fieldGroup--id" {
                                    div class="fieldLabel" { "id" }
                                    div id="record-id" class="recordId" { "-" }
                                }
                                div class="fieldGroup" {
                                    label class="fieldLabel" for="record-quantity" { "quantity" }
                                    input
                                        id="record-quantity"
                                        class="input"
                                        type="number"
                                        step="1"
                                        value="0"
                                    ;
                                }
                                div class="fieldGroup" {
                                    label class="fieldLabel" for="record-head" { "head" }
                                    textarea
                                        id="record-head"
                                        class="textarea textarea--compact"
                                        rows="2"
                                        spellcheck="false"
                                        placeholder="Head"
                                    {}
                                }
                                div class="fieldGroup" {
                                    label class="fieldLabel" for="record-body" { "body" }
                                    textarea
                                        id="record-body"
                                        class="textarea"
                                        rows="4"
                                        spellcheck="true"
                                        placeholder="Body"
                                    {}
                                }
                                div class="fieldGroup" {
                                    div class="fieldLabel" { "categories" }
                                    div id="record-category-list" class="chipList" {}
                                    input
                                        id="record-category-input"
                                        class="input"
                                        type="text"
                                        autocomplete="off"
                                        spellcheck="false"
                                        placeholder="Category"
                                    ;
                                }
                                div class="fieldGroup" {
                                    div class="fieldLabel" { "current parents" }
                                    div id="current-parent-list" class="relationRowList" {}
                                }
                                div class="fieldGroup" {
                                    label class="fieldLabel" for="parent-search-query" { "find parent" }
                                    input
                                        id="parent-search-query"
                                        class="input"
                                        type="text"
                                        autocomplete="off"
                                        spellcheck="false"
                                        placeholder="Search possible fathers by head"
                                    ;
                                    div id="parent-search-summary" class="mutedCopy" {}
                                    div id="parent-choice-list" class="parentChoiceList" {}
                                }
                                div class="fieldGroup" {
                                    div class="fieldLabel" { "children" }
                                    div id="child-list" class="chipList" {}
                                }
                            }
                        }
                    }

                    aside id="create-panel" class="sidePanel sidePanel--create" hidden="" {
                        div class="sidePanelHead" {
                            div {
                                div class="eyebrow" { "create" }
                                h2 class="panelTitle" { "Create record" }
                            }
                            div class="sidePanelActions" {
                                button id="create-close" class="button button--ghost" type="button" { "Close" }
                            }
                        }

                        div class="sidePanelBody sidePanelBody--create" {
                            section id="creator" class="sideSection sideSection--create" {
                                div class="sectionHead" {
                                    div {
                                        div class="sectionLabel" { "create" }
                                        h3 class="sectionTitle" { "Record" }
                                    }
                                }
                                p id="create-summary" class="mutedCopy" {
                                    "New records inherit the categories currently applied to this view."
                                }

                                div class="section" {
                                    label class="fieldLabel" for="create-head" { "Head" }
                                    input
                                        id="create-head"
                                        class="input"
                                        type="text"
                                        autocomplete="off"
                                        spellcheck="false"
                                        placeholder="Record head"
                                    ;
                                }

                                div class="section" {
                                    label class="fieldLabel" for="create-body" { "Body" }
                                    textarea
                                        id="create-body"
                                        class="textarea"
                                        rows="4"
                                        spellcheck="true"
                                        placeholder="Optional body"
                                    {}
                                }

                                div class="section" {
                                    label class="fieldLabel" for="create-quantity" { "Quantity" }
                                    input
                                        id="create-quantity"
                                        class="input"
                                        type="number"
                                        step="1"
                                        value="0"
                                    ;
                                }

                                div class="section" {
                                    div class="sectionHead" {
                                        div {
                                            label class="fieldLabel" for="create-parent-search" { "Parent" }
                                            p id="create-parent-summary" class="mutedCopy" {
                                                "Selected: no parent"
                                            }
                                        }
                                        button id="create-parent-clear" class="button button--ghost" type="button" { "No parent" }
                                    }
                                    input
                                        id="create-parent-search"
                                        class="input"
                                        type="text"
                                        autocomplete="off"
                                        spellcheck="false"
                                        placeholder="Search possible fathers by head"
                                    ;
                                    div id="create-parent-choice-list" class="parentChoiceList parentChoiceList--compact" {}
                                }

                                div class="section" {
                                    div class="sectionLabel" { "Applied categories" }
                                    div id="create-category-list" class="chipList" {}
                                }

                                div class="actionRow actionRow--split actionRow--sticky" {
                                    button id="create-clear" class="button button--ghost" type="button" { "Clear" }
                                    button id="create-submit" class="button button--primary" type="button" { "Create record" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
