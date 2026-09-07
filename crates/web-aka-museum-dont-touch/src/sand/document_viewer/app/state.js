(function () {
  const emptyState = () => ({
    source: "file",
    path: "",
    pdfMode: "scroll",
    pdfPage: 1,
    epubCfi: "",
    epubScrollTop: 0,
    loaded: false,
  });

  function storageArea() {
    try {
      return window.parent && window.parent !== window
        ? window.parent.localStorage
        : window.localStorage;
    } catch (error) {
      return null;
    }
  }

  function normalizeSource(rawSource) {
    const value = String(rawSource || "").trim().toLowerCase();
    if (value === "url" || value === "media" || value === "file") return value;
    if (value === "bucket") return "media";
    if (value === "local") return "file";
    return "file";
  }

  function normalizePath(source, rawPath) {
    const path = String(rawPath || "").trim();
    return normalizeSource(source) === "file" ? path.split(/[\\/]/).pop() || "" : path;
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
    const root = rawCardState && typeof rawCardState === "object" ? rawCardState : {};
    const value = root.documentViewer && typeof root.documentViewer === "object"
      ? root.documentViewer
      : root;
    const rawSource = value.source || value.kind || value.type || "";
    const rawPath = value.path || value.objectPath || value.key || value.url || "";
    const source = rawSource ? normalizeSource(rawSource) : rawPath ? "url" : "";
    return {
      source,
      path: normalizePath(source || "file", rawPath),
      pdfMode: normalizePdfMode(value.pdfMode || value.viewMode || ""),
      pdfPage: normalizePdfPage(value.pdfPage || value.page || 1),
      epubCfi: String(value.epubCfi || value.cfi || "").trim(),
      epubScrollTop: normalizeScrollTop(value.epubScrollTop || value.scrollTop || 0),
      loaded: Object.prototype.hasOwnProperty.call(value, "loaded")
        ? Boolean(value.loaded)
        : undefined,
    };
  }

  function readFallbackState(key) {
    const storage = storageArea();
    if (!storage) return emptyState();
    try {
      return normalizeCardState(JSON.parse(storage.getItem(key) || "{}"));
    } catch (error) {
      return emptyState();
    }
  }

  function writeFallbackState(key, state) {
    const storage = storageArea();
    if (!storage) return;
    try {
      storage.setItem(key, JSON.stringify(state));
    } catch (error) {
    }
  }

  window.LinceDocumentViewerState = {
    normalizeSource,
    normalizePath,
    normalizePdfMode,
    normalizePdfPage,
    normalizeScrollTop,
    normalizeCardState,
    readFallbackState,
    writeFallbackState,
  };
})();
