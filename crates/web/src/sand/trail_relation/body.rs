use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main #trail-root.trailShell {
            header.trailHeader {
                div {
                    h1 { "Trail Relation" }
                    p.small { "Search roots by assignee, category, and head. Bind one or create a copied trail, then sync and progress it." }
                }
                div #trail-status.status { "Loading contract..." }
            }

            section.panel {
                h2 { "Discover" }
                form #trail-search-form.grid {
                    label.field {
                        span { "Assignee id" }
                        input #trail-search-assignee.fieldInput type="number" min="1" placeholder="optional";
                    }
                    label.field {
                        span { "Category" }
                        input #trail-search-category.fieldInput type="text" placeholder="documentation, copy";
                    }
                    label.field {
                        span { "Head contains" }
                        input #trail-search-head.fieldInput type="text" placeholder="sickness";
                    }
                    button #trail-search-submit.button type="submit" { "Search" }
                }
                div #trail-search-results.resultList {}
            }

            section.panel {
                h2 { "Create Trail" }
                form #trail-create-form.grid {
                    label.field {
                        span { "Source record id" }
                        input #trail-create-source.fieldInput type="number" min="1" required;
                    }
                    label.field {
                        span { "Assignee id" }
                        input #trail-create-assignee.fieldInput type="number" min="1" required;
                    }
                    label.field {
                        span { "Scope" }
                        select #trail-sync-scope.fieldInput {
                            option value="t" selected { "Tree" }
                            option value="n" { "Node" }
                            option value="nt" { "Both" }
                        }
                    }
                    label.field {
                        span { "Fields (qhb)" }
                        input #trail-sync-fields.fieldInput type="text" value="hb" placeholder="hb";
                    }
                    button #trail-create-submit.button.buttonAccent type="submit" { "Create" }
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
