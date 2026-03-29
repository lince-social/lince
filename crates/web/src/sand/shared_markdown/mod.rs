pub(crate) const JS_HELPERS: &str = r##"
      function escapeHtml(value) {
        return String(value == null ? "" : value).replace(/[&<>\"']/g, (char) =>
          ({
            "&": "&amp;",
            "<": "&lt;",
            ">": "&gt;",
            '"': "&quot;",
            "'": "&#39;",
          })[char],
        );
      }

      function applyInlineMarkdown(text) {
        return escapeHtml(text)
          .replace(/`([^`]+)`/g, "<code>$1</code>")
          .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
          .replace(/\*([^*]+)\*/g, "<em>$1</em>")
          .replace(
            /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
            '<a href="$2" target="_blank" rel="noreferrer">$1</a>',
          );
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
"##;

pub(crate) const PREVIEW_STYLES: &str = r#"
      .markdownRender {
        color: var(--text, inherit);
        line-height: 1.6;
        word-break: break-word;
      }

      .markdownRender :first-child {
        margin-top: 0;
      }

      .markdownRender :last-child {
        margin-bottom: 0;
      }

      .markdownRender h1,
      .markdownRender h2,
      .markdownRender h3,
      .markdownRender h4 {
        margin: 1.05em 0 0.42em;
        line-height: 1.2;
      }

      .markdownRender h1 { font-size: 1.8rem; }
      .markdownRender h2 { font-size: 1.4rem; }
      .markdownRender h3 { font-size: 1.1rem; }
      .markdownRender h4 { font-size: 1rem; }

      .markdownRender p,
      .markdownRender ul,
      .markdownRender ol,
      .markdownRender blockquote,
      .markdownRender pre {
        margin: 0 0 0.92em;
      }

      .markdownRender ul,
      .markdownRender ol {
        padding-left: 1.2rem;
      }

      .markdownRender blockquote {
        padding-left: 0.9rem;
        color: var(--muted, inherit);
        border-left: 2px solid rgba(255, 255, 255, 0.14);
      }

      .markdownRender code {
        padding: 0.08rem 0.32rem;
        border-radius: 6px;
        background: rgba(255, 255, 255, 0.06);
        font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
        font-size: 0.92em;
      }

      .markdownRender pre {
        overflow: auto;
      }

      .markdownRender pre code {
        display: block;
        padding: 0;
        background: transparent;
      }

      .markdownRender hr {
        border: 0;
        border-top: 1px solid var(--line, rgba(255, 255, 255, 0.14));
        margin: 1.2em 0;
      }

      .markdownRender a {
        color: var(--accent, inherit);
      }
"#;
