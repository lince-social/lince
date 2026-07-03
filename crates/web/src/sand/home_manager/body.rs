use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="home-manager" class="homeManager" data-lince-bridge-root {
            nav class="tabs" aria-label="Home Manager tabs" {
                button class="tab isActive" type="button" data-tab="nutrition" { "Nutrition" }
                button class="tab" type="button" data-tab="bills" { "Bills" }
            }

            section id="nutrition-tab" class="tabPanel nutritionPanel" {
                header class="topbar" {
                    div {
                        div class="eyebrow" { "home manager" }
                        h1 { "Nutrition" }
                    }
                    div class="guideStrip" aria-label="Brazilian food guide summary" {
                        span data-tone="base" { "Base: in natura" }
                        span data-tone="use" { "Use: oils/salt/sugar" }
                        span data-tone="limit" { "Limit: processed" }
                        span data-tone="avoid" { "Avoid: ultraprocessed" }
                    }
                }

                section class="metrics" aria-label="Nutrition plan summary" {
                    (metric("metric-foods", "foods", "0"))
                    (metric("metric-kcal", "kcal/day", "0"))
                    (metric("metric-price", "price", "0"))
                    (metric("metric-volume", "volume", "0 L"))
                }

                section class="workspace" {
                    aside class="panel controlsPanel" {
                        div class="panelHeader" { h2 { "Inputs" } }
                        div class="formGrid" {
                            label { span { "weight kg" } input id="profile-weight" type="number" min="30" max="250" step="1"; }
                            label { span { "height cm" } input id="profile-height" type="number" min="120" max="230" step="1"; }
                            label { span { "age" } input id="profile-age" type="number" min="12" max="100" step="1"; }
                            label { span { "sex" } select id="profile-sex" { option value="female" { "female" } option value="male" { "male" } } }
                            label { span { "activity" } input id="profile-activity" type="number" min="1.1" max="2.2" step="0.05"; }
                            label { span { "days" } input id="plan-days" type="number" min="1" max="31" step="1"; }
                            label { span { "marmitas" } input id="plan-pots" type="number" min="1" max="200" step="1"; }
                            label { span { "pot L" } input id="plan-pot-volume" type="number" min="0.2" max="3" step="0.05"; }
                            label { span { "meals/day" } input id="plan-meals" type="number" min="1" max="8" step="1"; }
                            label { span { "fiber min g" } input id="constraint-fiber" type="number" min="0" max="80" step="1"; }
                            label { span { "protein min g" } input id="constraint-protein" type="number" min="0" max="220" step="1"; }
                        }
                        div class="actions" {
                            button id="generate-plan" class="button primary" type="button" { "Generate" }
                            button id="optimize-plan" class="button" type="button" { "Optimize" }
                            button id="save-plan" class="button subtle" type="button" { "Save" }
                        }
                        div id="status" class="status" { "Ready" }
                    }

                    section class="panel catalogPanel" {
                        div class="panelHeader" {
                            h2 { "Food Catalog" }
                            input id="food-search" type="search" placeholder="search foods";
                        }
                        div class="catalogTools" {
                            select id="category-filter" {}
                            button id="new-alimentum" class="button subtle" type="button" { "New alimentum" }
                        }
                        div id="food-list" class="foodList" {}
                    }

                    section class="panel planPanel" {
                        div class="panelHeader" { h2 { "Marmitas" } }
                        div id="marmita-list" class="marmitaList" {}
                    }

                    section class="panel shoppingPanel" {
                        div class="panelHeader" { h2 { "Nutrients + Shopping" } }
                        div id="nutrient-totals" class="totalsGrid" {}
                        div id="shopping-list" class="shoppingList" {}
                    }
                }
            }

            section id="bills-tab" class="tabPanel billsPanel" hidden {
                div class="panel emptyPanel" {
                    div class="panelHeader" { h2 { "Bills" } }
                    p { "Bills will live here. This pass keeps the tab thin while Nutrition is implemented end to end." }
                }
            }

            dialog id="alimentum-dialog" class="dialog" {
                form method="dialog" id="alimentum-form" class="dialogBody" {
                    div class="panelHeader" {
                        h2 { "Alimentum" }
                        button class="iconButton" value="cancel" type="button" onclick="document.getElementById('alimentum-dialog')?.close()" { "x" }
                    }
                    div class="formGrid dialogGrid" {
                        input id="alimentum-record-id" type="hidden";
                        input id="alimentum-extension-id" type="hidden";
                        label { span { "name" } input id="alimentum-name" required; }
                        label { span { "category" } input id="alimentum-category" required; }
                        label { span { "price/kg" } input id="alimentum-price" type="number" min="0" step="0.01"; }
                        label { span { "density g/ml" } input id="alimentum-density" type="number" min="0.1" max="2" step="0.01"; }
                    }
                    div id="alimentum-nutrients" class="formGrid dialogGrid nutrientEditor" {}
                    div class="actions" {
                        button id="alimentum-submit" class="button primary" value="default" type="button" { "Save alimentum" }
                    }
                }
            }
        }
    }
}

fn metric(id: &'static str, label: &'static str, value: &'static str) -> Markup {
    html! {
        div class="metric" {
            span { (label) }
            strong id=(id) { (value) }
        }
    }
}
