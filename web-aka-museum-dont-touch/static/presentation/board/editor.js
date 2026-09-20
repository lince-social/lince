// The reusable body editor (K6, institute "Body Magic"): a Notion-like slash
// block palette + @-mention picker over a plain <textarea>, and the shared
// line-block MARKDOWN renderer. The body stays canonical markdown — blocks
// are sugar over it, never a new storage format: typing `/h3` + Enter and
// typing `###` by hand produce the SAME stored text and the SAME visual
// block. Served embedded at the absolute `/board/editor.js` (like frame.js);
// exposes `window.LinceBodyEditor = { attach, renderMarkdown, BLOCKS }`.
// Record uses it first; any sand with a body (todo, table) can attach.
(function () {
  // ---- blocks ---------------------------------------------------------------
  // Selectable by NAME ("/h3", "/image", "/check") or by typing the underlying
  // characters ("/###", "/![", "/- [").
  const BLOCKS = [];
  for (let n = 1; n <= 7; n++) {
    BLOCKS.push({
      name: `Heading ${n}`,
      hint: "#".repeat(n),
      insert: `${"#".repeat(n)} `,
      keys: [`h${n}`, "#".repeat(n), "heading"],
    });
  }
  BLOCKS.push({
    // Picks a file from the system file explorer, uploads it to the host's
    // media store (`POST /host/media` — sniffed + opaquely named, see
    // presentation::http::media_assets), and inserts `![](/host/media/…)` —
    // the only image path that's actually servable (2026-07-17).
    name: "Image",
    hint: "pick a file…",
    keys: ["img", "image", "!["],
    pickFile: true,
  });
  BLOCKS.push({
    name: "Checkbox",
    hint: "- [ ]",
    insert: "- [ ] ",
    keys: ["check", "checkbox", "todo", "- [", "[]"],
  });

  const HEADING_RE = /^(#{1,7})\s+(.*)$/;
  const CHECKBOX_RE = /^(\s*)- \[( |x|X)\] (.*)$/;
  const IMG_RE = /!\[[^\]]*\]\(([^)\s]+)\)/g;
  // bare image URLs render too (comment bodies always did)
  const BARE_IMG_RE = /(^|\s)(https?:\/\/[^\s]+\.(?:png|jpe?g|gif|webp)(?:\?[^\s]*)?)/gi;
  const REF_RE = /@([a-z0-9][a-z0-9.\-]*)/gi;

  // ---- palette (one shared floating node, restyled per open) ----------------
  function makePalette() {
    const el = document.createElement("div");
    el.className = "lince-editor-palette";
    el.style.cssText = [
      "position:absolute", "z-index:60", "display:none", "min-width:180px",
      "max-height:200px", "overflow:auto", "background:#161b22",
      "border:1px solid #30363d", "border-radius:8px", "padding:4px",
      "font:12px/1.4 system-ui,sans-serif", "color:#e6edf3",
      "box-shadow:0 8px 24px rgba(0,0,0,.5)",
    ].join(";");
    document.body.appendChild(el);
    return el;
  }

  // ---- attach ---------------------------------------------------------------
  // opts.getNames: () => [{uid, slug, head}] for the @-mention picker.
  // Returns { detach }.
  function attach(textarea, opts = {}) {
    const palette = makePalette();
    // state.mode: null | "slash" | "mention"; tokenStart = index of "/" or "@"
    let state = { mode: null, tokenStart: 0, query: "", items: [], selected: 0 };

    function close() {
      state = { mode: null, tokenStart: 0, query: "", items: [], selected: 0 };
      palette.style.display = "none";
    }

    function scan() {
      const at = textarea.selectionStart;
      const before = textarea.value.slice(0, at);
      const lineStart = before.lastIndexOf("\n") + 1;
      const line = before.slice(lineStart);
      // slash mode: "/" at the START of a line, query without spaces
      const slash = line.match(/^\/(\S*)$/);
      if (slash) {
        openSlash(lineStart, slash[1]);
        return;
      }
      // mention mode: "@" preceded by start/whitespace, query without spaces
      const mention = before.match(/(?:^|\s)@([a-z0-9.\-]*)$/i);
      if (mention) {
        openMention(at - mention[1].length - 1, mention[1]);
        return;
      }
      close();
    }

    function openSlash(tokenStart, query) {
      const q = query.toLowerCase();
      const items = BLOCKS.filter((b) =>
        !q
        || b.keys.some((k) => k.startsWith(q))
        || b.name.toLowerCase().includes(q)).map((b) => ({
          label: b.name,
          hint: b.hint,
          apply: () => applyBlock(b, tokenStart),
        }));
      open("slash", tokenStart, query, items);
    }

    function openMention(tokenStart, query) {
      const names = (typeof opts.getNames === "function" ? opts.getNames() : []) || [];
      const q = query.toLowerCase();
      const items = names.filter((n) =>
        !q
        || String(n.slug || "").toLowerCase().startsWith(q)
        || String(n.head || "").toLowerCase().includes(q))
        .slice(0, 8)
        .map((n) => ({
          label: n.head || n.slug || n.uid,
          hint: n.slug ? `@${n.slug}` : "",
          apply: () => applyMention(n, tokenStart),
        }));
      open("mention", tokenStart, query, items);
    }

    function open(mode, tokenStart, query, items) {
      if (!items.length) { close(); return; }
      state = { mode, tokenStart, query, items, selected: 0 };
      renderPalette();
      const rect = textarea.getBoundingClientRect();
      palette.style.left = `${rect.left + window.scrollX + 8}px`;
      palette.style.top = `${rect.bottom + window.scrollY + 4}px`;
      palette.style.display = "block";
    }

    function renderPalette() {
      palette.innerHTML = "";
      state.items.forEach((item, at) => {
        const rowEl = document.createElement("div");
        rowEl.className = at === state.selected
          ? "lince-editor-item selected" : "lince-editor-item";
        rowEl.style.cssText = "display:flex;gap:8px;align-items:baseline;"
          + "padding:4px 8px;border-radius:6px;cursor:pointer;"
          + (at === state.selected ? "background:#1c2c4a;" : "");
        const label = document.createElement("span");
        label.style.cssText = "flex:1";
        label.textContent = item.label;
        rowEl.appendChild(label);
        if (item.hint) {
          const hint = document.createElement("span");
          hint.style.cssText = "color:#6e7681;font-family:monospace";
          hint.textContent = item.hint;
          rowEl.appendChild(hint);
        }
        // mousedown so the textarea keeps focus (click would blur first)
        rowEl.addEventListener("mousedown", (e) => { e.preventDefault(); item.apply(); });
        palette.appendChild(rowEl);
      });
    }

    function replaceRange(from, to, insert, caretStart, caretEnd) {
      const value = textarea.value;
      textarea.value = value.slice(0, from) + insert + value.slice(to);
      const base = from + (caretStart ?? insert.length);
      textarea.selectionStart = base;
      textarea.selectionEnd = from + (caretEnd ?? (caretStart ?? insert.length));
      close();
      // real input event so host dirty-tracking / auto-save fires
      textarea.dispatchEvent(new Event("input", { bubbles: true }));
      textarea.focus();
    }

    function applyBlock(block, tokenStart) {
      if (block.pickFile) { pickImageFile(tokenStart, textarea.selectionStart); return; }
      replaceRange(tokenStart, textarea.selectionStart, block.insert,
        block.caretStart, block.caretEnd);
    }

    // "/image" + Enter (or clicking it in the palette) opens a file picker —
    // no hand-typed URL. A placeholder holds the spot in the body while the
    // pick/upload is in flight.
    function pickImageFile(tokenStart, tokenEnd) {
      close();
      const placeholder = "![uploading…]()";
      replaceRange(tokenStart, tokenEnd, placeholder, placeholder.length, placeholder.length);
      let settled = false;
      const finish = (markdown) => {
        if (settled) return;
        settled = true;
        const idx = textarea.value.indexOf(placeholder, Math.max(0, tokenStart - 8));
        const from = idx >= 0 ? idx : tokenStart;
        replaceRange(from, from + placeholder.length, markdown, markdown.length, markdown.length);
      };
      // Prefer the server-side native picker (2026-07-18): opens the system
      // file dialog via xdg-desktop-portal in the Cell process itself — only
      // registered on the desktop build (`native-picker` feature). This
      // sidesteps WebKitGTK's own built-in <input type=file> chooser, which
      // crashes the whole app on this class of Linux setup (a GtkFileChooser-
      // Widget GSettings-schema lookup that's unconditionally fatal in GLib —
      // see docs/new-version-capabilities-and-maneirisms.md). A 404 (plain
      // CLI / remote Cell, no local display to show a dialog on) falls back
      // to the ordinary browser file input.
      fetch("/host/media/pick", { method: "POST" }).then(async (res) => {
        if (res.status === 404) { browserPick(); return; }
        if (!res.ok) throw new Error(`image pick failed (${res.status})`);
        const { path } = await res.json();
        finish(path ? `![](${path})` : "");
      }).catch((err) => {
        console.warn(err);
        browserPick();
      });
      function browserPick() {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = "image/png,image/jpeg,image/gif,image/webp";
        input.style.cssText = "position:fixed;left:-9999px;top:-9999px";
        document.body.appendChild(input);
        input.addEventListener("change", async () => {
          const file = input.files && input.files[0];
          input.remove();
          if (!file) { finish(""); return; }
          try {
            const form = new FormData();
            form.append("file", file);
            const res = await fetch("/host/media", { method: "POST", body: form });
            if (!res.ok) throw new Error(`image upload failed (${res.status})`);
            const { path } = await res.json();
            finish(`![](${path})`);
          } catch (err) {
            console.warn(err);
            finish("");
          }
        });
        input.addEventListener("cancel", () => { input.remove(); finish(""); });
        input.click();
      }
    }

    function applyMention(name, tokenStart) {
      const token = name.slug || name.uid;
      replaceRange(tokenStart, textarea.selectionStart, `@${token} `);
    }

    function onKeydown(e) {
      if (!state.mode) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        state.selected = (state.selected + 1) % state.items.length;
        renderPalette();
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        state.selected = (state.selected - 1 + state.items.length) % state.items.length;
        renderPalette();
      } else if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        state.items[state.selected]?.apply();
      } else if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    }

    const onInput = () => scan();
    const onClick = () => scan();
    const onBlur = () => setTimeout(close, 120);
    textarea.addEventListener("input", onInput);
    textarea.addEventListener("click", onClick);
    textarea.addEventListener("keydown", onKeydown);
    textarea.addEventListener("blur", onBlur);

    return {
      detach() {
        textarea.removeEventListener("input", onInput);
        textarea.removeEventListener("click", onClick);
        textarea.removeEventListener("keydown", onKeydown);
        textarea.removeEventListener("blur", onBlur);
        palette.remove();
      },
    };
  }

  // ---- renderer -------------------------------------------------------------
  // Line-block markdown -> DOM fragment. Same visuals wherever a body shows:
  // headings #×1–7, checkboxes (clickable when onToggle given), images,
  // @slug reference chips (navigable when onNavigate given).
  // opts: { resolveRef(token) -> {uid,label}|null, onNavigate(uid),
  //         onToggle(lineIndex), lines?: [start,end] }
  function renderMarkdown(body, opts = {}) {
    const fragment = document.createDocumentFragment();
    const lines = String(body || "").split("\n");
    const start = opts.lines ? opts.lines[0] : 0;
    const end = opts.lines ? Math.min(opts.lines[1], lines.length) : lines.length;
    for (let at = start; at < end; at++) {
      const line = lines[at];
      const fence = line.match(/^\s*```\s*([^\s`]*)/);
      if (fence) {
        let close = at + 1;
        while (close < end && !/^\s*```\s*$/.test(lines[close])) close++;
        const el = document.createElement("pre");
        el.className = fence[1].toLowerCase() === "mermaid"
          ? "md-block md-mermaid"
          : "md-block md-code";
        el.dataset.mdLine = String(at);
        el.dataset.mdLineEnd = String(Math.min(close + 1, end));
        el.textContent = lines.slice(at + 1, close).join("\n");
        fragment.appendChild(el);
        at = Math.min(close, end - 1);
        continue;
      }
      const heading = line.match(HEADING_RE);
      if (heading) {
        const level = heading[1].length;
        const el = document.createElement("div");
        el.className = `md-line md-h md-h${level}`;
        el.dataset.mdLine = String(at);
        el.style.cssText = `font-weight:700;font-size:${Math.max(1.02, 1.5 - 0.08 * (level - 1))}em;margin:.25em 0 .1em`;
        renderInline(el, heading[2], opts);
        fragment.appendChild(el);
        continue;
      }
      const checkbox = line.match(CHECKBOX_RE);
      if (checkbox) {
        const el = document.createElement("div");
        const done = checkbox[2] !== " ";
        el.className = done ? "md-line md-todo done" : "md-line md-todo";
        el.dataset.mdLine = String(at);
        el.style.cssText = "display:grid;grid-template-columns:auto minmax(0,1fr);gap:6px;align-items:start";
        const box = document.createElement("input");
        box.type = "checkbox";
        box.checked = done;
        box.dataset.mdLine = String(at);
        box.style.cssText = "margin:0;cursor:pointer";
        if (typeof opts.onToggle === "function") {
          box.addEventListener("click", (e) => { e.stopPropagation(); opts.onToggle(at); });
        } else {
          box.disabled = true;
        }
        const text = document.createElement("span");
        if (done) text.style.cssText = "text-decoration:line-through;opacity:.65";
        renderInline(text, checkbox[3], opts);
        el.append(box, text);
        fragment.appendChild(el);
        continue;
      }
      // plain line: inline images pulled out as block <img>, @refs as chips
      const holder = document.createElement("div");
      holder.className = "md-line";
      holder.dataset.mdLine = String(at);
      renderInline(holder, line, opts);
      if (!holder.hasChildNodes()) holder.appendChild(document.createElement("br"));
      fragment.appendChild(holder);
    }
    return fragment;
  }

  function renderInline(parent, text, opts) {
    const images = [];
    let stripped = text.replace(IMG_RE, (match, url) => { images.push(url); return ""; });
    stripped = stripped.replace(BARE_IMG_RE, (match, pre, url) => { images.push(url); return pre; });
    let last = 0;
    REF_RE.lastIndex = 0;
    for (let m; (m = REF_RE.exec(stripped)); ) {
      const ref = typeof opts.resolveRef === "function" ? opts.resolveRef(m[1]) : null;
      if (!ref) continue;
      parent.appendChild(document.createTextNode(stripped.slice(last, m.index)));
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = "md-ref";
      chip.dataset.ref = ref.uid;
      chip.textContent = `@${ref.label || m[1]}`;
      chip.style.cssText = "display:inline-block;border:1px solid #1f4b8e;"
        + "border-radius:999px;background:#12233c;color:#7aa2f7;padding:0 .45em;"
        + "font-size:.85em;cursor:pointer";
      if (typeof opts.onNavigate === "function") {
        chip.addEventListener("click", (e) => { e.stopPropagation(); opts.onNavigate(ref.uid); });
      }
      parent.appendChild(chip);
      last = m.index + m[0].length;
    }
    parent.appendChild(document.createTextNode(stripped.slice(last)));
    for (const url of images) {
      const img = document.createElement("img");
      img.src = url;
      img.alt = "";
      img.loading = "lazy";
      img.style.cssText = "max-width:100%;border-radius:6px;display:block;margin:.25em 0";
      parent.appendChild(img);
    }
  }

  window.LinceBodyEditor = { attach, renderMarkdown, BLOCKS };
})();
