use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="ghosttyTerminal" {
            header class="ghosttyChrome" {
                div class="ghosttyHeading" {
                    div class="ghosttyEyebrow" { "libghostty-vt" }
                    h1 class="ghosttyTitle" { "Ghostty Terminal" }
                }

                div class="ghosttyActions" {
                    button id="follow-button" class="ghosttyButton" type="button" data-state="on" {
                        "Follow"
                    }
                    button id="interrupt-button" class="ghosttyButton ghosttyButton--danger" type="button" {
                        "Ctrl+C"
                    }
                    button id="restart-button" class="ghosttyButton" type="button" {
                        "Restart"
                    }
                }
            }

            section id="viewport" class="ghosttyViewport" tabindex="0" aria-label="Ghostty terminal" {
                style id="ghostty-theme" {}
                div id="buffer" class="ghosttyBuffer" {}
            }

            footer class="ghosttyStatusbar" {
                div class="ghosttyStatusline" {
                    span id="status-pill" class="ghosttyPill" data-tone="busy" { "Booting" }
                    span id="session-meta" class="ghosttyMeta" { "Starting shell" }
                }

                div class="ghosttyStatusline ghosttyStatusline--right" {
                    a class="ghosttyLink" href="vendor/UPSTREAM.txt" target="_blank" rel="noreferrer" {
                        "Upstream"
                    }
                    a class="ghosttyLink" href="vendor/LICENSE.txt" target="_blank" rel="noreferrer" {
                        "MIT License"
                    }
                }
            }

            div id="measure" class="ghosttyMeasure" aria-hidden="true" {
                span id="measure-width" { "MMMMMMMMMM" }
                span id="measure-height" { "M" }
            }
        }
    }
}
