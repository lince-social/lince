use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

const DWASM_URL: &str = "https://dwasm.m-h.org.uk/";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "freedoom-portal.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▓".into(),
            title: "Freedoom Portal".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Free-to-use Doom-compatible launcher built around the Dwasm browser port and Freedoom content.".into(),
            details: "Launches the public Dwasm web port, which offers free-to-use Freedoom Phase 1 and Phase 2 content. If the remote page blocks embedding, the widget falls back to opening the portal in a new tab.".into(),
            initial_width: 6,
            initial_height: 6,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![style()],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script())],
    }
}

fn body() -> Markup {
    html! {
        div id="app" class="app" {
            header class="hero panel" {
                div class="eyebrow" { "free to use doom" }
                h1 class="title" { "Freedoom Portal" }
                p class="copy" {
                    "This widget launches Dwasm. Inside it, pick Freedoom Phase 1 or Phase 2, confirm the age gate, and start."
                }
            }

            section class="panel toolbar" {
                button id="launch-button" class="button button--primary" type="button" { "Launch Freedoom" }
                a id="open-link" class="button button--link" href=(DWASM_URL) target="_blank" rel="noreferrer noopener" { "Open in new tab" }
            }

            section class="panel notes" {
                p class="note" {
                    strong { "Controls: " }
                    "WASD move, mouse fire, Space use, Tab map, Esc menu."
                }
                p class="note" {
                    strong { "Fallback: " }
                    "If the embedded portal stays blank, use the new-tab button. Some remote sites block framing."
                }
            }

            section class="panel stage" {
                div id="frame-empty" class="empty" {
                    div class="emptyTitle" { "Freedoom is not running yet" }
                    div class="emptyCopy" { "Launch the portal to play a free Doom-compatible campaign in the browser." }
                }
                iframe id="doom-frame" class="frame" title="Freedoom via Dwasm" loading="lazy" allowfullscreen="" hidden="" {}
                div id="frame-hint" class="hint" hidden="" {
                    "If the remote portal does not appear, open it in a new tab."
                }
            }
        }
    }
}

fn style() -> &'static str {
    r#"
      :root {
        color-scheme: dark;
        --bg: #07090b;
        --panel: rgba(16, 20, 24, 0.96);
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.18);
        --text: #edf3f8;
        --muted: #95a4b2;
        --accent: #ff6f4b;
        --accent-soft: rgba(255, 111, 75, 0.15);
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
      }

      * { box-sizing: border-box; }

      html, body {
        min-height: 100%;
        margin: 0;
        background: transparent;
      }

      body {
        min-height: 100vh;
        padding: 12px;
        color: var(--text);
        background:
          radial-gradient(circle at top center, rgba(255, 111, 75, 0.16), transparent 24%),
          linear-gradient(180deg, rgba(6, 7, 9, 0.98), rgba(9, 11, 14, 0.98));
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      .app {
        min-height: calc(100vh - 24px);
        display: grid;
        grid-template-rows: auto auto auto minmax(0, 1fr);
        gap: 12px;
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 18px;
        background: linear-gradient(180deg, var(--panel), rgba(12, 15, 18, 0.96));
      }

      .hero, .toolbar, .notes, .stage {
        padding: 14px;
      }

      .eyebrow {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.67rem;
        font-weight: 700;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .title {
        margin: 6px 0 0;
        font-size: 1.08rem;
        letter-spacing: -0.03em;
      }

      .copy, .note, .emptyCopy, .hint {
        color: var(--muted);
        font-size: 0.79rem;
        line-height: 1.5;
      }

      .toolbar {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .button {
        min-height: 40px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0 14px;
        border-radius: 12px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.03);
        color: inherit;
        text-decoration: none;
        cursor: pointer;
      }

      .button:hover {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.05);
      }

      .button--primary {
        color: #ffd9cf;
        border-color: rgba(255, 111, 75, 0.28);
        background: var(--accent-soft);
        font-weight: 700;
      }

      .stage {
        min-height: 0;
        display: grid;
        grid-template-rows: minmax(0, 1fr) auto;
        gap: 10px;
      }

      .empty,
      .frame {
        min-height: 100%;
        width: 100%;
        border-radius: 16px;
        border: 1px solid var(--line);
        background: rgba(0, 0, 0, 0.28);
      }

      .empty {
        display: grid;
        align-content: center;
        justify-items: center;
        gap: 8px;
        padding: 20px;
        text-align: center;
      }

      .emptyTitle {
        font-size: 0.92rem;
        font-weight: 700;
      }

      .frame {
        min-height: 320px;
      }
    "#
}

fn script() -> &'static str {
    r#"
      (() => {
        const launchButton = document.getElementById("launch-button");
        const frame = document.getElementById("doom-frame");
        const empty = document.getElementById("frame-empty");
        const hint = document.getElementById("frame-hint");
        let hintTimer = null;

        function launch() {
          frame.hidden = false;
          empty.hidden = true;
          hint.hidden = true;
          frame.src = "https://dwasm.m-h.org.uk/";
          if (hintTimer) {
            clearTimeout(hintTimer);
          }
          hintTimer = window.setTimeout(() => {
            hint.hidden = false;
          }, 5000);
        }

        launchButton.addEventListener("click", launch);
      })();
    "#
}
