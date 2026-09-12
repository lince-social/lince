(function () {
  function documentKindFromPath(path) {
    const clean = String(path || "").split("?")[0].split("#")[0].toLowerCase();
    if (clean.endsWith(".pdf")) return "pdf";
    if (clean.endsWith(".epub")) return "epub";
    if ([".png", ".jpg", ".jpeg", ".gif", ".webp"].some((ext) => clean.endsWith(ext))) {
      return "image";
    }
    return "";
  }

  function resolveUrl(source, path) {
    if (!path) return "";
    if (source === "media") {
      return /^\/host\/media\/[0-9a-f-]{36}\.(png|jpg|gif|webp)$/i.test(path) ? path : "";
    }
    try {
      const url = new URL(path);
      return url.protocol === "http:" || url.protocol === "https:" ? url.href : "";
    } catch (error) {
      return "";
    }
  }

  function pdfUrlWithView(url, mode, page) {
    const base = String(url || "").split("#")[0];
    return mode === "page"
      ? `${base}#page=${page}&view=FitH`
      : page > 1 ? `${base}#page=${page}` : base;
  }

  window.LinceDocumentViewerSources = { documentKindFromPath, resolveUrl, pdfUrlWithView };
})();
