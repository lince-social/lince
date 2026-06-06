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
        const viewModeRow = document.getElementById("view-mode-row");
        const pdfModeSelect = document.getElementById("pdf-mode-select");
        const pdfPageInput = document.getElementById("pdf-page-input");
        const epubControls = document.getElementById("epub-controls");
        const epubPrev = document.getElementById("epub-prev");
        const epubNext = document.getElementById("epub-next");
        const image = document.getElementById("image");
        const pdfFrame = document.getElementById("pdf-frame");
        const epubViewer = document.getElementById("epub-viewer");
        const previewFrame = document.getElementById("frame");
        const navHit = document.getElementById("nav-hit");
        const navPrev = document.getElementById("nav-prev");
        const navNext = document.getElementById("nav-next");
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
          epubCfi: bridgeState.epubCfi || fallbackState.epubCfi || "",
          epubScrollTop: bridgeState.epubScrollTop ?? fallbackState.epubScrollTop ?? 0,
          loaded: Boolean(bridgeState.loaded ?? fallbackState.loaded ?? false),
        };

        const defaultPickButtonLabel = pickButton.textContent || "Choose";
        let selectedLocalFile = null;
        let currentObjectUrl = "";
        let epubBook = null;
        let epubRendition = null;
        let currentKind = "";
        let persistTimer = 0;
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

        function normalizeScrollTop(rawScrollTop) {
          const scrollTop = Number.parseInt(String(rawScrollTop || "0"), 10);
          return Number.isFinite(scrollTop) && scrollTop > 0 ? scrollTop : 0;
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
            epubCfi: String(value.epubCfi || value.cfi || "").trim(),
            epubScrollTop: normalizeScrollTop(value.epubScrollTop || value.scrollTop || 0),
            loaded: Object.prototype.hasOwnProperty.call(value, "loaded") ? Boolean(value.loaded) : undefined,
          };
        }

        function readFallbackState() {
          const storage = storageArea();
          if (!storage) {
            return { source: "local", path: "", pdfMode: "scroll", pdfPage: 1, epubCfi: "", epubScrollTop: 0, loaded: false };
          }
          try {
            return normalizeCardState(JSON.parse(storage.getItem(stateKey) || "{}"));
          } catch (error) {
            return { source: "local", path: "", pdfMode: "scroll", pdfPage: 1, epubCfi: "", epubScrollTop: 0, loaded: false };
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
            epubCfi: state.epubCfi,
            epubScrollTop: state.epubScrollTop,
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
          app.classList.toggle("hasDocument", state.loaded);
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

        function renderModeControls(kind = documentKindFromPath(state.path)) {
          const normalizedKind = String(kind || "");
          viewModeRow.hidden = normalizedKind !== "pdf" && normalizedKind !== "epub" && normalizedKind !== "image";
          pdfPageInput.hidden = normalizedKind !== "pdf";
          epubControls.hidden = normalizedKind !== "epub";
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
          renderModeControls();
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
          if (typeof next.epubCfi === "string") state.epubCfi = next.epubCfi;
          if (typeof next.epubScrollTop === "number") state.epubScrollTop = next.epubScrollTop;
          if (typeof next.loaded === "boolean") state.loaded = next.loaded;

          sourceSelect.value = state.source;
          pathInput.value = state.path;
          pdfModeSelect.value = state.pdfMode;
          pdfPageInput.value = String(state.pdfPage);
          renderModeControls();
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
          if (clean.endsWith(".epub")) return "epub";
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

        function clearEpub() {
          rememberEpubScroll();
          if (epubRendition) {
            try { epubRendition.destroy(); } catch (error) {}
            epubRendition = null;
          }
          if (epubBook) {
            try { epubBook.destroy(); } catch (error) {}
            epubBook = null;
          }
          epubViewer.innerHTML = "";
        }

        function clearPreview() {
          clearEpub();
          if (currentObjectUrl) {
            URL.revokeObjectURL(currentObjectUrl);
            currentObjectUrl = "";
          }
          currentKind = "";
          image.removeAttribute("src");
          pdfFrame.removeAttribute("src");
          image.hidden = true;
          pdfFrame.hidden = true;
          epubViewer.hidden = true;
          navHit.hidden = true;
          navHit.classList.remove("isEdgeOnly");
          epubViewer.classList.remove("isScroll");
          previewFrame.classList.remove("isImageScroll");
          app.classList.remove("hasDocument");
        }

        function setEmpty(title, copy) {
          clearPreview();
          emptyTitle.textContent = title;
          emptyCopy.textContent = copy;
          empty.hidden = false;
        }

        function schedulePersistState() {
          if (persistTimer) {
            window.clearTimeout(persistTimer);
          }
          persistTimer = window.setTimeout(() => {
            persistTimer = 0;
            persistState();
          }, 350);
        }

        function getEpubScrollTarget() {
          try {
            return epubViewer.querySelector("iframe")?.contentDocument?.scrollingElement || epubViewer;
          } catch (error) {
            return epubViewer;
          }
        }

        function rememberEpubScroll() {
          if (currentKind !== "epub" || state.pdfMode !== "scroll") return;
          const target = getEpubScrollTarget();
          state.epubScrollTop = normalizeScrollTop(target?.scrollTop || 0);
          schedulePersistState();
        }

        function bindEpubScrollPersistence() {
          if (state.pdfMode !== "scroll") return;
          const target = getEpubScrollTarget();
          target?.addEventListener?.("scroll", rememberEpubScroll, { passive: true });
        }

        function restoreEpubScroll() {
          if (state.pdfMode !== "scroll" || !state.epubScrollTop) return;
          window.setTimeout(() => {
            const target = getEpubScrollTarget();
            if (target) {
              target.scrollTop = state.epubScrollTop;
            }
          }, 120);
        }

        async function showEpub(url) {
          if (typeof window.ePub !== "function") {
            throw new Error("EPUB renderer is unavailable.");
          }
          epubViewer.hidden = false;
          epubBook = window.ePub(url, { openAs: "epub" });
          const isScrollMode = state.pdfMode === "scroll";
          epubViewer.classList.toggle("isScroll", isScrollMode);
          epubRendition = epubBook.renderTo(epubViewer, {
            width: "100%",
            height: "100%",
            flow: isScrollMode ? "scrolled-doc" : "paginated",
            manager: isScrollMode ? "continuous" : "default",
            allowScriptedContent: false,
          });
          epubBook.ready.catch((error) => {
            setDebug("epub-open-error\\n" + (error instanceof Error ? error.message : String(error)));
          });
          epubRendition.on("displayError", (section, error) => {
            setDebug("epub-display-error\\n" + (error instanceof Error ? error.message : String(error)));
          });
          epubRendition.on("relocated", (location) => {
            const cfi = String(location?.start?.cfi || "").trim();
            if (cfi) {
              state.epubCfi = cfi;
              schedulePersistState();
            }
          });
          epubRendition.on("rendered", () => {
            bindEpubScrollPersistence();
            restoreEpubScroll();
          });
          await epubRendition.display(state.epubCfi || undefined);
          bindEpubScrollPersistence();
          restoreEpubScroll();
        }

        async function showDocument(url, kind, title) {
          clearPreview();
          currentKind = kind;
          empty.hidden = true;
          navHit.classList.toggle("isEdgeOnly", kind === "epub" && state.pdfMode === "scroll");
          if (kind === "pdf") {
            pdfFrame.hidden = false;
            pdfFrame.src = pdfUrlWithView(url);
          } else if (kind === "epub") {
            await showEpub(url);
          } else {
            previewFrame.classList.toggle("isImageScroll", state.pdfMode === "scroll");
            image.hidden = false;
            image.src = url;
            image.alt = title;
          }
          navHit.hidden = false;
          app.classList.add("hasDocument");
          renderModeControls(kind);
        }

        async function loadLocalPickedFile(file) {
          const kind = documentKindFromPath(file.name || file.type || "");
          if (!kind) throw new Error("Only PDF, EPUB, JPEG, and PNG files are supported.");
          const objectUrl = URL.createObjectURL(file);
          try {
            await showDocument(objectUrl, kind, file.name);
            currentObjectUrl = objectUrl;
          } catch (error) {
            URL.revokeObjectURL(objectUrl);
            throw error;
          }
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
            setEmpty("Unsupported file", "Only PDF, EPUB, JPEG, and PNG files are supported.");
            return;
          }
          if (source === "bucket" && !String(state.serverId || "").trim()) {
            setLoaded(false);
            setEmpty("No server selected", "Choose a server for bucket files.");
            return;
          }

          try {
            if (hasPickedLocalFile) {
              await loadLocalPickedFile(selectedLocalFile);
            } else {
              if (!url) throw new Error("Invalid path.");
              await showDocument(url, kind, path || url);
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
          rememberEpubScroll();
          persistState();
          setLoaded(false);
          setEmpty("Document unloaded", "Path kept. Click Load to render it again.");
          setDebug("unloaded\\npath=" + state.path);
        }

        function scrollElementByPage(element, direction) {
          const target = element || previewFrame;
          const distance = Math.max(160, Math.floor((target.clientHeight || previewFrame.clientHeight || 600) * 0.86));
          target.scrollBy({ top: direction * distance, behavior: "smooth" });
        }

        function navigatePdf(direction) {
          if (state.pdfMode === "page") {
            state.pdfPage = Math.max(1, normalizePdfPage(state.pdfPage) + direction);
            pdfPageInput.value = String(state.pdfPage);
            persistState();
            if (pdfFrame.src) {
              pdfFrame.src = pdfUrlWithView(pdfFrame.src);
            }
            return;
          }
          try {
            pdfFrame.contentWindow?.scrollBy({ top: direction * Math.max(160, Math.floor(pdfFrame.clientHeight * 0.86)), behavior: "smooth" });
          } catch (error) {
            state.pdfPage = Math.max(1, normalizePdfPage(state.pdfPage) + direction);
            pdfPageInput.value = String(state.pdfPage);
            persistState();
            if (pdfFrame.src) {
              pdfFrame.src = pdfUrlWithView(pdfFrame.src);
            }
          }
        }

        function navigateImage(direction) {
          scrollElementByPage(previewFrame, direction);
        }

        function navigateEpub(direction) {
          if (!epubRendition) return;
          if (state.pdfMode === "page") {
            void (direction > 0 ? epubRendition.next() : epubRendition.prev());
            window.setTimeout(() => persistState(), 250);
            return;
          }
          try {
            const target = epubViewer.querySelector("iframe")?.contentDocument?.scrollingElement || epubViewer;
            const before = target.scrollTop;
            scrollElementByPage(target, direction);
            window.setTimeout(rememberEpubScroll, 260);
            window.setTimeout(() => {
              if (target.scrollTop === before) {
                void (direction > 0 ? epubRendition.next() : epubRendition.prev());
              }
            }, 180);
          } catch (error) {
            void (direction > 0 ? epubRendition.next() : epubRendition.prev());
          }
        }

        function navigateDocument(direction) {
          if (!state.loaded || !currentKind) return;
          if (currentKind === "pdf") {
            navigatePdf(direction);
          } else if (currentKind === "epub") {
            navigateEpub(direction);
          } else if (currentKind === "image") {
            navigateImage(direction);
          }
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
            navHit.classList.remove("isEdgeOnly");
            pdfFrame.src = pdfUrlWithView(pdfFrame.src);
          } else if (state.loaded && currentKind === "epub") {
            void loadDocument();
          } else if (state.loaded && currentKind === "image") {
            navHit.classList.remove("isEdgeOnly");
            previewFrame.classList.toggle("isImageScroll", state.pdfMode === "scroll");
          }
        });
        pdfPageInput.addEventListener("input", () => {
          syncFromInputs();
        });
        epubPrev.addEventListener("click", () => {
          navigateDocument(-1);
        });
        epubNext.addEventListener("click", () => {
          navigateDocument(1);
        });
        window.addEventListener("keydown", (event) => {
          if (!state.loaded) return;
          if (event.key === "ArrowLeft") {
            event.preventDefault();
            navigateDocument(-1);
          } else if (event.key === "ArrowRight") {
            event.preventDefault();
            navigateDocument(1);
          }
        });
        window.addEventListener("beforeunload", () => {
          rememberEpubScroll();
          persistState();
        });
        navPrev.addEventListener("click", () => navigateDocument(-1));
        navNext.addEventListener("click", () => navigateDocument(1));
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
        renderModeControls();
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
