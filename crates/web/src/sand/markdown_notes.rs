use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"markdown-notes.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"✎"#.into(),
            title: r#"Markdown Notes"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Bloco de notas em Markdown com alternancia entre texto cru e preview renderizado."#.into(),
            details: r#"Widget minimalista sem moldura: um switch pequeno no topo direito alterna entre edicao raw e renderizacao Markdown."#.into(),
            initial_width: 4,
            initial_height: 4,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        --bg: transparent;
        --text: #f3f4f6;
        --muted: rgba(209, 213, 219, 0.68);
        --line: rgba(255, 255, 255, 0.14);
        --line-strong: rgba(255, 255, 255, 0.24);
        --accent: #f9fafb;
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
        padding: 12px 14px 14px;
        color: var(--text);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      .app {
        display: grid;
        grid-template-rows: auto 1fr;
        gap: 10px;
        min-height: calc(100vh - 26px);
      }

      .toolbar {
        display: flex;
        align-items: center;
        justify-content: flex-end;
      }

      .mode-switch {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        color: var(--muted);
        font-size: 11px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        user-select: none;
      }

      .mode-switch input {
        position: absolute;
        opacity: 0;
        pointer-events: none;
      }

      .mode-switch__track {
        position: relative;
        width: 34px;
        height: 20px;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.08);
        transition: background 160ms ease;
      }

      .mode-switch__track::after {
        content: "";
        position: absolute;
        top: 2px;
        left: 2px;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: rgba(255, 255, 255, 0.92);
        transition: transform 160ms ease;
      }

      .mode-switch input:checked + .mode-switch__track {
        background: rgba(255, 255, 255, 0.18);
      }

      .mode-switch input:checked + .mode-switch__track::after {
        transform: translateX(14px);
      }

      .raw,
      .preview {
        min-height: 0;
      }

      .raw {
        width: 100%;
        height: 100%;
        padding: 0;
        border: 0;
        resize: none;
        color: var(--text);
        background: transparent;
        font: 400 14px/1.7 "IBM Plex Mono", "SFMono-Regular", monospace;
        outline: none;
      }

      .raw::placeholder {
        color: rgba(209, 213, 219, 0.38);
      }

      .preview {
        overflow: auto;
        color: var(--text);
        line-height: 1.7;
        word-break: break-word;
      }

      .preview :first-child {
        margin-top: 0;
      }

      .preview :last-child {
        margin-bottom: 0;
      }

      .preview h1,
      .preview h2,
      .preview h3,
      .preview h4 {
        margin: 1.05em 0 0.42em;
        line-height: 1.2;
      }

      .preview h1 { font-size: 1.8rem; }
      .preview h2 { font-size: 1.4rem; }
      .preview h3 { font-size: 1.1rem; }
      .preview h4 { font-size: 1rem; }

      .preview p,
      .preview ul,
      .preview ol,
      .preview blockquote,
      .preview pre {
        margin: 0 0 0.92em;
      }

      .preview ul,
      .preview ol {
        padding-left: 1.2rem;
      }

      .preview blockquote {
        padding-left: 0.9rem;
        color: var(--muted);
        border-left: 2px solid rgba(255, 255, 255, 0.14);
      }

      .preview code {
        padding: 0.08rem 0.32rem;
        border-radius: 6px;
        background: rgba(255, 255, 255, 0.06);
        font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
        font-size: 0.92em;
      }

      .preview pre {
        padding: 0;
        background: transparent;
        overflow: auto;
      }

      .preview pre code {
        display: block;
        padding: 0;
        background: transparent;
      }

      .preview hr {
        border: 0;
        border-top: 1px solid var(--line);
        margin: 1.2em 0;
      }

      .preview a {
        color: var(--accent);
      }

      [hidden] {
        display: none !important;
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r##"      const toggle = document.getElementById("mode-toggle");
      const modeLabel = document.getElementById("mode-label");
      const rawInput = document.getElementById("raw-input");
      const preview = document.getElementById("preview");
      const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
      const noteKey = "markdown-notes/content/" + instanceId;
      const modeKey = "markdown-notes/mode/" + instanceId;

      const DEFAULT_TEXT = [
        "# Notas",
        "",
        "Escreva em **Markdown**.",
        "",
        "- `Raw` para editar",
        "- `MD` para renderizar",
      ].join("\n");

      function escapeHtml(value) {
        return String(value)
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;")
          .replaceAll('"', "&quot;")
          .replaceAll("'", "&#39;");
      }

      function readStorage(key, fallback) {
        try {
          const value = window.localStorage.getItem(key);
          return value == null ? fallback : value;
        } catch (error) {
          return fallback;
        }
      }

      function writeStorage(key, value) {
        try {
          window.localStorage.setItem(key, value);
        } catch (error) {
          return;
        }
      }

      function applyInlineMarkdown(text) {
        return escapeHtml(text)
          .replace(/`([^`]+)`/g, "<code>$1</code>")
          .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
          .replace(/\*([^*]+)\*/g, "<em>$1</em>")
          .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
      }

      function renderMarkdown(source) {
        const lines = String(source || "").replace(/\r\n/g, "\n").split("\n");
        const blocks = [];
        let paragraph = [];
        let listItems = [];
        let listTag = "";
        let codeFence = [];
        let inFence = false;

        function flushParagraph() {
          if (!paragraph.length) {
            return;
          }
          blocks.push("<p>" + applyInlineMarkdown(paragraph.join(" ")) + "</p>");
          paragraph = [];
        }

        function flushList() {
          if (!listItems.length) {
            return;
          }
          blocks.push(
            "<" +
              listTag +
              ">" +
              listItems.map((item) => "<li>" + applyInlineMarkdown(item) + "</li>").join("") +
              "</" +
              listTag +
              ">",
          );
          listItems = [];
          listTag = "";
        }

        function flushFence() {
          if (!inFence) {
            return;
          }
          blocks.push("<pre><code>" + escapeHtml(codeFence.join("\n")) + "</code></pre>");
          codeFence = [];
          inFence = false;
        }

        for (const rawLine of lines) {
          const line = rawLine.trimEnd();
          const trimmed = line.trim();

          if (trimmed.startsWith("```")) {
            flushParagraph();
            flushList();
            if (inFence) {
              flushFence();
            } else {
              inFence = true;
              codeFence = [];
            }
            continue;
          }

          if (inFence) {
            codeFence.push(rawLine);
            continue;
          }

          if (!trimmed) {
            flushParagraph();
            flushList();
            continue;
          }

          const heading = trimmed.match(/^(#{1,4})\s+(.+)$/);
          if (heading) {
            flushParagraph();
            flushList();
            const level = heading[1].length;
            blocks.push("<h" + level + ">" + applyInlineMarkdown(heading[2]) + "</h" + level + ">");
            continue;
          }

          if (trimmed === "---" || trimmed === "***") {
            flushParagraph();
            flushList();
            blocks.push("<hr />");
            continue;
          }

          const bullet = trimmed.match(/^[-*]\s+(.+)$/);
          if (bullet) {
            flushParagraph();
            if (listTag && listTag !== "ul") {
              flushList();
            }
            listTag = "ul";
            listItems.push(bullet[1]);
            continue;
          }

          const ordered = trimmed.match(/^\d+\.\s+(.+)$/);
          if (ordered) {
            flushParagraph();
            if (listTag && listTag !== "ol") {
              flushList();
            }
            listTag = "ol";
            listItems.push(ordered[1]);
            continue;
          }

          const quote = trimmed.match(/^>\s+(.+)$/);
          if (quote) {
            flushParagraph();
            flushList();
            blocks.push("<blockquote>" + applyInlineMarkdown(quote[1]) + "</blockquote>");
            continue;
          }

          flushList();
          paragraph.push(trimmed);
        }

        flushParagraph();
        flushList();
        flushFence();
        return blocks.join("");
      }

      function renderPreview() {
        preview.innerHTML = renderMarkdown(rawInput.value);
      }

      function setMode(rendered) {
        toggle.checked = rendered;
        modeLabel.textContent = rendered ? "MD" : "Raw";
        rawInput.hidden = rendered;
        preview.hidden = !rendered;
        writeStorage(modeKey, rendered ? "rendered" : "raw");

        if (rendered) {
          renderPreview();
        } else {
          rawInput.focus();
        }
      }

      rawInput.value = readStorage(noteKey, DEFAULT_TEXT);
      rawInput.addEventListener("input", () => {
        writeStorage(noteKey, rawInput.value);
        if (toggle.checked) {
          renderPreview();
        }
      });

      toggle.addEventListener("change", () => {
        setMode(toggle.checked);
      });

      setMode(readStorage(modeKey, "raw") === "rendered");
    "##)],
    }
}

fn body() -> Markup {
    html! {
        main class="app" {
            div class="toolbar" {
                label class="mode-switch" {
                    input id="mode-toggle" type="checkbox" aria-label="Alternar preview markdown";
                    span class="mode-switch__track" aria-hidden="true" {}
                    span id="mode-label" { "Raw" }
                }
            }
            textarea
                id="raw-input"
                class="raw"
                spellcheck="false"
                placeholder="# Notas\n\nEscreva em Markdown aqui."
            {}
            article id="preview" class="preview" hidden {}
        }
    }
}
