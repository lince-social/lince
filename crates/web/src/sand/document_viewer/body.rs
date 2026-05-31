use maud::{Markup, html};

pub(super) fn body() -> Markup {
    html! {
        div id="app" class="app" data-lince-bridge-root {
            button id="config-toggle" class="configToggle" type="button" aria-expanded="true" aria-controls="config-panel" title="Document settings" { "" }

            aside id="config-panel" class="configPanel" {
                div class="controls" {
                    div class="fieldRow fieldRow--source" {
                        select id="source-select" class="field field--select" {
                            option value="local" { "Local path" }
                            option value="bucket" { "Lince bucket" }
                            option value="url" { "Internet URL" }
                        }
                        div class="actionRow" {
                            button id="load-button" class="button button--primary" type="button" { "Load" }
                            button id="unload-button" class="button" type="button" { "Unload" }
                        }
                    }
                    div class="fieldRow fieldRow--path" {
                        input id="path-input" class="field" type="text" inputmode="text" autocomplete="off" spellcheck="false" placeholder="Path or URL";
                        button id="pick-button" class="button" type="button" { "Choose" }
                        input id="file-input" class="fileInput" type="file" accept="application/pdf,image/png,image/jpeg" hidden="";
                    }
                    div class="fieldRow fieldRow--pdf" {
                        select id="pdf-mode-select" class="field field--select" {
                            option value="scroll" { "Infinite scroll" }
                            option value="page" { "Page anchor" }
                        }
                        input id="pdf-page-input" class="field pageField" type="number" min="1" step="1" inputmode="numeric" value="1";
                    }
                }
                pre id="debug" class="debug" {}
            }

            main class="documentArea" aria-label="Document preview" {
                div id="frame" class="frame" {
                    img id="image" class="image" alt="Document preview" hidden="";
                    iframe id="pdf-frame" class="pdfFrame" title="PDF preview" hidden="" {}
                    div id="empty" class="empty" {
                        div class="emptyEyebrow" { "No document selected" }
                        div id="empty-title" class="emptyTitle" { "Choose a file or path" }
                        div id="empty-copy" class="emptyCopy" { "The settings panel stays visible so you can load a document right away." }
                    }
                }
            }
        }
    }
}
