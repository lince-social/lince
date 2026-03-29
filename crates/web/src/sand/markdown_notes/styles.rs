pub(super) const INLINE_STYLES: [&str; 1] = [r#"      :root {
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
    "#];
