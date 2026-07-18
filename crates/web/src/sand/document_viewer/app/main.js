      const bootDebug = document.getElementById("debug");
      try {
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

        const H = window.LinceWidgetHost;
        const State = window.LinceDocumentViewerState;
        const Sources = window.LinceDocumentViewerSources;
        const {
          normalizeSource,
          normalizePath,
          normalizePdfMode,
          normalizePdfPage,
          normalizeScrollTop,
          normalizeCardState,
        } = State;
        const { documentKindFromPath } = Sources;
        const instanceId = String(H?.instanceId || "preview").trim() || "preview";
        const stateKey = "document-viewer/" + instanceId;
        const fallbackState = State.readFallbackState(stateKey);
        const bridgeState = normalizeCardState(H?.getCardState?.() || null);
        const state = {
          source: bridgeState.source || fallbackState.source || "file",
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
        const scrollConsumptionTargets = new WeakSet();

        function consumeScrollInteraction(event) {
          event.stopPropagation();
        }

        function bindScrollConsumption(target) {
          if (!target || typeof target.addEventListener !== "function" || scrollConsumptionTargets.has(target)) {
            return;
          }
          scrollConsumptionTargets.add(target);
          target.addEventListener("wheel", consumeScrollInteraction, { capture: true, passive: true });
          target.addEventListener("touchmove", consumeScrollInteraction, { capture: true, passive: true });
        }

        function bindFrameScrollConsumption(frameElement) {
          bindScrollConsumption(frameElement);
          try {
            bindScrollConsumption(frameElement?.contentWindow);
            bindScrollConsumption(frameElement?.contentDocument);
            bindScrollConsumption(frameElement?.contentDocument?.documentElement);
            bindScrollConsumption(frameElement?.contentDocument?.body);
            bindScrollConsumption(frameElement?.contentDocument?.scrollingElement);
          } catch (error) {
          }
        }

        bindScrollConsumption(window);
        bindScrollConsumption(document);
        bindScrollConsumption(document.documentElement);
        bindScrollConsumption(document.body);
        bindScrollConsumption(app);
        bindScrollConsumption(previewFrame);
        bindScrollConsumption(image);
        bindScrollConsumption(pdfFrame);
        bindScrollConsumption(epubViewer);
        pdfFrame.addEventListener("load", () => bindFrameScrollConsumption(pdfFrame));

        function setDebug(message) {
          debug.textContent = String(message || "");
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
          State.writeFallbackState(stateKey, nextState);
          H?.patchCardState?.({ documentViewer: nextState });
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
          if (state.source === "media") {
            pathInput.disabled = false;
            pathInput.placeholder = "/host/media/…";
            pickButton.hidden = false;
            pickButton.textContent = "Pick image";
          } else if (state.source === "url") {
            pathInput.disabled = false;
            pathInput.placeholder = "https://example.com/document.pdf";
            pickButton.hidden = true;
          } else {
            pathInput.disabled = true;
            pathInput.placeholder = "Choose a PDF, EPUB, JPEG, or PNG";
            pickButton.hidden = false;
            pickButton.textContent = selectedLocalFile?.name || defaultPickButtonLabel;
          }
        }

        function renderPickedFileState() {
          if (state.source === "file") {
            pickButton.textContent = selectedLocalFile ? selectedLocalFile.name : defaultPickButtonLabel;
          }
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
          if (next.source || next.path) state.path = next.path;
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
          setEmpty("Document needs reload", "Device files cannot be restored after refresh. Choose the file again.");
          setDebug("ready\\nautoloadSkipped=true\\nreason=" + reason);
        }

        function applyBridgeState(savedState) {
          applySavedState(savedState || {});
          maybeAutoloadSavedDocument("bridge");
        }

        function canRestoreLoadedDocument() {
          const source = normalizeSource(state.source);
          const path = normalizePath(source, state.path);
          return source !== "file" && Boolean(path) && Boolean(documentKindFromPath(path));
        }

        function getCurrentUrl() {
          const source = normalizeSource(state.source);
          const path = normalizePath(source, state.path);
          return Sources.resolveUrl(source, path);
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
          bindScrollConsumption(target);
          bindFrameScrollConsumption(epubViewer.querySelector("iframe"));
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
            pdfFrame.src = Sources.pdfUrlWithView(url, state.pdfMode, normalizePdfPage(state.pdfPage));
          } else if (kind === "epub") {
            await showEpub(url);
          } else {
            previewFrame.classList.toggle("isImageScroll", state.pdfMode === "scroll");
            image.hidden = false;
            image.src = url;
            image.alt = title;
          }
          navHit.hidden = kind === "pdf";
          app.classList.add("hasDocument");
          renderModeControls(kind);
        }

        async function loadLocalPickedFile(file) {
          const kind = documentKindFromPath(file.name || file.type || "");
          if (!kind) throw new Error("Only PDF, EPUB, and common image files are supported.");
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
          const hasPickedLocalFile = source === "file" && Boolean(selectedLocalFile);
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
            setEmpty("Unsupported file", "Only PDF, EPUB, and common image files are supported.");
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
              pdfFrame.src = Sources.pdfUrlWithView(pdfFrame.src, state.pdfMode, normalizePdfPage(state.pdfPage));
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
              pdfFrame.src = Sources.pdfUrlWithView(pdfFrame.src, state.pdfMode, normalizePdfPage(state.pdfPage));
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
          if (normalizeSource(sourceSelect.value) !== "file") {
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
            pdfFrame.src = Sources.pdfUrlWithView(pdfFrame.src, state.pdfMode, normalizePdfPage(state.pdfPage));
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
        pickButton.addEventListener("click", async () => {
          if (normalizeSource(sourceSelect.value) === "media") {
            try {
              const response = await fetch("/host/media/pick", { method: "POST" });
              if (response.status === 404) {
                fileInput.click();
                return;
              }
              if (!response.ok) throw new Error(`host picker failed (${response.status})`);
              const result = await response.json();
              if (!result.path) return;
              state.path = String(result.path);
              pathInput.value = state.path;
              await loadDocument();
            } catch (error) {
              setDebug("pick-error\\n" + (error instanceof Error ? error.message : String(error)));
            }
            return;
          }
          fileInput.click();
        });
        fileInput.addEventListener("change", () => {
          const file = fileInput.files && fileInput.files[0] ? fileInput.files[0] : null;
          if (!file) return;
          selectedLocalFile = file;
          sourceSelect.value = "file";
          state.source = "file";
          state.path = file.name;
          pathInput.value = file.name;
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

        H?.onCardState?.((cardState) => applyBridgeState(cardState));
      } catch (error) {
        if (bootDebug) {
          bootDebug.textContent = "fatal\\n" + (error && error.stack ? error.stack : String(error));
        }
        console.error(error);
      }
