use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main #trail-root.trailShell {
            header.trailHeader {
                div {
                    h1 { "Trail Relation" }
                    p.small { "Discover records by assignee, category, and head. Select an original, create a copied trail, then sync and progress it." }
                }
                div #trail-status.status { "Loading contract..." }
            }

            section #trail-discover-panel.panel {
                h2 { "Discover" }
                div.discoverLayout {
                    div.discoverFilters {
                        label.field {
                            span { "Head contains" }
                            input #trail-search-head.fieldInput type="text" placeholder="record name";
                        }
                        label.field {
                            span { "Category" }
                            input #trail-search-category.fieldInput type="text" placeholder="documentation, copy";
                        }
                        label.field.autocompleteHost {
                            span { "Assignee" }
                            input #trail-search-assignee.fieldInput type="text" placeholder="name, username, or id";
                            div #trail-search-assignee-suggestions.suggestionPanel hidden {}
                        }
                        p.small { "Discover fetches the record dataset once, then filters it locally while you type." }
                    }
                    div.discoverResults {
                        p #trail-search-summary.small.searchSummary { "Loading records..." }
                        div.resultListShell {
                            div #trail-search-results.resultList {}
                        }
                    }
                }
            }

            section.panel {
                h2 { "Create Trail" }
                div #trail-create-form.grid {
                    label.field.fieldWide {
                        span { "Original record" }
                        div.inlineField {
                            input #trail-create-source-label.fieldInput type="text" placeholder="Type here to narrow Discover, or click a Discover item";
                            button #trail-create-source-clear.button type="button" { "Clear" }
                        }
                        input #trail-create-source type="hidden";
                        p.small { "Typing here narrows Discover locally. Clicking a Discover result selects the original root." }
                    }
                    label.field.autocompleteHost {
                        span { "Assignee" }
                        input #trail-create-assignee.fieldInput type="text" placeholder="name, username, or id" required;
                        div #trail-create-assignee-suggestions.suggestionPanel hidden {}
                    }
                    label.field {
                        span { "Scope" }
                        select #trail-sync-scope.fieldInput {
                            option value="t" selected { "Tree" }
                            option value="n" { "Node" }
                            option value="nt" { "Both" }
                        }
                        p #trail-sync-scope-copy.small { "Tree syncs children only." }
                    }
                    fieldset.field.fieldWide.choiceFieldset {
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
                        p.small { "Checked properties are overwritten from the original when sync runs." }
                    }
                    button #trail-create-submit.button.buttonAccent.fieldWide type="button" disabled { "Create trail" }
                }
            }

            section.panel {
                h2 { "Sync" }
                p #trail-binding-copy.small { "No trail bound." }
                p #trail-overwrite-copy.warning hidden { "" }
                button #trail-sync-submit.button type="button" { "Run sync" }
            }

            section.panel {
                h2 { "Trail" }
                div #trail-tree.tree {}
            }
        }
    }
}
