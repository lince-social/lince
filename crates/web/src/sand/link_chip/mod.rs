use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "sand.link_chip";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"link-chip.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"↗"#.into(),
            title: r#"Link Chip"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Card horizontal de link com texto customizado e favicon automatico."#.into(),
            details: r#"Widget minimalista para links: mostra label customizada, favicon do dominio e um editor compacto para trocar URL e texto."#.into(),
            initial_width: 4,
            initial_height: 2,
            requires_server: false,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        --text: #f3f4f6;
        --muted: rgba(209, 213, 219, 0.68);
        --line: rgba(255, 255, 255, 0.12);
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        margin: 0;
        min-height: 100%;
        background: transparent;
      }

      body {
        min-height: 100vh;
        padding: 10px 12px;
        color: var(--text);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      .app {
        position: relative;
        display: grid;
        min-height: calc(100vh - 20px);
        align-items: center;
      }

      .toolbar {
        position: absolute;
        top: 0;
        right: 0;
        z-index: 3;
      }

      .edit-toggle {
        padding: 0;
        border: 0;
        color: var(--muted);
        background: transparent;
        font-size: 11px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        cursor: pointer;
      }

      .edit-toggle:hover {
        color: var(--text);
      }

      .link-row {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr);
        align-items: center;
        gap: 18px;
        min-height: 92px;
        padding-top: 10px;
        color: inherit;
        text-decoration: none;
      }

      .favicon {
        width: 56px;
        height: 56px;
        border-radius: 12px;
        object-fit: contain;
        background: rgba(255, 255, 255, 0.04);
      }

      .label {
        min-width: 0;
        font-size: 1.95rem;
        font-weight: 600;
        line-height: 1.05;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .host {
        margin-top: 7px;
        color: var(--muted);
        font-size: 1.2rem;
        line-height: 1.2;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .editor {
        position: absolute;
        inset: 28px 0 0;
        display: grid;
        gap: 8px;
        align-content: start;
        padding-top: 8px;
      }

      .field {
        width: 100%;
        min-height: 40px;
        padding: 0 11px;
        border: 1px solid var(--line);
        border-radius: 10px;
        color: var(--text);
        background: rgba(255, 255, 255, 0.03);
        outline: none;
      }

      .field:focus {
        border-color: rgba(255, 255, 255, 0.22);
      }

      .actions {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 10px;
      }

      .hint {
        color: var(--muted);
        font-size: 0.76rem;
        line-height: 1.35;
      }

      .buttons {
        display: flex;
        gap: 8px;
      }

      .button {
        min-height: 32px;
        padding: 0 10px;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 9px;
        color: var(--text);
        background: rgba(255, 255, 255, 0.05);
        cursor: pointer;
      }

      .button--ghost {
        border-color: transparent;
        color: var(--muted);
        background: transparent;
      }

      .button--ghost:hover {
        color: var(--text);
      }

      [hidden] {
        display: none !important;
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r#"      const editToggle = document.getElementById("edit-toggle");
      const linkRow = document.getElementById("link-row");
      const favicon = document.getElementById("favicon");
      const label = document.getElementById("label");
      const host = document.getElementById("host");
      const editor = document.getElementById("editor");
      const labelInput = document.getElementById("label-input");
      const urlInput = document.getElementById("url-input");
      const saveButton = document.getElementById("save-button");
      const cancelButton = document.getElementById("cancel-button");
      const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
      const stateKey = "link-chip/state/" + instanceId;

      const DEFAULT_STATE = {
        label: "Repositorio",
        url: "https://github.com/",
      };

      function storageArea() {
        try {
          if (window.parent && window.parent !== window && window.parent.localStorage) {
            return window.parent.localStorage;
          }
        } catch (error) {
          // fallback below
        }

        try {
          return window.localStorage;
        } catch (error) {
          return null;
        }
      }

      function readState() {
        const storage = storageArea();
        if (!storage) {
          return { ...DEFAULT_STATE };
        }

        try {
          const raw = storage.getItem(stateKey);
          if (!raw) {
            return { ...DEFAULT_STATE };
          }

          const parsed = JSON.parse(raw);
          return {
            label: String(parsed?.label || DEFAULT_STATE.label),
            url: String(parsed?.url || DEFAULT_STATE.url),
          };
        } catch (error) {
          return { ...DEFAULT_STATE };
        }
      }

      function writeState(nextState) {
        const storage = storageArea();
        if (!storage) {
          return false;
        }

        try {
          storage.setItem(stateKey, JSON.stringify(nextState));
          return true;
        } catch (error) {
          return false;
        }
      }

      function normalizeUrl(raw) {
        const value = String(raw || "").trim();
        if (!value) {
          return DEFAULT_STATE.url;
        }

        if (/^https?:\/\//i.test(value)) {
          return value;
        }

        return "https://" + value;
      }

      function faviconFor(url) {
        try {
          const parsed = new URL(url);
          return "https://www.google.com/s2/favicons?domain=" + encodeURIComponent(parsed.host) + "&sz=128";
        } catch (error) {
          return "https://www.google.com/s2/favicons?domain=github.com&sz=128";
        }
      }

      function hostFor(url) {
        try {
          return new URL(url).host;
        } catch (error) {
          return "";
        }
      }

      function applyState(nextState) {
        label.textContent = nextState.label || DEFAULT_STATE.label;
        host.textContent = hostFor(nextState.url);
        linkRow.href = nextState.url;
        favicon.src = faviconFor(nextState.url);
        favicon.alt = "";
        labelInput.value = nextState.label;
        urlInput.value = nextState.url;
      }

      function setEditing(editing) {
        editor.hidden = !editing;
        linkRow.hidden = editing;
        editToggle.textContent = editing ? "Fechar" : "Editar";

        if (editing) {
          labelInput.focus();
          labelInput.select();
        }
      }

      let state = readState();
      applyState(state);
      setEditing(false);

      favicon.addEventListener("error", () => {
        try {
          const parsed = new URL(state.url);
          favicon.src = parsed.origin + "/favicon.ico";
        } catch (error) {
          favicon.src = "https://www.google.com/s2/favicons?domain=github.com&sz=128";
        }
      });

      editToggle.addEventListener("click", () => {
        if (editor.hidden) {
          labelInput.value = state.label;
          urlInput.value = state.url;
          setEditing(true);
          return;
        }

        setEditing(false);
      });

      saveButton.addEventListener("click", () => {
        state = {
          label: String(labelInput.value || DEFAULT_STATE.label).trim() || DEFAULT_STATE.label,
          url: normalizeUrl(urlInput.value),
        };
        writeState(state);
        applyState(state);
        setEditing(false);
      });

      cancelButton.addEventListener("click", () => {
        labelInput.value = state.label;
        urlInput.value = state.url;
        setEditing(false);
      });
    "#)],
    }
}

fn body() -> Markup {
    let editor_fields = [
        ("label-input", "text", "Texto do link"),
        ("url-input", "url", "https://github.com/..."),
    ];
    let editor_actions = [
        ("cancel-button", "button button--ghost", "Cancelar"),
        ("save-button", "button", "Salvar"),
    ];

    html! {
        main class="app" {
            div class="toolbar" {
                button id="edit-toggle" class="edit-toggle" type="button" { "Editar" }
            }
            a id="link-row" class="link-row" href="#" target="_blank" rel="noreferrer" {
                img id="favicon" class="favicon" alt="";
                div {
                    div id="label" class="label" { "Repositorio" }
                    div id="host" class="host" { "github.com" }
                }
            }
            div id="editor" class="editor" hidden {
                @for (id, kind, placeholder) in editor_fields {
                    input id=(id) class="field" type=(kind) placeholder=(placeholder);
                }
                div class="actions" {
                    span class="hint" { "Salva por instancia e pega o favicon do dominio." }
                    div class="buttons" {
                        @for (id, class_name, label) in editor_actions {
                            button id=(id) class=(class_name) type="button" { (label) }
                        }
                    }
                }
            }
        }
    }
}
