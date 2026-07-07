pub(crate) const INLINE_STYLES: [&str; 1] = [r#"      :root {
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

      .recordInfoApp {
        display: flex;
        min-height: 100vh;
        padding: 10px;
      }

      .recordInfoBall {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 56px;
        height: 56px;
        margin: auto;
        border: 1px solid var(--line-strong);
        border-radius: 50%;
        background: var(--panel);
        color: var(--accent);
        font-size: 22px;
        cursor: default;
        transition: transform 160ms ease, border-color 160ms ease;
      }

      .recordInfoBall:hover {
        transform: scale(1.06);
        border-color: var(--accent);
      }

      .recordInfoPanel {
        display: flex;
        flex: 1;
        flex-direction: column;
        gap: 10px;
        min-width: 0;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 10px;
        background: var(--panel);
      }

      .recordInfoPanel__header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
      }

      .recordInfoPanel__title {
        margin: 0;
        overflow: hidden;
        font-size: 14px;
        font-weight: 600;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .recordInfoPanel__close {
        flex: none;
        width: 26px;
        height: 26px;
        border: 1px solid var(--line);
        border-radius: 6px;
        background: transparent;
        color: var(--muted);
        font-size: 14px;
        line-height: 1;
        cursor: pointer;
      }

      .recordInfoPanel__close:hover {
        border-color: var(--line-strong);
        color: var(--text);
      }

      .recordInfoStatus {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
        color: var(--muted);
      }

      .recordInfoStatus::before {
        content: "";
        width: 7px;
        height: 7px;
        border-radius: 50%;
        background: var(--muted);
      }

      .recordInfoStatus[data-state="live"] { color: var(--good); }
      .recordInfoStatus[data-state="live"]::before { background: var(--good); }
      .recordInfoStatus[data-state="error"] { color: var(--danger); }
      .recordInfoStatus[data-state="error"]::before { background: var(--danger); }

      .recordInfoFields {
        display: grid;
        grid-template-columns: minmax(72px, auto) 1fr;
        gap: 6px 12px;
        margin: 0;
        overflow: auto;
      }

      .recordInfoFields dt {
        overflow: hidden;
        color: var(--muted);
        font-size: 11px;
        letter-spacing: 0.04em;
        text-overflow: ellipsis;
        text-transform: uppercase;
        white-space: nowrap;
      }

      .recordInfoFields dd {
        min-width: 0;
        margin: 0;
        overflow-wrap: anywhere;
        font-size: 13px;
      }

      .recordInfoFacts {
        display: flex;
        flex-direction: column;
        gap: 6px;
        min-height: 0;
        padding-top: 2px;
        border-top: 1px solid var(--line);
      }

      .recordInfoFacts__title {
        margin: 0;
        color: var(--muted);
        font-size: 11px;
        font-weight: 600;
        letter-spacing: 0.04em;
        text-transform: uppercase;
      }

      .recordInfoFacts__list {
        display: flex;
        flex-direction: column;
        gap: 4px;
        margin: 0;
        padding: 0;
        overflow: auto;
        list-style: none;
      }

      .recordInfoFact {
        display: grid;
        grid-template-columns: minmax(38px, auto) minmax(0, 1fr);
        gap: 2px 8px;
        align-items: baseline;
        padding: 6px 0;
        border-bottom: 1px solid rgba(255, 255, 255, 0.08);
      }

      .recordInfoFact:last-child {
        border-bottom: 0;
      }

      .recordInfoFact__delta {
        color: var(--good);
        font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
        font-size: 12px;
        font-weight: 600;
        white-space: nowrap;
      }

      .recordInfoFact__cause {
        min-width: 0;
        overflow-wrap: anywhere;
        font-size: 12px;
      }

      .recordInfoFact__when {
        grid-column: 2;
        color: var(--muted);
        font-size: 11px;
      }

      .recordInfoEmpty {
        margin: 0;
        color: var(--muted);
        font-size: 12px;
      }
"#];
