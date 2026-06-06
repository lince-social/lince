use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="transfer-app" class="transferApp" {
            header class="topbar" {
                div {
                    h1 { "Transfer" }
                    p id="transfer-status" class="status" data-tone="idle" { "Loading" }
                }
                div class="topActions" {
                    button type="button" data-action="refresh" { "Refresh" }
                }
            }

            datalist id="record-options" {}
            datalist id="organ-options" {}

            section class="workspace" {
                aside class="sidebar" {
                    section class="panel" aria-labelledby="identity-title" {
                        div class="panelHead" {
                            h2 id="identity-title" { "Local party" }
                        }
                        div id="identity-summary" class="panelBody" {}
                        form id="identity-form" class="formGrid" {
                            label {
                                span { "Label" }
                                input id="identity-label" name="label" autocomplete="off" placeholder="my-cell" {}
                            }
                            p class="muted" {
                                "This creates one persistent local signing identity for this node. It stays in the local database and is reused automatically for Transfer signatures."
                            }
                            button id="identity-save" type="button" class="primary" {
                                "Save signing identity"
                            }
                        }
                    }

                    section class="panel" aria-labelledby="ingress-title" {
                        div class="panelHead" {
                            h2 id="ingress-title" { "Ingress" }
                        }
                        div id="ingress-summary" class="panelBody" {}
                        form id="ingress-form" class="formGrid" {
                            label class="checkRow" {
                                input id="public-proposals-enabled" name="publicProposalsEnabled" type="checkbox" {}
                                span { "Accept public proposals" }
                            }
                            button type="submit" { "Save ingress" }
                        }
                    }

                    section class="panel" aria-labelledby="organ-login-title" {
                        div class="panelHead" {
                            h2 id="organ-login-title" { "Organ login" }
                        }
                        form id="organ-login-form" class="formGrid" {
                            label {
                                span { "Organ" }
                                select id="login-organ" name="organ" {}
                            }
                            label {
                                span { "Username" }
                                input id="login-username" name="username" autocomplete="username" {}
                            }
                            label {
                                span { "Password" }
                                input id="login-password" name="password" type="password" autocomplete="current-password" {}
                            }
                            button type="submit" { "Login Organ" }
                        }
                    }

                    section class="panel" aria-labelledby="record-title" {
                        div class="panelHead" {
                            h2 id="record-title" { "Records" }
                        }
                        form id="record-form" class="formGrid" {
                            label {
                                span { "Head" }
                                input id="record-head" name="head" autocomplete="off" placeholder="Record head" {}
                            }
                            label {
                                span { "Quantity" }
                                input id="record-quantity" name="quantity" inputmode="decimal" value="1" {}
                            }
                            label {
                                span { "Body" }
                                textarea id="record-body" name="body" rows="2" {}
                            }
                            button type="submit" { "Create Record" }
                        }
                        div id="record-list" class="compactList" {}
                    }
                }

                section class="mainColumn" {
                    section class="panel" aria-labelledby="proposal-title" {
                        div class="panelHead" {
                            h2 id="proposal-title" { "New proposal" }
                        }
                        form id="proposal-form" class="proposalGrid" {
                            label {
                                span { "Title" }
                                input id="proposal-title-input" name="title" value="Record transfer" autocomplete="off" {}
                            }
                            label {
                                span { "Local side" }
                                select id="proposal-role" name="role" {
                                    option value="need" { "Need" }
                                    option value="contribution" { "Contribution" }
                                }
                            }
                            label {
                                span { "Local Record" }
                                input id="proposal-record" name="record" list="record-options" autocomplete="off" placeholder="Select Record" {}
                            }
                            label {
                                span { "Quantity" }
                                input id="proposal-quantity" name="quantity" inputmode="decimal" value="1" {}
                            }
                            label {
                                span { "Counterparty" }
                                input id="proposal-counterparty" name="counterparty" list="organ-options" autocomplete="off" placeholder="other-cell" {}
                            }
                            label {
                                span { "Post target" }
                                select id="proposal-organ" name="organ" {}
                            }
                            div class="formActions" {
                                button id="proposal-create" type="button" class="primary" {
                                    "Create proposal"
                                }
                            }
                        }
                    }

                    section class="panel transfersPanel" aria-labelledby="transfers-title" {
                        div class="panelHead splitHead" {
                            h2 id="transfers-title" { "Transfers" }
                            div id="transfer-count" class="muted" {}
                        }
                        div class="transferLayout" {
                            div id="transfer-list" class="transferList" {}
                            article id="transfer-detail" class="transferDetail" {}
                        }
                    }
                }
            }
        }
    }
}
