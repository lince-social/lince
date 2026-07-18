use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="ghosttyTerminal" {
            section id="viewport" class="ghosttyViewport" tabindex="0" aria-label="Ghostty terminal" {
                style id="ghostty-theme" {}
                div id="buffer" class="ghosttyBuffer" {}
            }

            button
                id="connection-button"
                class="ghosttyConnection"
                type="button"
                data-tone="busy"
                aria-controls="info-panel"
                aria-expanded="false"
                aria-label="Open terminal controls; connection is starting"
                title="Terminal connection" {}

            aside id="info-panel" class="ghosttyPanel" aria-label="Terminal controls" hidden {
                div class="ghosttyPanelHeader" {
                    strong { "Ghostty VT" }
                    button id="close-panel-button" class="ghosttyIconButton" type="button" aria-label="Close terminal controls" title="Close" {
                        "X"
                    }
                }

                div class="ghosttyConnectionInfo" {
                    span id="panel-status-dot" class="ghosttyPanelDot" data-tone="busy" aria-hidden="true" {}
                    div {
                        div id="status-pill" class="ghosttyStatus" { "Booting" }
                        div id="session-meta" class="ghosttyMeta" { "Starting shell" }
                    }
                }

                p class="ghosttyLicense" {
                    "Terminal emulation is powered by libghostty-vt and distributed under the MIT License."
                }
                nav class="ghosttyLinks" aria-label="Ghostty notices" {
                    a href="vendor/UPSTREAM.txt" target="_blank" rel="noreferrer" { "Upstream" }
                    a href="vendor/LICENSE.txt" target="_blank" rel="noreferrer" { "MIT License" }
                }

                div class="ghosttyActions" {
                    button id="interrupt-button" class="ghosttyButton ghosttyButton--danger" type="button" { "Ctrl+C" }
                    button id="restart-button" class="ghosttyButton" type="button" { "Restart shell" }
                }
            }

            div id="measure" class="ghosttyMeasure" aria-hidden="true" {
                span id="measure-width" { "MMMMMMMMMM" }
                span id="measure-height" { "M" }
            }
        }
    }
}
