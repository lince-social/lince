use maud::{Markup, html};

pub(crate) fn body() -> Markup {
    html! {
        main class="accessApp" {
            header class="topbar" {
                div {
                    div class="eyebrow" { "Access control" }
                    h1 { "Roles, permissions, users" }
                }
                div class="topActions" {
                    span id="server-pill" class="pill" { "server unset" }
                    button id="refresh-button" class="button" type="button" { "Refresh" }
                }
            }

            div id="status" class="status" data-tone="idle" { "Waiting for server" }

            section class="layout" {
                aside class="panel listPanel" {
                    div class="panelHeader" {
                        h2 { "Roles" }
                        button id="new-role-button" class="button buttonPrimary" type="button" { "New role" }
                    }
                    div id="role-list" class="roleList" {}
                }

                section class="panel detailPanel" {
                    div class="panelHeader split" {
                        div {
                            div class="eyebrow" { "Selected role" }
                            h2 id="role-title" { "No role selected" }
                        }
                        span id="role-count" class="pill" { "0 permissions" }
                    }
                    div id="permission-grid" class="permissionGrid" {}
                }

                section class="panel userPanel" {
                    div class="panelHeader" {
                        h2 { "Users" }
                        button id="new-user-button" class="button buttonPrimary" type="button" { "New user" }
                    }
                    div id="user-list" class="userList" {}
                }
            }
        }

        div id="modal-backdrop" class="modalBackdrop" hidden {
            section class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title" {
                div class="modalHeader" {
                    h2 id="modal-title" { "Create" }
                    button id="modal-close" class="iconButton" type="button" aria-label="Close" { "x" }
                }
                form id="modal-form" class="form" {
                    div id="modal-fields" class="formFields" {}
                    div class="modalActions" {
                        button id="modal-cancel" class="button" type="button" { "Cancel" }
                        button id="modal-submit" class="button buttonPrimary" type="submit" { "Save" }
                    }
                }
            }
        }
    }
}
