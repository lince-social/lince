use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "bucket-image-view.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "◧".into(),
            title: "Bucket Image View".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Loads an image from the configured server bucket by relative path.".into(),
            details: "The card config chooses the organ/server, and the widget stores the bucket object path in the card state so it survives refreshes. It fetches the bytes through the host proxy and renders them as an image preview.".into(),
            initial_width: 4,
            initial_height: 4,
            requires_server: true,
            permissions: vec!["bridge_state".into(), "read_files".into()],
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
            header class="header" {
                div class="headerCopy" {
                    div class="eyebrow" { "Bucket image" }
                    h1 class="title" { "Bucket Image View" }
                    p class="copy" {
                        "Enter a relative object path. The card's server configuration decides which organ to read from."
                    }
                }
                div class="headerMeta" {
                    div id="server-pill" class="pill" { "Server unset" }
                    div id="status" class="status" { "Waiting for config" }
                }
            }

            section class="panel controls" {
                label class="fieldLabel" for="path-input" { "Object path" }
                div class="fieldRow" {
                    input id="path-input" class="field" type="text" inputmode="text" autocomplete="off" spellcheck="false" placeholder="folder/photo.png";
                    button id="load-button" class="button button--primary" type="button" { "Load" }
                }
                div class="hint" id="hint" {
                    "Use a path relative to the bucket root. Do not include the bucket name."
                }
            }

            section class="viewer panel" aria-label="Image preview" {
                div id="frame" class="frame" {
                    img id="image" class="image" alt="Bucket image preview" hidden="";
                    div id="empty" class="empty" {
                        div class="emptyEyebrow" { "Preview" }
                        div id="empty-title" class="emptyTitle" { "No image loaded" }
                        div id="empty-copy" class="emptyCopy" {
                            "Pick a path and load the file. If the object is an image, it will render here."
                        }
                    }
                }
            }

            footer class="footer" {
                div class="pill" id="path-pill" { "path: unset" }
                div class="pill" id="mime-pill" { "mime: unset" }
                div class="pill" id="size-pill" { "bytes: 0" }
                div class="pill" id="dim-pill" { "0 x 0" }
            }
        }
    }
}

fn style() -> &'static str {
    r#"
      :root {
        color-scheme: dark;
        --bg: #0d1014;
        --panel: #151a21;
        --panel-soft: #10151b;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.14);
        --text: #edf2f7;
        --muted: #93a0ae;
        --accent: #9cc2ff;
        --ok: #8df0b9;
        --warn: #f3c77b;
        --danger: #ff9ba9;
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        min-height: 100%;
        margin: 0;
        background: transparent;
      }

      body {
        min-height: 100vh;
        padding: 12px;
        color: var(--text);
        background: var(--bg);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      button,
      input {
        font: inherit;
      }

      .app {
        min-height: calc(100vh - 24px);
        display: grid;
        grid-template-rows: auto auto minmax(0, 1fr) auto;
        gap: 10px;
      }

      .header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 10px;
      }

      .headerCopy {
        min-width: 0;
      }

      .headerMeta {
        display: grid;
        gap: 8px;
        justify-items: end;
        flex: 0 0 auto;
      }

      .eyebrow,
      .fieldLabel,
      .emptyEyebrow {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.66rem;
        font-weight: 600;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .title {
        margin: 4px 0 0;
        font-size: 1rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .copy {
        margin: 6px 0 0;
        color: var(--muted);
        font-size: 0.78rem;
        line-height: 1.45;
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 16px;
        background: linear-gradient(180deg, rgba(21, 26, 33, 0.98), rgba(16, 20, 26, 0.98));
      }

      .controls {
        display: grid;
        gap: 8px;
        padding: 12px;
      }

      .fieldRow {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 8px;
      }

      .field,
      .button {
        min-height: 38px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panel-soft);
        color: var(--text);
      }

      .field {
        width: 100%;
        padding: 0 12px;
        outline: none;
      }

      .field:focus {
        border-color: rgba(156, 194, 255, 0.32);
      }

      .button {
        padding: 0 12px;
        cursor: pointer;
        transition: border-color 140ms ease, background 140ms ease;
      }

      .button:hover {
        background: rgba(255, 255, 255, 0.04);
        border-color: var(--line-strong);
      }

      .button--primary {
        border-color: rgba(156, 194, 255, 0.24);
        color: #dfeaff;
      }

      .hint {
        color: var(--muted);
        font-size: 0.74rem;
        line-height: 1.35;
      }

      .viewer {
        min-height: 0;
        padding: 10px;
      }

      .frame {
        position: relative;
        min-height: 100%;
        overflow: hidden;
        border-radius: 14px;
        border: 1px solid var(--line);
        background:
          radial-gradient(circle at top right, rgba(156, 194, 255, 0.06), transparent 26%),
          #0b0f13;
      }

      .image {
        display: block;
        width: 100%;
        height: 100%;
        object-fit: contain;
      }

      .empty {
        position: absolute;
        inset: 0;
        display: grid;
        align-content: center;
        justify-items: center;
        gap: 8px;
        padding: 18px;
        text-align: center;
      }

      .emptyTitle {
        font-size: 0.96rem;
        font-weight: 700;
        letter-spacing: -0.02em;
      }

      .emptyCopy {
        max-width: 32ch;
        color: var(--muted);
        font-size: 0.78rem;
        line-height: 1.45;
      }

      .footer {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .pill,
      .status {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        min-height: 32px;
        padding: 0 10px;
        border-radius: 999px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.03);
        color: var(--muted);
        font-size: 0.72rem;
        line-height: 1;
        white-space: nowrap;
      }

      .status {
        letter-spacing: 0.04em;
        text-transform: uppercase;
      }

      .status[data-tone="live"] {
        color: #daf7e6;
        border-color: rgba(141, 240, 185, 0.24);
        background: rgba(19, 46, 32, 0.72);
      }

      .status[data-tone="loading"] {
        color: #f7e6bf;
        border-color: rgba(243, 199, 123, 0.22);
        background: rgba(44, 34, 15, 0.72);
      }

      .status[data-tone="error"] {
        color: #ffd9df;
        border-color: rgba(255, 155, 169, 0.22);
        background: rgba(57, 21, 31, 0.72);
      }

      .status[data-tone="idle"] {
        color: var(--muted);
      }

      [hidden] {
        display: none !important;
      }
    "#
}

fn script() -> &'static str {
    r#"
      const app = document.getElementById("app");
      const frame = window.frameElement;
      const pathInput = document.getElementById("path-input");
      const loadButton = document.getElementById("load-button");
      const image = document.getElementById("image");
      const empty = document.getElementById("empty");
      const emptyTitle = document.getElementById("empty-title");
      const emptyCopy = document.getElementById("empty-copy");
      const status = document.getElementById("status");
      const serverPill = document.getElementById("server-pill");
      const pathPill = document.getElementById("path-pill");
      const mimePill = document.getElementById("mime-pill");
      const sizePill = document.getElementById("size-pill");
      const dimPill = document.getElementById("dim-pill");
      const hint = document.getElementById("hint");
      const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
      const stateKey = "bucket-image-view/" + instanceId;

      const bridge = window.LinceWidgetHost || null;
      const bridgeState = normalizeCardState(bridge?.getCardState?.() || null);
      const fallbackState = readFallbackState();
      const state = {
        serverId: String(frame?.dataset?.linceServerId || ""),
        path: normalizePath(bridgeState.path || fallbackState.path),
      };

      let loadToken = 0;
      let saveTimer = null;
      let currentObjectUrl = "";

      function storageArea() {
        try {
          if (window.parent && window.parent !== window && window.parent.localStorage) {
            return window.parent.localStorage;
          }
        } catch (error) {
        }

        try {
          return window.localStorage;
        } catch (error) {
          return null;
        }
      }

      function readFallbackState() {
        const storage = storageArea();
        if (!storage) {
          return { path: "" };
        }

        try {
          const raw = storage.getItem(stateKey);
          if (!raw) {
            return { path: "" };
          }

          const parsed = JSON.parse(raw);
          return {
            path: String(parsed?.path || ""),
          };
        } catch (error) {
          return { path: "" };
        }
      }

      function writeFallbackState(nextState) {
        const storage = storageArea();
        if (!storage) {
          return;
        }

        try {
          storage.setItem(stateKey, JSON.stringify(nextState));
        } catch (error) {
        }
      }

      function normalizePath(rawPath) {
        return String(rawPath || "").trim().replace(/^\/+/, "");
      }

      function normalizeCardState(rawCardState) {
        const value = rawCardState && typeof rawCardState === "object" ? rawCardState : {};
        return {
          path: normalizePath(value.path || value.objectPath || value.key || ""),
        };
      }

      function getCurrentUrl() {
        const serverId = String(state.serverId || "").trim();
        const path = normalizePath(state.path);

        if (!serverId || !path) {
          return "";
        }

        return (
          "/host/integrations/servers/" +
          encodeURIComponent(serverId) +
          "/files?path=" +
          encodeURIComponent(path)
        );
      }

      function setStatus(text, tone = "idle") {
        status.textContent = text;
        status.dataset.tone = tone;
      }

      function setEmpty(title, copy) {
        emptyTitle.textContent = title;
        emptyCopy.textContent = copy;
        empty.hidden = false;
        image.hidden = true;
      }

      function setMetaFromResponse(response, blob) {
        const mime = response.headers.get("content-type") || blob.type || "application/octet-stream";
        const bytes = Number(blob.size) || 0;
        mimePill.textContent = "mime: " + mime;
        sizePill.textContent = "bytes: " + bytes;
      }

      function setDimensions(width, height) {
        dimPill.textContent = width && height ? width + " x " + height : "0 x 0";
      }

      function clearObjectUrl() {
        if (currentObjectUrl) {
          URL.revokeObjectURL(currentObjectUrl);
          currentObjectUrl = "";
        }
      }

      function syncCardState(nextPath) {
        const normalized = normalizePath(nextPath);
        state.path = normalized;
        pathInput.value = normalized;
        pathPill.textContent = normalized ? "path: " + normalized : "path: unset";

        writeFallbackState({ path: normalized });

        if (bridge?.patchCardState) {
          bridge.patchCardState({ path: normalized });
          return;
        }
      }

      function schedulePersistPath() {
        if (saveTimer) {
          clearTimeout(saveTimer);
        }

        saveTimer = window.setTimeout(() => {
          saveTimer = null;
          syncCardState(pathInput.value);
        }, 180);
      }

      function updateServerPill() {
        const serverId = String(state.serverId || "").trim();
        serverPill.textContent = serverId ? "server: " + serverId : "server unset";
        hint.hidden = false;
      }

      function renderIdleState(message) {
        clearObjectUrl();
        mimePill.textContent = "mime: unset";
        sizePill.textContent = "bytes: 0";
        setDimensions(0, 0);
        setEmpty("No image loaded", message);
      }

      async function loadImage() {
        const serverId = String(state.serverId || "").trim();
        const path = normalizePath(state.path);

        if (!serverId) {
          setStatus("Waiting for server", "idle");
          updateServerPill();
          renderIdleState("Choose a server in the card configuration first.");
          return;
        }

        if (!path) {
          setStatus("Waiting for path", "idle");
          updateServerPill();
          renderIdleState("Enter a relative object path and press Load.");
          return;
        }

        const token = ++loadToken;
        setStatus("Loading file", "loading");
        updateServerPill();
        setEmpty("Loading image...", "Fetching bytes through the host proxy.");

        try {
          const response = await fetch(getCurrentUrl(), {
            headers: {
              Accept: "image/*,*/*;q=0.8",
            },
          });

          if (token !== loadToken) {
            return;
          }

          if (response.status === 401) {
            window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
            setStatus("Server locked", "error");
            renderIdleState("Authenticate the selected server in the host.");
            return;
          }

          if (!response.ok) {
            const body = await response.text().catch(() => "");
            throw new Error(body || "The host could not load that object.");
          }

          const blob = await response.blob();
          if (token !== loadToken) {
            return;
          }

          const nextUrl = URL.createObjectURL(blob);
          clearObjectUrl();
          currentObjectUrl = nextUrl;
          setMetaFromResponse(response, blob);

          image.onload = () => {
            if (token !== loadToken) {
              return;
            }

            empty.hidden = true;
            image.hidden = false;
            setDimensions(image.naturalWidth, image.naturalHeight);
            setStatus("Image loaded", "live");
          };

          image.onerror = () => {
            if (token !== loadToken) {
              return;
            }

            clearObjectUrl();
            setStatus("Not an image", "error");
            renderIdleState("The object bytes could not be decoded as an image.");
          };

          image.src = currentObjectUrl;
          image.alt = "Bucket image " + path;
        } catch (error) {
          if (token !== loadToken) {
            return;
          }

          clearObjectUrl();
          setStatus("Load failed", "error");
          renderIdleState(
            error instanceof Error ? error.message : "The host could not load that object.",
          );
        }
      }

      function applyBridgeDetail(detail) {
        const nextMeta = detail?.meta && typeof detail.meta === "object" ? detail.meta : {};
        const nextServerId = String(nextMeta.serverId || frame?.dataset?.linceServerId || "");
        const nextCardState = normalizeCardState(nextMeta.cardState);
        const nextPath = nextCardState.path || state.path;
        const previousServerId = state.serverId;
        const previousPath = state.path;

        state.serverId = nextServerId;
        state.path = nextPath;
        pathInput.value = nextPath;
        updateServerPill();
        pathPill.textContent = nextPath ? "path: " + nextPath : "path: unset";

        if (previousServerId !== nextServerId || previousPath !== nextPath) {
          void loadImage();
        }
      }

      function handlePathInput() {
        state.path = normalizePath(pathInput.value);
        pathPill.textContent = state.path ? "path: " + state.path : "path: unset";
        schedulePersistPath();
        setStatus(state.path ? "Ready to load" : "Waiting for path", "idle");
      }

      function handleLoadClick() {
        syncCardState(pathInput.value);
        void loadImage();
      }

      function handleKeydown(event) {
        if (event.key !== "Enter") {
          return;
        }

        event.preventDefault();
        handleLoadClick();
      }

      app.addEventListener("lince-bridge-state", (event) => {
        applyBridgeDetail(event.detail || {});
      });

      pathInput.addEventListener("input", handlePathInput);
      pathInput.addEventListener("change", handlePathInput);
      pathInput.addEventListener("keydown", handleKeydown);
      loadButton.addEventListener("click", handleLoadClick);

      const fallbackState = readFallbackState();
      state.path = normalizePath(state.path || fallbackState.path);
      pathInput.value = state.path;
      pathPill.textContent = state.path ? "path: " + state.path : "path: unset";
      updateServerPill();
      setStatus("Waiting for config", "idle");
      renderIdleState("Choose a path and load it once the server is configured.");

      if (bridge?.requestState) {
        bridge.requestState();
      }

      if (state.serverId && state.path) {
        void loadImage();
      }
    "#
}
