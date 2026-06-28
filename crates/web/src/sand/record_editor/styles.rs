pub(crate) const INLINE_STYLES: [&str; 2] = [
    r#"      :root {
        color-scheme: dark;
        --bg: #15181d;
        --panel: #1d2229;
        --text: #f3f4f6;
        --muted: #9ca3af;
        --line: rgba(255, 255, 255, 0.13);
        --line-strong: rgba(255, 255, 255, 0.22);
        --accent: #8bd3ff;
        --good: #73e5a5;
        --danger: #fb7185;
      }

      * { box-sizing: border-box; }

      html,
      body {
        margin: 0;
        min-height: 100%;
        background: transparent;
      }

      body {
        min-height: 100vh;
        color: var(--text);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      .recordEditorApp,
      .recordEditor {
        min-height: 100vh;
      }

      .recordEditor {
        display: grid;
        grid-template-rows: auto auto 1fr auto;
        gap: 10px;
        padding: 12px;
        background: transparent;
      }

      .recordEditor__bar {
        display: flex;
        align-items: center;
        gap: 8px;
      }

      .recordEditor__status {
        width: 11px;
        height: 11px;
        border: 1px solid rgba(255, 255, 255, 0.35);
        border-radius: 50%;
        background: var(--good);
      }

      .recordEditor__statusButton {
        width: 22px;
        height: 22px;
        border: 0;
        border-radius: 50%;
        background: transparent;
        display: inline-grid;
        place-items: center;
        cursor: pointer;
      }

      .recordEditor__statusButton:focus-visible,
      .recordEditor__button:focus-visible,
      .recordEditor__title:focus,
      .recordEditor__body:focus,
      .recordEditor__search:focus {
        outline: 2px solid rgba(139, 211, 255, 0.75);
        outline-offset: 2px;
      }

      .recordEditor__mode {
        color: var(--muted);
        font-size: 11px;
        text-transform: uppercase;
      }

      .recordEditor__spacer { flex: 1; }

      .recordEditor__button {
        border: 1px solid var(--line);
        border-radius: 6px;
        background: rgba(255, 255, 255, 0.06);
        color: var(--text);
        padding: 6px 9px;
        font: inherit;
        cursor: pointer;
      }

      .recordEditor__button:hover {
        border-color: var(--line-strong);
      }

      .recordEditor__button[data-primary="true"] {
        color: var(--accent);
      }

      .recordEditor__title,
      .recordEditor__body,
      .recordEditor__search {
        width: 100%;
        border: 1px solid var(--line);
        border-radius: 6px;
        background: rgba(255, 255, 255, 0.05);
        color: var(--text);
        font: inherit;
      }

      .recordEditor__title {
        padding: 10px;
        font-size: 18px;
        font-weight: 650;
      }

      .recordEditor__body {
        min-height: 260px;
        resize: none;
        padding: 10px;
        font: 400 14px/1.65 "IBM Plex Mono", "SFMono-Regular", monospace;
      }

      .recordEditor__search {
        padding: 8px 10px;
      }

      .recordEditor__picker {
        border: 1px solid var(--line);
        border-radius: 6px;
        padding: 8px;
        background: rgba(0, 0, 0, 0.2);
      }

      .recordEditor__results {
        display: grid;
        gap: 4px;
        max-height: 170px;
        overflow: auto;
        margin-top: 8px;
      }

      .recordEditor__result {
        border: 1px solid var(--line);
        border-radius: 6px;
        background: rgba(255, 255, 255, 0.04);
        color: var(--text);
        text-align: left;
        padding: 7px 8px;
        cursor: pointer;
      }

      .recordEditor__hint,
      .recordEditor__footer {
        color: var(--muted);
        font-size: 12px;
      }

      .recordEditor__error {
        color: var(--danger);
      }

      [hidden] { display: none !important; }
    "#,
    crate::sand::shared_markdown::PREVIEW_STYLES,
];
