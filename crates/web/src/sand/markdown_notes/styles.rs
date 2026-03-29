pub(super) const INLINE_STYLES: [&str; 2] = [r#"      :root {
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
      }

      [hidden] {
        display: none !important;
      }
    "#, crate::sand::shared_markdown::PREVIEW_STYLES];
