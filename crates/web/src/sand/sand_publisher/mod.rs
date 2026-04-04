use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "sand-publisher.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "⬆".into(),
            title: "Sand Publisher".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description:
                "Publishes a local sand into the configured organ bucket and creates the record-focused DNA rows."
                    .into(),
            details: "The card configuration chooses the organ. This widget previews a .html/.sand/.lince package locally, uploads the canonical artifact and sand.toml into lince/dna/sand/{channel}/{aa}/{slug}/..., and then creates record, record_extension(namespace=lince.dna), and record_resource_ref(provider=bucket, resource_kind=sand).".into(),
            initial_width: 6,
            initial_height: 6,
            permissions: vec!["bridge_state".into()],
        },
        head_links: vec![],
        inline_styles: vec![style()],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script())],
    }
}

fn body() -> Markup {
    html! {
        div id="app" class="app" data-lince-bridge-root {
            header class="hero panel" {
                div class="heroCopy" {
                    div class="eyebrow" { "DNA publish" }
                    h1 class="title" { "Sand Publisher" }
                    p class="copy" {
                        "Use the card config to point this widget at an organ, then publish a local sand into that organ's bucket and DB."
                    }
                }
                div class="heroMeta" {
                    div id="server-pill" class="pill" { "server unset" }
                    div id="status-pill" class="pill pill--status" data-tone="idle" { "Waiting for config" }
                    button id="auth-button" class="button" type="button" hidden="" { "Authenticate organ" }
                }
            }

            section class="panel uploadPanel" {
                div class="sectionHead" {
                    div {
                        div class="eyebrow" { "Package" }
                        h2 class="sectionTitle" { "Preview before publish" }
                    }
                    div id="package-kind-pill" class="pill" { "no package" }
                }
                label class="fieldLabel" for="file-input" { "Sand file" }
                input id="file-input" class="fileInput" type="file" accept=".html,.sand,.lince";
                p class="hint" {
                    "Accepted formats: .html, .sand, .lince. The host validates and normalizes the canonical transport."
                }
                div id="preview-grid" class="previewGrid" {
                    div class="metric" {
                        span class="metricLabel" { "title" }
                        strong id="preview-title" { "unset" }
                    }
                    div class="metric" {
                        span class="metricLabel" { "version" }
                        strong id="preview-version" { "unset" }
                    }
                    div class="metric" {
                        span class="metricLabel" { "author" }
                        strong id="preview-author" { "unset" }
                    }
                    div class="metric" {
                        span class="metricLabel" { "size" }
                        strong id="preview-size" { "0 bytes" }
                    }
                }
                div id="preview-copy" class="previewCopy" {
                    "Choose a sand file to inspect its embedded manifest."
                }
            }

            section class="panel formPanel" {
                div class="sectionHead" {
                    div {
                        div class="eyebrow" { "Publication" }
                        h2 class="sectionTitle" { "Record-first contract" }
                    }
                    div id="channel-pill" class="pill" { "official" }
                }

                div class="fieldGroup" {
                    label class="fieldLabel" for="channel-select" { "Channel" }
                    select id="channel-select" class="field" {
                        option value="official" { "official" }
                        option value="community" { "community" }
                    }
                }

                div class="fieldGroup" {
                    label class="fieldLabel" for="head-input" { "record.head" }
                    input id="head-input" class="field" type="text" maxlength="160" placeholder="Visible sand name";
                }

                div class="fieldGroup" {
                    label class="fieldLabel" for="body-input" { "record.body" }
                    textarea id="body-input" class="field field--textarea" rows="4" maxlength="600" placeholder="Short description shown in the DNA catalog" {}
                }

                div class="warning" id="channel-warning" {
                    "Official sand is visible by default. Community sand stays unsafe by default and should require explicit user confirmation before consumption."
                }

                div id="bucket-preview" class="bucketPreview" {
                    "Bucket path preview appears after you choose a package."
                }

                div class="actions" {
                    button id="publish-button" class="button button--primary" type="button" { "Publish sand" }
                    button id="refresh-button" class="button" type="button" { "Refresh organ" }
                }
            }

            section class="panel resultPanel" {
                div class="sectionHead" {
                    div {
                        div class="eyebrow" { "Result" }
                        h2 class="sectionTitle" { "Latest publication" }
                    }
                    div id="result-tone" class="pill" { "idle" }
                }
                pre id="result-output" class="resultOutput" { "No publication yet." }
            }
        }
    }
}

fn style() -> &'static str {
    r#"
      :root {
        color-scheme: dark;
        --bg: #0b1013;
        --panel: rgba(14, 19, 24, 0.96);
        --panel-soft: rgba(18, 24, 31, 0.94);
        --line: rgba(255, 255, 255, 0.09);
        --line-strong: rgba(255, 255, 255, 0.18);
        --text: #edf3f9;
        --muted: #97a5b6;
        --accent: #8ee0bf;
        --accent-soft: rgba(142, 224, 191, 0.16);
        --danger: #ff9ba6;
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
          radial-gradient(circle at top left, rgba(142, 224, 191, 0.09), transparent 26%),
          linear-gradient(180deg, rgba(8, 12, 15, 0.98), rgba(10, 14, 18, 0.98));
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      button, input, textarea, select {
        font: inherit;
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
        background: linear-gradient(180deg, var(--panel), var(--panel-soft));
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
      }

      .hero,
      .uploadPanel,
      .formPanel,
      .resultPanel {
        padding: 14px;
      }

      .hero {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
      }

      .heroMeta {
        display: grid;
        justify-items: end;
        gap: 8px;
      }

      .eyebrow,
      .fieldLabel,
      .metricLabel {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.67rem;
        font-weight: 700;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .title,
      .sectionTitle {
        margin: 4px 0 0;
        font-size: 1rem;
        letter-spacing: -0.03em;
      }

      .copy,
      .hint,
      .previewCopy,
      .warning {
        color: var(--muted);
        font-size: 0.78rem;
        line-height: 1.45;
      }

      .sectionHead {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 10px;
        margin-bottom: 12px;
      }

      .pill,
      .button,
      .field,
      .fileInput {
        min-height: 38px;
        border-radius: 12px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.03);
        color: inherit;
      }

      .pill {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0 12px;
        font-family: var(--mono);
        font-size: 0.74rem;
      }

      .pill--status[data-tone="live"] {
        color: var(--accent);
        border-color: rgba(142, 224, 191, 0.24);
      }

      .pill--status[data-tone="error"] {
        color: var(--danger);
        border-color: rgba(255, 155, 166, 0.24);
      }

      .button {
        padding: 0 12px;
        cursor: pointer;
      }

      .button:hover {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.05);
      }

      .button--primary {
        color: var(--accent);
        border-color: rgba(142, 224, 191, 0.28);
        background: var(--accent-soft);
        font-weight: 700;
      }

      .button[disabled] {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .fileInput,
      .field {
        width: 100%;
        padding: 10px 12px;
      }

      .field--textarea {
        min-height: 112px;
        resize: vertical;
      }

      .fieldGroup {
        display: grid;
        gap: 6px;
        margin-bottom: 12px;
      }

      .previewGrid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 8px;
        margin: 12px 0 8px;
      }

      .metric {
        display: grid;
        gap: 4px;
        padding: 10px 12px;
        border-radius: 14px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.02);
      }

      .metric strong {
        font-size: 0.86rem;
      }

      .warning,
      .bucketPreview,
      .resultOutput {
        padding: 10px 12px;
        border-radius: 14px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.02);
      }

      .warning {
        margin-bottom: 12px;
      }

      .bucketPreview {
        margin-bottom: 12px;
        font-family: var(--mono);
        font-size: 0.75rem;
        white-space: pre-wrap;
        word-break: break-word;
      }

      .actions {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .resultPanel {
        min-height: 0;
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
      }

      .resultOutput {
        margin: 0;
        min-height: 0;
        overflow: auto;
        color: var(--text);
        line-height: 1.45;
      }

      @media (max-width: 720px) {
        .hero,
        .sectionHead {
          display: grid;
        }

        .heroMeta {
          justify-items: start;
        }

        .previewGrid {
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }
      }
    "#
}

fn script() -> &'static str {
    r#"
      (() => {
        const bridge = window.LinceWidgetHost || null;
        const app = document.getElementById("app");
        const frame = window.frameElement || null;
        const serverPill = document.getElementById("server-pill");
        const statusPill = document.getElementById("status-pill");
        const authButton = document.getElementById("auth-button");
        const fileInput = document.getElementById("file-input");
        const channelSelect = document.getElementById("channel-select");
        const headInput = document.getElementById("head-input");
        const bodyInput = document.getElementById("body-input");
        const publishButton = document.getElementById("publish-button");
        const refreshButton = document.getElementById("refresh-button");
        const channelPill = document.getElementById("channel-pill");
        const packageKindPill = document.getElementById("package-kind-pill");
        const previewTitle = document.getElementById("preview-title");
        const previewVersion = document.getElementById("preview-version");
        const previewAuthor = document.getElementById("preview-author");
        const previewSize = document.getElementById("preview-size");
        const previewCopy = document.getElementById("preview-copy");
        const bucketPreview = document.getElementById("bucket-preview");
        const resultTone = document.getElementById("result-tone");
        const resultOutput = document.getElementById("result-output");

        const state = {
          serverId: String(frame?.dataset?.linceServerId || ""),
          servers: [],
          selectedServer: null,
          upload: null,
          preview: null,
          channel: "official",
          published: null,
          busy: false,
        };

        function escapeHtml(value) {
          return String(value || "").replace(/[&<>\"']/g, (char) => {
            switch (char) {
              case "&": return "&amp;";
              case "<": return "&lt;";
              case ">": return "&gt;";
              case "\"": return "&quot;";
              case "'": return "&#39;";
              default: return char;
            }
          });
        }

        function normalizeSnake(value) {
          let output = "";
          let lastWasSeparator = false;
          for (const char of String(value || "").trim()) {
            const normalized = /[a-z0-9]/.test(char)
              ? char
              : /[A-Z]/.test(char)
                ? char.toLowerCase()
                : /[\s_-]/.test(char)
                  ? "_"
                  : "";
            if (!normalized) {
              continue;
            }
            if (normalized === "_") {
              if (!output || lastWasSeparator) {
                continue;
              }
              lastWasSeparator = true;
            } else {
              lastWasSeparator = false;
            }
            output += normalized;
          }
          output = output.replace(/^_+|_+$/g, "");
          return output || "lince_sand";
        }

        function packageSeed() {
          const filename = String(state.upload?.name || "").replace(/\.(html|sand|lince)$/i, "");
          if (filename && filename !== "index" && filename !== "widget") {
            return filename;
          }
          return state.preview?.title || headInput.value || "lince_sand";
        }

        function packageSlug() {
          return normalizeSnake(packageSeed());
        }

        function packagePrefixLetters(slug) {
          const compact = String(slug || "").replace(/[^a-z0-9]/g, "");
          const first = compact[0] || "x";
          const second = compact[1] || first;
          return first + second;
        }

        function transportFilename() {
          const slug = packageSlug();
          return state.preview?.filename?.toLowerCase().endsWith(".lince")
            ? `${slug}.lince`
            : `${slug}_metadata.html`;
        }

        function predictedBucketPath() {
          if (!state.preview) {
            return "";
          }
          const slug = packageSlug();
          return `lince/dna/sand/${state.channel}/${packagePrefixLetters(slug)}/${slug}/${transportFilename()}`;
        }

        function setStatus(text, tone = "idle") {
          statusPill.textContent = text;
          statusPill.dataset.tone = tone;
        }

        function setBusy(nextBusy) {
          state.busy = Boolean(nextBusy);
          publishButton.disabled = state.busy;
          refreshButton.disabled = state.busy;
          authButton.disabled = state.busy;
          fileInput.disabled = state.busy;
          channelSelect.disabled = state.busy;
          headInput.disabled = state.busy;
          bodyInput.disabled = state.busy;
        }

        async function parseJsonResponse(response) {
          return response.json().catch(() => null);
        }

        async function refreshServers() {
          const response = await fetch("/host/servers");
          const payload = await parseJsonResponse(response);
          if (!response.ok) {
            throw new Error(payload?.error || "Falha ao carregar os organs.");
          }
          state.servers = Array.isArray(payload) ? payload : [];
          state.selectedServer =
            state.servers.find((server) => server.id === state.serverId) || null;
          renderServer();
        }

        function renderServer() {
          const server = state.selectedServer;
          serverPill.textContent = server
            ? `${server.name} · ${server.id}`
            : state.serverId
              ? `server: ${state.serverId}`
              : "server unset";
          authButton.hidden = !server?.requiresAuth || Boolean(server?.authenticated);
          if (!state.serverId) {
            setStatus("Choose a server in the card config first", "idle");
            return;
          }
          if (server?.requiresAuth && !server?.authenticated) {
            setStatus("Remote organ locked", "error");
            return;
          }
          setStatus("Ready to publish", state.preview ? "live" : "idle");
        }

        function renderPreview() {
          channelPill.textContent = state.channel;
          packageKindPill.textContent = state.preview
            ? state.preview.filename.toLowerCase().endsWith(".lince")
              ? ".lince archive"
              : "html transport"
            : "no package";
          previewTitle.textContent = state.preview?.title || "unset";
          previewVersion.textContent = state.preview?.version || "unset";
          previewAuthor.textContent = state.preview?.author || "unset";
          previewSize.textContent = state.upload
            ? `${Number(state.upload.size || 0).toLocaleString()} bytes`
            : "0 bytes";
          previewCopy.textContent = state.preview
            ? state.preview.description || "Manifest loaded."
            : "Choose a sand file to inspect its embedded manifest.";
          bucketPreview.textContent = state.preview
            ? `bucket_key = ${predictedBucketPath()}\npackage_prefix = ${predictedBucketPath().replace(/[^/]+$/, "")}`
            : "Bucket path preview appears after you choose a package.";
          renderPublishButton();
        }

        function renderPublishButton() {
          publishButton.disabled =
            state.busy ||
            !state.serverId ||
            !state.upload ||
            !state.preview ||
            !headInput.value.trim() ||
            !bodyInput.value.trim();
        }

        function persistDraft() {
          bridge?.patchCardState?.({
            dnaPublisher: {
              channel: state.channel,
              head: headInput.value.trim(),
              body: bodyInput.value.trim(),
              published: state.published,
            },
          });
        }

        async function previewPackage(file) {
          const formData = new FormData();
          formData.append("file", file, file.name);
          const response = await fetch("/host/packages/preview", {
            method: "POST",
            body: formData,
          });
          const payload = await parseJsonResponse(response);
          if (!response.ok) {
            throw new Error(payload?.error || "Falha ao ler o manifesto do sand.");
          }
          return payload;
        }

        async function handleFileChange() {
          const file = fileInput.files?.[0] || null;
          state.upload = file;
          state.preview = null;
          renderPreview();

          if (!file) {
            return;
          }

          setBusy(true);
          setStatus("Previewing package", "idle");

          try {
            const preview = await previewPackage(file);
            state.preview = preview;
            if (!headInput.value.trim()) {
              headInput.value = String(preview?.title || "").trim();
            }
            if (!bodyInput.value.trim()) {
              bodyInput.value = String(preview?.description || "").trim();
            }
            renderPreview();
            setStatus("Package ready", "live");
            persistDraft();
          } catch (error) {
            state.preview = null;
            resultTone.textContent = "error";
            resultOutput.textContent =
              error instanceof Error ? error.message : "Falha ao ler o sand.";
            setStatus("Preview failed", "error");
          } finally {
            setBusy(false);
            renderPublishButton();
          }
        }

        function renderPublishedResult(payload) {
          resultTone.textContent = payload ? "published" : "idle";
          resultOutput.innerHTML = payload
            ? [
                `organ_id = ${escapeHtml(payload.organId)}`,
                `record_id = ${escapeHtml(payload.recordId)}`,
                `slug = ${escapeHtml(payload.slug)}`,
                `channel = ${escapeHtml(payload.channel)}`,
                `bucket_key = ${escapeHtml(payload.bucketKey)}`,
                `sand_toml_key = ${escapeHtml(payload.sandTomlKey)}`,
              ].join("\n")
            : "No publication yet.";
        }

        async function publishPackage() {
          if (!state.upload || !state.preview) {
            return;
          }
          if (!state.serverId) {
            setStatus("Choose a server in the card config first", "error");
            return;
          }

          setBusy(true);
          setStatus("Publishing sand", "idle");
          const formData = new FormData();
          formData.append("serverId", state.serverId);
          formData.append("channel", state.channel);
          formData.append("head", headInput.value.trim());
          formData.append("body", bodyInput.value.trim());
          formData.append("file", state.upload, state.upload.name);

          try {
            const response = await fetch("/host/packages/dna/publish", {
              method: "POST",
              body: formData,
            });
            const payload = await parseJsonResponse(response);
            if (response.status === 401) {
              bridge?.invalidateServerAuth?.(state.serverId);
              throw new Error(
                payload?.error || "Authenticate this organ before publishing.",
              );
            }
            if (!response.ok) {
              throw new Error(payload?.error || "Falha ao publicar o sand.");
            }

            state.published = payload;
            renderPublishedResult(payload);
            setStatus("Sand published", "live");
            persistDraft();
          } catch (error) {
            renderPublishedResult(null);
            resultTone.textContent = "error";
            resultOutput.textContent =
              error instanceof Error ? error.message : "Falha ao publicar o sand.";
            setStatus("Publish failed", "error");
          } finally {
            setBusy(false);
            renderPublishButton();
          }
        }

        function applyBridge(detail) {
          const meta = detail?.meta && typeof detail.meta === "object" ? detail.meta : {};
          const cardState = meta.cardState && typeof meta.cardState === "object" ? meta.cardState : {};
          const draft = cardState.dnaPublisher && typeof cardState.dnaPublisher === "object"
            ? cardState.dnaPublisher
            : {};
          state.serverId = String(meta.serverId || frame?.dataset?.linceServerId || "").trim();
          state.channel = draft.channel === "community" ? "community" : "official";
          channelSelect.value = state.channel;
          channelPill.textContent = state.channel;
          headInput.value = draft.head || headInput.value;
          bodyInput.value = draft.body || bodyInput.value;
          state.published = draft.published || null;
          renderPublishedResult(state.published);
          void refreshServers().catch((error) => {
            setStatus("Organ refresh failed", "error");
            resultTone.textContent = "error";
            resultOutput.textContent =
              error instanceof Error ? error.message : "Falha ao carregar os organs.";
          });
          renderPreview();
        }

        authButton.addEventListener("click", () => {
          if (state.serverId) {
            bridge?.invalidateServerAuth?.(state.serverId);
          }
        });

        refreshButton.addEventListener("click", () => {
          void refreshServers().catch((error) => {
            setStatus("Organ refresh failed", "error");
            resultTone.textContent = "error";
            resultOutput.textContent =
              error instanceof Error ? error.message : "Falha ao carregar os organs.";
          });
        });

        fileInput.addEventListener("change", () => {
          void handleFileChange();
        });

        channelSelect.addEventListener("change", () => {
          state.channel = channelSelect.value === "community" ? "community" : "official";
          renderPreview();
          persistDraft();
        });

        headInput.addEventListener("input", () => {
          renderPreview();
          persistDraft();
        });

        bodyInput.addEventListener("input", () => {
          renderPublishButton();
          persistDraft();
        });

        publishButton.addEventListener("click", () => {
          void publishPackage();
        });

        app.addEventListener("lince-bridge-state", (event) => {
          applyBridge(event.detail || {});
        });

        renderPublishedResult(null);
        renderPreview();
        renderServer();
        bridge?.requestState?.();
      })();
    "#
}
