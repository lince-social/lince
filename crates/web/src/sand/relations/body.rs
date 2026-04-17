use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="app" class="app" data-lince-bridge-root {
            header class="hero panel" {
                div class="heroCopy" {
                    div class="eyebrow" { "origin" }
                    h1 id="title" class="title" { "Relations" }
                    p id="copy" class="copy" {
                        "A directed graph for parent and child records. Persistent filters rewrite the generated view, while graph physics stays local to this widget."
                    }
                }
                div class="heroMeta" {
                    span id="mode-pill" class="pill pill--mode" { "view" }
                    span id="origin-pill" class="pill" { "Waiting for origin" }
                    span id="status-pill" class="pill pill--status" { "Booting" }
                }
            }

            section class="panel graphPanel" {
                div class="panelHead" {
                    div class="panelHeadCopy" {
                        div class="eyebrow" { "view" }
                        h2 class="panelTitle" { "Graph" }
                    }
                    div class="panelToolbar" {
                        div class="panelChips" {
                            span id="row-pill" class="pill" { "0 nodes" }
                            span id="link-pill" class="pill" { "0 links" }
                            span id="filter-pill" class="pill" { "0 filters" }
                            span id="zoom-pill" class="pill" { "100%" }
                        }
                        div class="toolbarButtons" {
                            button id="create-open" class="button button--primary" type="button" { "Create" }
                            button id="zoom-out" class="button button--ghost" type="button" aria-label="Zoom out" { "-" }
                            button id="zoom-fit" class="button button--ghost" type="button" { "Fit" }
                            button id="zoom-in" class="button button--ghost" type="button" aria-label="Zoom in" { "+" }
                            button id="panel-toggle" class="button button--ghost" type="button" { "Panel" }
                        }
                    }
                }

                div class="graphWorkspace" {
                    div class="graphStage" {
                        canvas id="graph" aria-label="Relations graph" {}
                        div class="graphHint" {
                            span class="pill graphHint__pill" { "Wheel or pinch to zoom" }
                            span class="pill graphHint__pill" { "Drag the background to pan" }
                        }
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

                    aside id="side-panel" class="sidePanel" hidden="" {
                        div class="sidePanelHead" {
                            div {
                                div class="eyebrow" { "panel" }
                                h2 class="panelTitle" { "Relations controls" }
                            }
                            div class="sidePanelActions" {
                                a class="linkChip" href="LICENSE.txt" target="_blank" rel="noreferrer" { "License" }
                                button id="panel-close" class="button button--ghost" type="button" { "Close" }
                            }
                        }

                        div class="sidePanelBody" {
                            section id="details" class="sideSection" {
                                div class="sectionHead" {
                                    div {
                                        div class="sectionLabel" { "selection" }
                                        h3 class="sectionTitle" { "Parent link" }
                                    }
                                    span id="selection-pill" class="pill" { "None" }
                                }

                                div id="selection-empty" class="selectionEmpty" {
                                    p class="mutedCopy" {
                                        "Select a node. Add parents without replacing the existing ones, or click a current parent to remove only that link. Leave nothing selected to remove all parents."
                                    }
                                }

                                div id="selection-content" hidden="" {
                                    pre id="selection-summary" class="codeBlock" {}

                                    div class="section" {
                                        div class="sectionHead" {
                                            div {
                                                div class="sectionLabel" { "record" }
                                                h3 class="sectionTitle" { "Edit record" }
                                            }
                                        }
                                        p id="record-editor-copy" class="mutedCopy" {
                                            "Load the selected record before editing it."
                                        }
                                        label class="fieldLabel" for="record-head" { "Head" }
                                        input
                                            id="record-head"
                                            class="input"
                                            type="text"
                                            autocomplete="off"
                                            spellcheck="false"
                                            placeholder="Record head"
                                        ;
                                        label class="fieldLabel" for="record-body" { "Body" }
                                        textarea
                                            id="record-body"
                                            class="textarea"
                                            rows="4"
                                            spellcheck="true"
                                            placeholder="Optional body"
                                        {}
                                        label class="fieldLabel" for="record-quantity" { "Quantity" }
                                        input
                                            id="record-quantity"
                                            class="input"
                                            type="number"
                                            step="1"
                                            value="0"
                                        ;
                                        div class="sectionHead" {
                                            div {
                                                div class="sectionLabel" { "Categories" }
                                                p class="mutedCopy" {
                                                    "Changing categories updates the record itself."
                                                }
                                            }
                                        }
                                        div class="actionRow actionRow--split" {
                                            input
                                                id="record-category-input"
                                                class="input"
                                                type="text"
                                                autocomplete="off"
                                                spellcheck="false"
                                                placeholder="Add category"
                                            ;
                                            button id="record-category-add" class="button button--ghost" type="button" { "Add" }
                                        }
                                        div id="record-category-list" class="chipList" {}
                                        div class="sectionLabel" { "Available categories" }
                                        div id="record-category-choice-list" class="checkList" {}
                                        div class="actionRow actionRow--split" {
                                            button id="record-save" class="button button--primary" type="button" { "Save record" }
                                            button id="record-delete" class="button button--danger" type="button" { "Delete record" }
                                        }
                                    }

                                    div class="actionRow actionRow--split" {
                                        button id="connect-parent" class="button button--primary" type="button" { "Set parent" }
                                        button id="disconnect-parent" class="button button--ghost" type="button" { "Remove parent" }
                                    }

                                    div class="section" {
                                        div class="sectionLabel" { "Current parents" }
                                        p class="mutedCopy" {
                                            "Click a parent chip to remove just that link. If nothing is selected, removing clears every parent link."
                                        }
                                        div id="current-parent-list" class="chipList" {}
                                    }

                                    div class="section" {
                                        label class="fieldLabel" for="parent-search-query" { "Find parent" }
                                        input
                                            id="parent-search-query"
                                            class="input"
                                            type="text"
                                            autocomplete="off"
                                            spellcheck="false"
                                            placeholder="Search possible fathers by head"
                                        ;
                                        p id="parent-search-summary" class="mutedCopy" {
                                            "Choose a possible father from the current graph."
                                        }
                                        div id="parent-choice-list" class="parentChoiceList" {}
                                    }

                                    div class="section" {
                                        div class="sectionLabel" { "Children" }
                                        div id="child-list" class="chipList" {}
                                    }
                                }
                            }

                            section id="editor" class="sideSection" hidden="" {
                                div class="sectionHead" {
                                    div {
                                        div class="sectionLabel" { "origin" }
                                        h3 class="sectionTitle" { "Source binding" }
                                    }
                                }
                                p id="origin-copy" class="mutedCopy" {
                                    "The widget uses the host origin. Change it on the board card, not here."
                                }
                                pre id="origin-text" class="codeBlock" { "Waiting for host metadata..." }

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
                        }
                    }
                }
            }
        }
    }
}
