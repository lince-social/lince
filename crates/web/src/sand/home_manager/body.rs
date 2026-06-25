use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="home-manager" class="homeManager" {
            header class="hero" {
                div {
                    div class="eyebrow" { "nix profile" }
                    h1 { "Home Manager" }
                    p class="lede" { "Compose a user environment, stage activations, and keep rollback context visible." }
                }
                div class="generationCard" {
                    span class="label" { "active generation" }
                    strong id="generation-number" { "42" }
                    span id="generation-label" { "stable workstation" }
                }
            }

            section class="toolbar" aria-label="Home Manager controls" {
                button id="activate-button" class="button buttonPrimary" type="button" { "Activate" }
                button id="rollback-button" class="button" type="button" { "Rollback" }
                label class="searchWrap" for="module-filter" {
                    span class="label" { "filter" }
                    input id="module-filter" type="search" placeholder="module, package, service";
                }
                label class="selectWrap" for="profile-select" {
                    span class="label" { "profile" }
                    select id="profile-select" {
                        option value="workstation" { "workstation" }
                        option value="portable" { "portable" }
                        option value="studio" { "studio" }
                    }
                }
            }

            section class="summaryGrid" aria-label="Profile summary" {
                (summary_tile("modules-enabled", "modules", "0 / 0"))
                (summary_tile("packages-count", "packages", "0"))
                (summary_tile("services-running", "services", "0 / 0"))
                (summary_tile("drift-count", "drift", "0"))
            }

            section class="workspace" {
                div class="panel modulesPanel" {
                    div class="panelHeader" {
                        h2 { "Modules" }
                        span id="module-count" class="meta" { "0 enabled" }
                    }
                    div id="module-list" class="moduleList" {}
                }

                div class="panel packagePanel" {
                    div class="panelHeader" {
                        h2 { "Packages" }
                        button id="add-package-button" class="iconButton" type="button" aria-label="Add package" title="Add package" { "+" }
                    }
                    form id="package-form" class="packageForm" {
                        input id="package-input" type="text" placeholder="attribute name";
                        button class="button" type="submit" { "Add" }
                    }
                    div id="package-list" class="packageList" {}
                }

                div class="panel servicePanel" {
                    div class="panelHeader" {
                        h2 { "Services" }
                        span id="service-state" class="meta" { "idle" }
                    }
                    div id="service-list" class="serviceList" {}
                }

                div class="panel activityPanel" {
                    div class="panelHeader" {
                        h2 { "Activation" }
                        span id="activation-state" class="meta" { "clean" }
                    }
                    ol id="activation-list" class="activationList" {}
                }
            }
        }
    }
}

fn summary_tile(id: &'static str, label: &'static str, value: &'static str) -> Markup {
    html! {
        div class="summaryTile" {
            span class="label" { (label) }
            strong id=(id) { (value) }
        }
    }
}
