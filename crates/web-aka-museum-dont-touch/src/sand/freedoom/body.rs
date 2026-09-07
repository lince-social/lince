use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        main class="app" {
            header class="topbar" {
                div class="identity" {
                    div class="eyebrow" { "Local game" }
                    h1 { "Freedoom Portal" }
                    p { "Bundled wasm engine and Freedoom Phase 1 WAD." }
                }
                nav class="notices" aria-label="Package notices" {
                    a href="COPYING.txt" target="_blank" rel="noreferrer" { "License" }
                    a href="CREDITS.txt" target="_blank" rel="noreferrer" { "Credits" }
                }
                div class="controls" {
                    button id="launch-button" class="primary" type="button" disabled { "Launch" }
                    button id="fullscreen-button" type="button" disabled title="Fullscreen" aria-label="Fullscreen" { "Fullscreen" }
                    button id="reload-button" type="button" { "Reload" }
                    details {
                        summary { "Runtime log" }
                        pre id="runtime-log" aria-live="polite" { "Booting archive assets..." }
                    }
                }
            }
            section class="game" {
                div class="statusbar" {
                    div class="statusmain" {
                        span id="status-dot" class="statusdot" {}
                        span id="status-text" { "Loading the local Freedoom engine..." }
                    }
                    span class="hint" { "Click the canvas to capture the pointer." }
                }
                div id="canvas-shell" class="canvas-shell" {
                    canvas id="canvas" tabindex="-1" {}
                    div id="placeholder" class="placeholder" {
                        strong { "Ready for local play" }
                        span { "Launch starts the runtime directly from this archive." }
                    }
                }
            }
        }
    }
}
