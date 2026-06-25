use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main id="transfer-app" class="transferApp" {
            datalist id="record-options" {}
            datalist id="organ-options" {}

            aside id="settings-panel" class="settingsPanel" aria-label="Transfer settings" aria-hidden="true" {
                div class="settingsBackdrop" data-action="close-settings" {}
                div class="settingsDrawer" {
                    div class="panelHead" {
                        h2 { "Transfer settings" }
                        button
                            type="button"
                            class="iconButton"
                            data-action="close-settings"
                            aria-label="Close settings"
                            title="Close settings"
                            data-keep-enabled="true"
                        {
                            "×"
                        }
                    }

                    section class="panel drawerPanel" aria-labelledby="identity-title" {
                        div class="panelHead" {
                            h2 id="identity-title" { "Signing identity" }
                        }
                        div id="identity-summary" class="panelBody" {}
                        form id="identity-form" class="formGrid" {
                            label {
                                span { "Label" }
                                input id="identity-label" name="label" autocomplete="off" placeholder="my-cell" {}
                            }
                            p class="muted" {
                                "Created automatically on this node. The private key stays local; Transfer events share only the public key and signatures."
                            }
                            div class="formActions" {
                                button id="identity-save" type="button" class="primary" {
                                    "Save label"
                                }
                                button id="identity-reset" type="button" class="danger" {
                                    "Reset key"
                                }
                            }
                        }
                    }

                    section class="panel drawerPanel" aria-labelledby="visualization-title" {
                        div class="panelHead" {
                            h2 id="visualization-title" { "Visualization" }
                        }
                        div class="formActions" {
                            button
                                id="visualization-structured"
                                type="button"
                                data-action="structured-transfer-view"
                                data-keep-enabled="true"
                            {
                                "Structured"
                            }
                            button
                                id="visualization-graph"
                                type="button"
                                class="primary"
                                data-action="graph-transfer-view"
                                data-keep-enabled="true"
                            {
                                "D3 graph"
                            }
                        }
                    }

                    section class="panel drawerPanel" aria-labelledby="ingress-title" {
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

                    section class="panel drawerPanel" aria-labelledby="network-title" {
                        div class="panelHead" {
                            h2 id="network-title" { "Network" }
                        }
                        div id="network-summary" class="panelBody" {}
                        form id="network-policy-form" class="formGrid" {
                            label class="checkRow" {
                                input id="known-peer-polling-enabled" name="knownPeerPollingEnabled" type="checkbox" {}
                                span { "Poll known peers" }
                            }
                            label class="checkRow" {
                                input id="share-quantity-projections" name="shareQuantityProjections" type="checkbox" {}
                                span { "Share quantity projections" }
                            }
                            button type="submit" { "Save network" }
                        }
                        form id="receipt-policy-form" class="formGrid" {
                            label class="checkRow" {
                                input id="send-received-receipts" name="sendReceivedReceipts" type="checkbox" {}
                                span { "Send received receipts" }
                            }
                            label class="checkRow" {
                                input id="send-seen-receipts" name="sendSeenReceipts" type="checkbox" {}
                                span { "Send seen receipts" }
                            }
                            label class="checkRow" {
                                input id="anonymous-package-viewing" name="anonymousPackageViewing" type="checkbox" {}
                                span { "Anonymous package viewing" }
                            }
                            button type="submit" { "Save receipts" }
                        }
                        form id="contact-discovery-form" class="formGrid networkDiscoveryForm" {
                            label {
                                span { "Node URL" }
                                input id="contact-discovery-base-url" name="baseUrl" autocomplete="off" placeholder="http://127.0.0.1:3000" {}
                            }
                            label {
                                span { "Search" }
                                input id="contact-discovery-search" name="search" autocomplete="off" placeholder="name or URL" {}
                            }
                            button type="submit" { "Discover contacts" }
                        }
                        div id="contact-discovery-list" class="compactList" {}
                        div id="peer-list" class="compactList peerList" {}
                    }

                    section class="panel drawerPanel" aria-labelledby="organ-login-title" {
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

                    section class="panel drawerPanel" aria-labelledby="record-title" {
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
            }

            div id="reset-key-modal" class="modalLayer" aria-hidden="true" {
                div class="modalCard" role="dialog" aria-modal="true" aria-labelledby="reset-key-title" {
                    h2 id="reset-key-title" { "Reset signing key?" }
                    p class="muted" {
                        "This creates a new local private/public key pair. Existing Transfers signed by the old key will not be owned by the new key."
                    }
                    div class="formActions" {
                        button type="button" data-action="cancel-reset-key" data-keep-enabled="true" { "Cancel" }
                        button type="button" class="danger" data-action="confirm-reset-key" data-keep-enabled="true" { "Reset key" }
                    }
                }
            }

            section class="workspace" {
                aside class="transferBrowser" aria-label="Transfer browser" {
                    input id="transfer-search" class="searchInput" autocomplete="off" placeholder="Search" {}
                    section class="panel transfersPanel" aria-labelledby="transfers-title" {
                        div class="browserHead" {
                            div id="transfer-tabs" class="tabs" {}
                            div id="transfer-count" class="muted" {}
                        }
                        div id="transfer-list" class="transferList" {}
                    }
                }

                section class="mainColumn" {
                    header class="utilityBar" {
                        div {
                            h1 { "Transfer" }
                            p id="transfer-status" class="status" data-tone="idle" { "Loading" }
                        }
                        div class="topActions" {
                            button
                                id="proposal-create"
                                type="button"
                                class="iconButton primary"
                                data-action="new-transfer"
                                aria-label="Create Transfer"
                                title="Create Transfer"
                            {
                                "+"
                            }
                            button
                                id="visualization-toggle"
                                type="button"
                                data-action="toggle-transfer-visualization"
                                data-keep-enabled="true"
                                aria-label="Switch to graph view"
                                title="Switch to graph view"
                            {
                                "Graph"
                            }
                            button
                                type="button"
                                class="iconButton"
                                data-action="open-settings"
                                aria-label="Transfer settings"
                                title="Transfer settings"
                            {
                                "⚙"
                            }
                            button
                                type="button"
                                class="iconButton"
                                data-action="refresh"
                                aria-label="Refresh"
                                title="Refresh"
                            {
                                "↻"
                            }
                        }
                    }
                    section class="panel transferWorkspace" aria-labelledby="transfers-title" {
                        h2 id="transfers-title" class="srOnly" { "Transfer workspace" }
                        section id="transfer-graph-view" class="transferGraphView" aria-label="Transfer graph" aria-hidden="true" {
                            div class="graphToolbar" {
                                div {
                                    h2 { "Transfer graph" }
                                    p id="transfer-graph-summary" class="meta" { "0 transfers" }
                                }
                                div class="graphActions" {
                                    button type="button" data-action="fit-transfer-graph" data-keep-enabled="true" { "Fit" }
                                    button type="button" data-action="structured-transfer-view" data-keep-enabled="true" { "Structured" }
                                }
                            }
                            svg id="transfer-graph" class="transferGraph" role="img" aria-label="Transfer graph nodes" {}
                        }
                        article id="transfer-detail" class="transferDetail" {}
                    }
                }
            }
        }
    }
}
