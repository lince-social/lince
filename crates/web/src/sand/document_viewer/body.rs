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
                        input id="file-input" class="fileInput" type="file" accept="application/pdf,application/epub+zip,.epub,image/png,image/jpeg" hidden="";
                    }
                    div id="view-mode-row" class="fieldRow fieldRow--mode" {
                        select id="pdf-mode-select" class="field field--select" {
                            option value="scroll" { "Infinite scroll" }
                            option value="page" { "Paging" }
                        }
                        input id="pdf-page-input" class="field pageField" type="number" min="1" step="1" inputmode="numeric" value="1";
                    }
                    div id="epub-controls" class="fieldRow fieldRow--epub" hidden="" {
                        button id="epub-prev" class="button" type="button" title="Previous EPUB section" { "Prev" }
                        button id="epub-next" class="button" type="button" title="Next EPUB section" { "Next" }
                    }
                    div class="licenseRow" {
                        span { "Reader libraries:" }
                        a href="vendor/EPUBJS-LICENSE.txt" target="_blank" rel="noreferrer" { "epub.js BSD" }
                        a href="vendor/JSZIP-LICENSE.txt" target="_blank" rel="noreferrer" { "JSZip MIT" }
                    }
                }
                pre id="debug" class="debug" {}
            }

            main class="documentArea" aria-label="Document preview" {
                div id="frame" class="frame" {
                    img id="image" class="image" alt="Document preview" hidden="";
                    iframe id="pdf-frame" class="pdfFrame" title="PDF preview" hidden="" {}
                    div id="epub-viewer" class="epubViewer" hidden="" {}
                    div id="nav-hit" class="navHit" aria-hidden="true" hidden="" {
                        button id="nav-prev" class="navZone navZone--prev" type="button" title="Previous" {}
                        button id="nav-next" class="navZone navZone--next" type="button" title="Next" {}
                    }
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
