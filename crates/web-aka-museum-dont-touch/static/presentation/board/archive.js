// Static workspace export (the Archive sand's engine — see
// notes/institute/Playground.md). Turns the ACTIVE workspace into ONE
// self-contained HTML file: every sand as it looked, the data as it was
// rendered at that moment, and no way to make a request from the output.
//
// Runs in the board chrome, never inside a sand: only the chrome owns every
// card iframe same-origin, so only it can read their rendered DOM. The data
// captured is exactly what the archiving user could already see on screen —
// the store/Protein is never consulted here.
//
// The output's no-network guarantee is layered:
//   1. every <script>, on* handler, <link>, <base>, javascript: URL, srcset
//      and media src is stripped from each captured document;
//   2. every image and stylesheet is inlined (data: URIs / <style> text) and
//      remaining CSS url() references are neutralized to `url("data:,")`;
//   3. each captured document is embedded as <iframe srcdoc sandbox> — the
//      empty sandbox attribute blocks scripts/forms/navigation even if a
//      stripping pass ever misses something;
//   4. the outer page carries a CSP meta of `default-src 'none'` (plus
//      data: images and inline styles), which srcdoc children inherit.
// Session material never enters the file: the auth cookie is HttpOnly (not
// in any DOM), nothing from the outer board page (bootstrap JSON, tokens) is
// serialized, and password-ish input values are explicitly skipped.

const STAGE_PADDING = 24;

const STRIP_SELECTOR = [
  "script",
  "link",
  "base",
  "object",
  "embed",
  "applet",
  "iframe",
  "frame",
  "portal",
  "meta[http-equiv]",
].join(", ");

const URL_ATTRIBUTES = ["href", "src", "action", "formaction", "xlink:href"];

const SECRET_FIELD_PATTERN = /token|secret|password|senha|credential|apikey|api-key/i;

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

// srcdoc lives in an attribute: & and " must be escaped or the embedded
// document terminates the attribute. Nothing else — the browser parses the
// unescaped rest as the subdocument's own markup.
function escapeSrcdoc(html) {
  return String(html ?? "").replaceAll("&", "&amp;").replaceAll('"', "&quot;");
}

function slugifyFilename(value) {
  const slug = String(value ?? "")
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "workspace";
}

function archiveTimestamp(now) {
  const stamp = now instanceof Date ? now : new Date();
  const pad = (part) => String(part).padStart(2, "0");
  return (
    `${stamp.getFullYear()}${pad(stamp.getMonth() + 1)}${pad(stamp.getDate())}` +
    `-${pad(stamp.getHours())}${pad(stamp.getMinutes())}`
  );
}

// CSS may reference the network (fonts, background images). The inherited CSP
// already blocks those fetches; neutralizing the references as well keeps the
// file honest when inspected and silences console noise.
function neutralizeExternalCssUrls(cssText) {
  return String(cssText ?? "").replace(
    /url\(\s*(['"]?)([^'")]*)\1\s*\)/gi,
    (match, _quote, target) =>
      target.trim().toLowerCase().startsWith("data:") ? match : 'url("data:,")',
  );
}

// Everything the document's live CSSOM knows (inline <style>, same-origin
// <link> sheets) collected as one text block, in sheet order. Sheets whose
// rules are unreadable (cross-origin) are dropped — their <link> is stripped
// from the clone anyway, so nothing references them.
function collectStylesheetText(sandDocument) {
  const chunks = [];
  for (const sheet of Array.from(sandDocument.styleSheets || [])) {
    let rules = null;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    if (!rules) {
      continue;
    }
    const text = Array.from(rules)
      .map((rule) => rule.cssText)
      .join("\n");
    if (text) {
      chunks.push(text);
    }
  }
  return neutralizeExternalCssUrls(chunks.join("\n"));
}

async function imageToDataUrl(url) {
  const response = await fetch(url, { credentials: "same-origin" });
  if (!response.ok) {
    throw new Error(`fetch failed (${response.status})`);
  }
  const blob = await response.blob();
  return await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(new Error("image read failed"));
    reader.readAsDataURL(blob);
  });
}

// The live DOM holds form state in element properties, not attributes;
// serialization only sees attributes. Copy the visible state across so the
// archive shows what was on screen — except password-ish fields, whose
// values must never reach the file.
function reflectFormState(liveElements, clonedElements) {
  for (let index = 0; index < liveElements.length; index += 1) {
    const live = liveElements[index];
    const clone = clonedElements[index];
    if (!live || !clone) {
      continue;
    }
    const tag = live.tagName;
    if (tag === "TEXTAREA") {
      clone.textContent = live.value;
    } else if (tag === "SELECT") {
      const options = live.querySelectorAll("option");
      const clonedOptions = clone.querySelectorAll("option");
      for (let optionIndex = 0; optionIndex < options.length; optionIndex += 1) {
        clonedOptions[optionIndex]?.toggleAttribute(
          "selected",
          options[optionIndex].selected,
        );
      }
    } else if (tag === "INPUT") {
      const type = String(live.type || "").toLowerCase();
      const identity = `${live.name || ""} ${live.id || ""} ${live.autocomplete || ""}`;
      const secret =
        type === "password" || type === "hidden" || SECRET_FIELD_PATTERN.test(identity);
      if (type === "checkbox" || type === "radio") {
        clone.toggleAttribute("checked", live.checked);
      } else if (secret) {
        clone.removeAttribute("value");
      } else {
        clone.setAttribute("value", live.value);
      }
    }
  }
}

// A serialized <canvas> is blank; rasterize the live pixels into an <img>.
// Tainted canvases throw on toDataURL — those are dropped, not left blank.
function reflectCanvases(liveCanvases, clonedCanvases) {
  for (let index = 0; index < liveCanvases.length; index += 1) {
    const live = liveCanvases[index];
    const clone = clonedCanvases[index];
    if (!live || !clone) {
      continue;
    }
    let dataUrl = "";
    try {
      dataUrl = live.toDataURL("image/png");
    } catch {
      clone.remove();
      continue;
    }
    const image = clone.ownerDocument.createElement("img");
    image.setAttribute("src", dataUrl);
    image.setAttribute("width", String(live.width));
    image.setAttribute("height", String(live.height));
    if (clone.getAttribute("class")) {
      image.setAttribute("class", clone.getAttribute("class"));
    }
    if (clone.getAttribute("style")) {
      image.setAttribute("style", clone.getAttribute("style"));
    }
    clone.replaceWith(image);
  }
}

function stripActiveContent(root) {
  for (const node of Array.from(root.querySelectorAll(STRIP_SELECTOR))) {
    node.remove();
  }
  for (const element of Array.from(root.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on") || name === "srcset" || name === "ping") {
        element.removeAttribute(attribute.name);
        continue;
      }
      if (
        URL_ATTRIBUTES.includes(name) &&
        attribute.value.trim().toLowerCase().startsWith("javascript:")
      ) {
        element.removeAttribute(attribute.name);
      }
    }
    // Media cannot be inlined cheaply; a stripped src leaves an inert element
    // instead of a network reference.
    const tag = element.tagName;
    if (tag === "AUDIO" || tag === "VIDEO" || tag === "SOURCE" || tag === "TRACK") {
      element.removeAttribute("src");
    }
  }
}

// Images referenced by URL get fetched NOW (with the archiving user's
// session, same-origin only) and embedded as data: URIs. Anything that cannot
// be inlined is dropped — never left as a live URL.
async function inlineImages(liveImages, clonedImages) {
  for (let index = 0; index < liveImages.length; index += 1) {
    const live = liveImages[index];
    const clone = clonedImages[index];
    if (!live || !clone) {
      continue;
    }
    const source = String(live.currentSrc || live.src || "");
    if (!source || source.startsWith("data:")) {
      continue;
    }
    try {
      clone.setAttribute("src", await imageToDataUrl(source));
    } catch {
      clone.removeAttribute("src");
      clone.setAttribute("alt", clone.getAttribute("alt") || "imagem removida");
    }
  }
}

/// One sand document -> one static, self-referencing HTML string.
async function serializeSandDocument(sandDocument) {
  const liveRoot = sandDocument.documentElement;
  if (!liveRoot) {
    throw new Error("sand document has no root");
  }
  const clone = liveRoot.cloneNode(true);

  // querySelectorAll walks live and clone in identical document order, so
  // index-parallel iteration pairs each live element with its clone.
  reflectFormState(
    liveRoot.querySelectorAll("input, textarea, select"),
    clone.querySelectorAll("input, textarea, select"),
  );
  reflectCanvases(
    liveRoot.querySelectorAll("canvas"),
    clone.querySelectorAll("canvas"),
  );
  await inlineImages(liveRoot.querySelectorAll("img"), clone.querySelectorAll("img"));

  const styleText = collectStylesheetText(sandDocument);
  stripActiveContent(clone);
  // Old <style> nodes survived stripping (they're inert), but the CSSOM
  // collection above already covers them — drop them to avoid doubling.
  for (const style of Array.from(clone.querySelectorAll("style"))) {
    style.remove();
  }
  const head = clone.querySelector("head") || clone;
  const style = sandDocument.createElement("style");
  style.textContent = styleText;
  head.insertBefore(style, head.firstChild);

  return `<!doctype html>\n${clone.outerHTML}`;
}

function boundingRect(cards) {
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  for (const card of cards) {
    left = Math.min(left, card.x);
    top = Math.min(top, card.y);
    right = Math.max(right, card.x + card.width);
    bottom = Math.max(bottom, card.y + card.height);
  }
  return { left, top, width: right - left, height: bottom - top };
}

function renderTextCard(card, rect) {
  const position =
    `left:${card.x - rect.left + STAGE_PADDING}px;top:${card.y - rect.top + STAGE_PADDING}px;` +
    `width:${card.width}px;height:${card.height}px;z-index:${card.zIndex || 1};`;
  return (
    `<section class="card text-card" style="${position}">` +
    `<h2>${escapeHtml(card.title)}</h2>` +
    `<p>${escapeHtml(card.text || card.description || "")}</p>` +
    `</section>`
  );
}

function renderSandCard(card, rect, serializedHtml) {
  const position =
    `left:${card.x - rect.left + STAGE_PADDING}px;top:${card.y - rect.top + STAGE_PADDING}px;` +
    `width:${card.width}px;height:${card.height}px;z-index:${card.zIndex || 1};`;
  return (
    `<iframe class="card" sandbox title="${escapeHtml(card.title)}" ` +
    `style="${position}" srcdoc="${escapeSrcdoc(serializedHtml)}"></iframe>`
  );
}

const OUTER_STYLE = `
  * { box-sizing: border-box; }
  body { margin: 0; background: #0d1117; color: #e6edf3; font: 14px/1.5 system-ui, sans-serif; }
  .stage { position: relative; }
  .card { position: absolute; border: 1px solid #29313b; border-radius: 10px; background: #101318; overflow: hidden; }
  iframe.card { border: 1px solid #29313b; }
  .text-card { padding: 14px; overflow: auto; }
  .text-card h2 { margin: 0 0 8px; font-size: 15px; }
  .text-card p { margin: 0; color: #b6c2cf; white-space: pre-wrap; word-break: break-word; }
`;

/**
 * Archive the given workspace cards into one static HTML page.
 *
 * `cards` is the active workspace's card list (store snapshot shape);
 * `frameForCard(cardId)` resolves a package card's live iframe;
 * `excludeCardIds` removes the Archive sand's own card from its output.
 * Shell chrome (system/pinned cards) never belongs in an archive: it is
 * host UI, and its on-screen position is viewport-anchored, not world-anchored.
 *
 * Returns { filename, html, included, skipped } where skipped lists
 * { id, title, reason } for cards that could not be captured.
 */
export async function buildWorkspaceArchive({
  workspaceName,
  cards,
  frameForCard,
  excludeCardIds = [],
  filename = "",
  now = null,
}) {
  const excluded = new Set(excludeCardIds.filter(Boolean));
  const skipped = [];
  const captured = [];

  const candidates = (Array.isArray(cards) ? cards : []).filter(
    (card) =>
      card &&
      !excluded.has(card.id) &&
      card.system !== true &&
      card.pinned !== true,
  );

  for (const card of candidates) {
    if (card.kind === "text" || (!card.html && card.kind !== "package")) {
      captured.push({ card, kind: "text" });
      continue;
    }
    const frame = typeof frameForCard === "function" ? frameForCard(card.id) : null;
    const sandDocument = frame?.contentDocument || null;
    if (!sandDocument?.documentElement) {
      skipped.push({
        id: card.id,
        title: card.title,
        reason: "sem iframe renderizado",
      });
      continue;
    }
    try {
      captured.push({
        card,
        kind: "sand",
        html: await serializeSandDocument(sandDocument),
      });
    } catch (error) {
      skipped.push({
        id: card.id,
        title: card.title,
        reason: error instanceof Error ? error.message : "captura falhou",
      });
    }
  }

  if (!captured.length) {
    throw new Error("Nenhum card capturavel neste workspace.");
  }

  const rect = boundingRect(captured.map((entry) => entry.card));
  const stageWidth = Math.ceil(rect.width + STAGE_PADDING * 2);
  const stageHeight = Math.ceil(rect.height + STAGE_PADDING * 2);

  const body = captured
    .slice()
    .sort((a, b) => (a.card.zIndex || 1) - (b.card.zIndex || 1))
    .map((entry) =>
      entry.kind === "sand"
        ? renderSandCard(entry.card, rect, entry.html)
        : renderTextCard(entry.card, rect),
    )
    .join("\n");

  const title = String(workspaceName || "Workspace");
  const html =
    `<!doctype html>\n<html lang="en">\n<head>\n` +
    `<meta charset="utf-8"/>\n` +
    `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'"/>\n` +
    `<meta name="viewport" content="width=device-width, initial-scale=1"/>\n` +
    `<meta name="generator" content="Lince Archive sand"/>\n` +
    `<title>${escapeHtml(title)}</title>\n` +
    `<style>${OUTER_STYLE}</style>\n` +
    `</head>\n<body>\n` +
    `<main class="stage" style="width:${stageWidth}px;height:${stageHeight}px">\n` +
    `${body}\n` +
    `</main>\n</body>\n</html>\n`;

  const requestedName = String(filename || "").trim();
  const safeName = requestedName
    ? `${slugifyFilename(requestedName.replace(/\.html?$/i, ""))}.html`
    : `${slugifyFilename(title)}-${archiveTimestamp(now)}.html`;

  return {
    filename: safeName,
    html,
    included: captured.length,
    skipped,
  };
}
