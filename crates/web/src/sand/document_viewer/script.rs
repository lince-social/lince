pub(super) fn script() -> &'static str {
    r##"
      const bootDebug = document.getElementById("debug");
      try {
        const frame = window.frameElement;
        const sourceSelect = document.getElementById("source-select");
        const configToggle = document.getElementById("config-toggle");
        const configPanel = document.getElementById("config-panel");
        const pathInput = document.getElementById("path-input");
        const loadButton = document.getElementById("load-button");
        const unloadButton = document.getElementById("unload-button");
        const pickButton = document.getElementById("pick-button");
        const fileInput = document.getElementById("file-input");
        const pdfModeSelect = document.getElementById("pdf-mode-select");
        const pdfPageInput = document.getElementById("pdf-page-input");
        const image = document.getElementById("image");
        const pdfFrame = document.getElementById("pdf-frame");
        const empty = document.getElementById("empty");
        const emptyTitle = document.getElementById("empty-title");
        const emptyCopy = document.getElementById("empty-copy");
        const debug = document.getElementById("debug");
        const app = document.getElementById("app");

        const bridge = window.LinceWidgetHost || null;
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const stateKey = "document-viewer/" + instanceId;
        const fallbackState = readFallbackState();
        const bridgeState = normalizeCardState(bridge?.getCardState?.() || null);
        const state = {
          serverId: String(frame?.dataset?.linceServerId || ""),
          source: bridgeState.source || fallbackState.source || "local",
          path: bridgeState.path || fallbackState.path || "",
          pdfMode: bridgeState.pdfMode || fallbackState.pdfMode || "scroll",
          pdfPage: bridgeState.pdfPage || fallbackState.pdfPage || 1,
          loaded: Boolean(bridgeState.loaded ?? fallbackState.loaded ?? false),
        };

        const defaultPickButtonLabel = pickButton.textContent || "Choose";
        let selectedLocalFile = null;
        let currentObjectUrl = "";
        let configOpen = false;
        let didAttemptInitialLoad = false;
        let bridgeBound = false;

        function setDebug(message) {
          debug.textContent = String(message || "");
        }

        function storageArea() {
          try {
            return window.parent && window.parent !== window ? window.parent.localStorage : window.localStorage;
          } catch (error) {
            return null;
          }
        }

        function normalizeSource(rawSource) {
          const value = String(rawSource || "").trim().toLowerCase();
          return value === "bucket" || value === "url" || value === "local" ? value : "local";
        }

        function normalizePath(source, rawPath) {
          const path = String(rawPath || "").trim();
          return normalizeSource(source) === "bucket" ? path.replace(/^\/+/, "") : path;
        }

        function normalizePdfMode(rawMode) {
          return String(rawMode || "").trim().toLowerCase() === "page" ? "page" : "scroll";
        }

        function normalizePdfPage(rawPage) {
          const page = Number.parseInt(String(rawPage || "1"), 10);
          return Number.isFinite(page) && page > 0 ? page : 1;
        }

        function normalizeCardState(rawCardState) {
          const value = rawCardState && typeof rawCardState === "object" ? rawCardState : {};
          const rawSource = value.source || value.kind || value.type || "";
          const rawPath = value.path || value.objectPath || value.key || value.url || "";
          const source = rawSource ? normalizeSource(rawSource) : rawPath ? "bucket" : "";
          return {
            source,
            path: normalizePath(source || "local", rawPath),
            pdfMode: normalizePdfMode(value.pdfMode || value.viewMode || ""),
            pdfPage: normalizePdfPage(value.pdfPage || value.page || 1),
            loaded: Object.prototype.hasOwnProperty.call(value, "loaded") ? Boolean(value.loaded) : undefined,
          };
        }

        function readFallbackState() {
          const storage = storageArea();
          if (!storage) {
            return { source: "local", path: "", pdfMode: "scroll", pdfPage: 1, loaded: false };
          }
          try {
            return normalizeCardState(JSON.parse(storage.getItem(stateKey) || "{}"));
          } catch (error) {
            return { source: "local", path: "", pdfMode: "scroll", pdfPage: 1, loaded: false };
          }
        }

        function writeFallbackState(nextState) {
          const storage = storageArea();
          if (!storage) return;
          try {
            storage.setItem(stateKey, JSON.stringify(nextState));
          } catch (error) {
          }
        }

        function persistState() {
          const nextState = {
            source: state.source,
            path: state.path,
            pdfMode: state.pdfMode,
            pdfPage: state.pdfPage,
            loaded: state.loaded,
          };
          writeFallbackState(nextState);
          bridge?.patchCardState?.(nextState);
        }

        function setConfigOpen(nextOpen) {
          configOpen = Boolean(nextOpen);
          configPanel.hidden = !configOpen;
          configToggle.setAttribute("aria-expanded", configOpen ? "true" : "false");
        }

        function setLoaded(nextLoaded, persist = true) {
          state.loaded = Boolean(nextLoaded);
          loadButton.disabled = state.loaded;
          unloadButton.disabled = !state.loaded;
          if (persist) {
            persistState();
          }
        }

        function renderSourceHints() {
          if (state.source === "bucket") {
            pathInput.placeholder = "folder/document.pdf";
            pickButton.hidden = true;
          } else if (state.source === "url") {
            pathInput.placeholder = "https://example.com/document.pdf";
            pickButton.hidden = true;
          } else {
            pathInput.placeholder = "~/.config/lince/files/document.pdf";
            pickButton.hidden = false;
          }
        }

        function renderPickedFileState() {
          pickButton.textContent = selectedLocalFile ? selectedLocalFile.name : defaultPickButtonLabel;
        }

        function syncFromInputs() {
          state.source = normalizeSource(sourceSelect.value);
          state.path = normalizePath(state.source, pathInput.value);
          state.pdfMode = normalizePdfMode(pdfModeSelect.value);
          state.pdfPage = normalizePdfPage(pdfPageInput.value);
          sourceSelect.value = state.source;
          pathInput.value = state.path;
          pdfModeSelect.value = state.pdfMode;
          pdfPageInput.value = String(state.pdfPage);
          renderSourceHints();
          renderPickedFileState();
          persistState();
          setDebug(
            "sync\\nsource=" + state.source +
            "\\npath=" + state.path +
            "\\npdfMode=" + state.pdfMode +
            "\\npdfPage=" + state.pdfPage +
            "\\nloaded=" + state.loaded
          );
        }

        function applySavedState(savedState) {
          const next = normalizeCardState(savedState);
          if (next.source) state.source = next.source;
          if (next.path) state.path = next.path;
          if (next.pdfMode) state.pdfMode = next.pdfMode;
          if (next.pdfPage) state.pdfPage = next.pdfPage;
          if (typeof next.loaded === "boolean") state.loaded = next.loaded;

          sourceSelect.value = state.source;
          pathInput.value = state.path;
          pdfModeSelect.value = state.pdfMode;
          pdfPageInput.value = String(state.pdfPage);
          renderSourceHints();
          renderPickedFileState();
          setLoaded(state.loaded, false);
        }

        function savedStateFromBridgeDetail(detail) {
          const meta = detail?.meta && typeof detail.meta === "object"
            ? detail.meta
            : detail && typeof detail === "object" && !Array.isArray(detail)
              ? detail
              : bridge?.getMeta?.() || null;
          return meta?.cardState || detail?.cardState || bridge?.getCardState?.() || null;
        }

        function maybeAutoloadSavedDocument(reason) {
          if (didAttemptInitialLoad || !state.loaded) {
            return;
          }
          if (canRestoreLoadedDocument()) {
            didAttemptInitialLoad = true;
            setDebug("autoload\\nreason=" + reason + "\\npath=" + state.path);
            void loadDocument();
            return;
          }
          setLoaded(false);
          setEmpty("Document needs reload", "Picked local files cannot be restored after refresh. Choose it again or type a path.");
          setDebug("ready\\nautoloadSkipped=true\\nreason=" + reason);
        }

        function applyBridgeDetail(detail) {
          const savedState = savedStateFromBridgeDetail(detail);
          if (!savedState) {
            return;
          }
          applySavedState(savedState);
          maybeAutoloadSavedDocument("bridge");
        }

        function pdfUrlWithView(url) {
          const base = String(url || "").split("#")[0];
          const page = normalizePdfPage(state.pdfPage);
          return state.pdfMode === "page"
            ? base + "#page=" + page + "&view=FitH"
            : (page > 1 ? base + "#page=" + page : base);
        }

        function documentKindFromPath(path) {
          const clean = String(path || "").split("?")[0].split("#")[0].toLowerCase();
          if (clean.endsWith(".pdf")) return "pdf";
          if (clean.endsWith(".png") || clean.endsWith(".jpg") || clean.endsWith(".jpeg")) return "image";
          return "";
        }

        function canRestoreLoadedDocument() {
          const source = normalizeSource(state.source);
          const path = normalizePath(source, state.path);
          return Boolean(path) && Boolean(documentKindFromPath(path));
        }

        function getCurrentUrl() {
          const source = normalizeSource(state.source);
          const path = normalizePath(source, state.path);
          if (!path) return "";
          if (source === "bucket") {
            const serverId = String(state.serverId || "").trim();
            return serverId ? "/host/integrations/servers/" + encodeURIComponent(serverId) + "/files?path=" + encodeURIComponent(path) : "";
          }
          if (source === "local") {
            return "/host/local-files?path=" + encodeURIComponent(path);
          }
          try {
            const url = new URL(path);
            return url.protocol === "http:" || url.protocol === "https:" ? url.href : "";
          } catch (error) {
            return "";
          }
        }

        function clearPreview() {
          if (currentObjectUrl) {
            URL.revokeObjectURL(currentObjectUrl);
            currentObjectUrl = "";
          }
          image.removeAttribute("src");
          pdfFrame.removeAttribute("src");
          image.hidden = true;
          pdfFrame.hidden = true;
        }

        function setEmpty(title, copy) {
          clearPreview();
          emptyTitle.textContent = title;
          emptyCopy.textContent = copy;
          empty.hidden = false;
        }

        function showDocument(url, kind, title) {
          clearPreview();
          empty.hidden = true;
          if (kind === "pdf") {
            pdfFrame.hidden = false;
            pdfFrame.src = pdfUrlWithView(url);
          } else {
            image.hidden = false;
            image.src = url;
            image.alt = title;
          }
        }

        function loadLocalPickedFile(file) {
          const kind = documentKindFromPath(file.name || file.type || "");
          if (!kind) throw new Error("Only PDF, JPEG, and PNG files are supported.");
          currentObjectUrl = URL.createObjectURL(file);
          showDocument(currentObjectUrl, kind, file.name);
        }

        async function loadDocument() {
          syncFromInputs();
          const source = normalizeSource(state.source);
          const path = normalizePath(source, state.path);
          const hasPickedLocalFile = source === "local" && Boolean(selectedLocalFile);
          const url = getCurrentUrl();
          const kind = hasPickedLocalFile ? documentKindFromPath(selectedLocalFile.name || selectedLocalFile.type || "") : documentKindFromPath(path || url);
          setDebug("load\\nsource=" + source + "\\npath=" + path + "\\nurl=" + url + "\\nkind=" + kind + "\\npickedLocalFile=" + hasPickedLocalFile);

          if (!path && !hasPickedLocalFile) {
            setLoaded(false);
            setEmpty("No document selected", "Choose a file or path.");
            return;
          }
          if (!kind) {
            setLoaded(false);
            setEmpty("Unsupported file", "Only PDF, JPEG, and PNG files are supported.");
            return;
          }
          if (source === "bucket" && !String(state.serverId || "").trim()) {
            setLoaded(false);
            setEmpty("No server selected", "Choose a server for bucket files.");
            return;
          }

          try {
            if (hasPickedLocalFile) {
              loadLocalPickedFile(selectedLocalFile);
            } else {
              if (!url) throw new Error("Invalid path.");
              showDocument(url, kind, path || url);
            }
            setLoaded(true);
            setDebug("loaded\\nurl=" + url + "\\npickedLocalFile=" + hasPickedLocalFile);
          } catch (error) {
            setLoaded(false);
            setDebug("load-error\\n" + (error instanceof Error ? error.message : "unknown"));
            setEmpty("Load failed", error instanceof Error ? error.message : "The file could not be loaded.");
          }
        }

        function unloadDocument() {
          setLoaded(false);
          setEmpty("Document unloaded", "Path kept. Click Load to render it again.");
          setDebug("unloaded\\npath=" + state.path);
        }

        sourceSelect.addEventListener("change", () => {
          if (normalizeSource(sourceSelect.value) !== "local") {
            selectedLocalFile = null;
          }
          syncFromInputs();
        });
        pathInput.addEventListener("input", () => {
          if (selectedLocalFile) {
            selectedLocalFile = null;
          }
          syncFromInputs();
        });
        pdfModeSelect.addEventListener("change", () => {
          syncFromInputs();
          if (state.loaded && !pdfFrame.hidden && pdfFrame.src) {
            pdfFrame.src = pdfUrlWithView(pdfFrame.src);
          }
        });
        pdfPageInput.addEventListener("input", () => {
          syncFromInputs();
        });
        loadButton.addEventListener("click", () => void loadDocument());
        unloadButton.addEventListener("click", () => unloadDocument());
        configToggle.addEventListener("click", () => setConfigOpen(!configOpen));
        app.addEventListener("lince-bridge-state", (event) => {
          if (!event.detail || typeof event.detail !== "object") return;
          applyBridgeDetail(event.detail);
        });
        pickButton.addEventListener("click", () => {
          sourceSelect.value = "local";
          syncFromInputs();
          fileInput.click();
        });
        fileInput.addEventListener("change", () => {
          const file = fileInput.files && fileInput.files[0] ? fileInput.files[0] : null;
          if (!file) return;
          selectedLocalFile = file;
          state.loaded = true;
          renderPickedFileState();
          void loadDocument();
        });

        pdfModeSelect.value = state.pdfMode;
        pdfPageInput.value = String(state.pdfPage);
        sourceSelect.value = state.source;
        pathInput.value = state.path;
        renderSourceHints();
        renderPickedFileState();
        setConfigOpen(false);
        setLoaded(state.loaded, false);
        if (state.loaded) {
          maybeAutoloadSavedDocument("startup");
        }
        if (!state.loaded) {
          setEmpty("No document selected", "Use the inputs above.");
          setDebug("ready");
        }

        function bindBridgeWhenReady() {
          if (bridgeBound || !window.LinceWidgetHost || typeof window.LinceWidgetHost.subscribe !== "function") {
            return false;
          }
          bridgeBound = true;
          window.LinceWidgetHost.subscribe((detail) => applyBridgeDetail(detail));
          window.LinceWidgetHost.requestState?.();
          return true;
        }

        applyBridgeDetail({
          meta: bridge?.getMeta?.() || null,
          cardState: bridge?.getCardState?.() || null
        });
        if (!bindBridgeWhenReady()) {
          window.setTimeout(bindBridgeWhenReady, 0);
        }
      } catch (error) {
        if (bootDebug) {
          bootDebug.textContent = "fatal\\n" + (error && error.stack ? error.stack : String(error));
        }
        console.error(error);
      }
    "##
}
