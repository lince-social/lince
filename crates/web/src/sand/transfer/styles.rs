pub(super) const INLINE_STYLES: &[&str] = &[r#"
  :root {
    color-scheme: dark;
    --bg: #101216;
    --panel: #171b21;
    --panel-2: #20262f;
    --field: #11161d;
    --line: rgba(255, 255, 255, 0.12);
    --text: #eef3f8;
    --muted: #9ca8b5;
    --accent: #86c7ff;
    --ok: #8fe3aa;
    --warn: #f0c979;
    --danger: #ff9a9a;
    --button: #283342;
    --button-hover: #344356;
    --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
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
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    color: var(--text);
    background: var(--bg);
  }

  button,
  input,
  select,
  textarea {
    font: inherit;
  }

  button {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--button);
    color: var(--text);
    min-height: 34px;
    padding: 0 12px;
    cursor: pointer;
  }

  button:hover:not(:disabled) { background: var(--button-hover); }
  button:disabled { cursor: not-allowed; opacity: 0.5; }
  button.primary { border-color: rgba(143, 227, 170, 0.5); color: var(--ok); }
  button.danger { border-color: rgba(255, 154, 154, 0.45); color: var(--danger); }
  button.danger:hover:not(:disabled) { background: rgba(255, 154, 154, 0.12); }
  button.iconButton {
    width: 36px;
    min-width: 36px;
    padding: 0;
    display: inline-grid;
    place-items: center;
    font-size: 1.05rem;
    line-height: 1;
  }

  input,
  select,
  textarea {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--field);
    color: var(--text);
    min-height: 34px;
    padding: 7px 9px;
  }

  textarea {
    min-height: 64px;
    resize: vertical;
    font-family: var(--mono);
    font-size: 0.78rem;
    line-height: 1.35;
  }

  label {
    display: grid;
    gap: 5px;
    min-width: 0;
  }

  .checkRow {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .checkRow input {
    width: auto;
    min-height: auto;
  }

  label > span {
    color: var(--muted);
    font-size: 0.76rem;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    font-size: 1.1rem;
    line-height: 1.25;
  }

  h2 {
    font-size: 0.9rem;
    line-height: 1.25;
  }

  .transferApp {
    min-height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr;
    gap: 12px;
    padding: 12px;
  }

  .topbar {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
    border-bottom: 1px solid var(--line);
    padding-bottom: 10px;
  }

  .topActions,
  .formActions,
  .inlineControls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .tabs {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .tabButton {
    min-height: 28px;
    padding: 0 9px;
    font-size: 0.76rem;
    color: var(--muted);
  }

  .tabButton[data-active="true"] {
    border-color: rgba(134, 199, 255, 0.45);
    color: var(--accent);
    background: rgba(134, 199, 255, 0.1);
  }

  .status {
    min-height: 18px;
    margin-top: 4px;
    color: var(--muted);
    font-size: 0.78rem;
  }

  .status[data-tone="ok"] { color: var(--ok); }
  .status[data-tone="warn"] { color: var(--warn); }
  .status[data-tone="danger"] { color: var(--danger); }

  .workspace {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(260px, 320px) minmax(0, 1fr);
    gap: 12px;
  }

  .sidebar,
  .mainColumn {
    min-width: 0;
    min-height: 0;
    display: grid;
    align-content: start;
    gap: 12px;
  }

  .panel {
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    overflow: hidden;
  }

  .panelHead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 38px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel-2);
  }

  .splitHead { align-items: baseline; }

  .panelBody,
  .formGrid,
  .proposalGrid {
    padding: 12px;
  }

  .formGrid {
    display: grid;
    gap: 10px;
  }

  .proposalGrid {
    display: grid;
    grid-template-columns: repeat(3, minmax(140px, 1fr));
    gap: 10px;
  }

  .proposalGrid .formActions {
    align-self: end;
  }

  .identityBox,
  .sideBox,
  .actionBox {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.025);
    padding: 10px;
  }

  .strong,
  .sideTitle,
  .transferTitle,
  .actionTitle {
    font-weight: 700;
  }

  .meta,
  .muted {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.35;
  }

  .mono,
  .quantity {
    font-family: var(--mono);
  }

  .quantity {
    color: var(--accent);
    font-size: 0.95rem;
  }

  .compactList {
    display: grid;
    gap: 6px;
    padding: 0 12px 12px;
  }

  .listRow,
  .transferRow {
    width: 100%;
    height: auto;
    min-height: 40px;
    display: grid;
    justify-items: start;
    text-align: left;
    gap: 3px;
    padding: 8px 10px;
  }

  .transferLayout {
    min-height: 480px;
    display: grid;
    grid-template-columns: minmax(230px, 0.42fr) minmax(0, 1fr);
  }

  .transferList {
    min-height: 0;
    max-height: 70vh;
    overflow: auto;
    display: grid;
    align-content: start;
    gap: 8px;
    padding: 12px;
    border-right: 1px solid var(--line);
  }

  .transferRow[data-active="true"] {
    border-color: rgba(134, 199, 255, 0.55);
    background: #223043;
  }

  .transferDetail {
    min-width: 0;
    min-height: 0;
    padding: 12px;
    display: grid;
    align-content: start;
    gap: 12px;
  }

  .detailHeader {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 24px;
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 0 8px;
    color: var(--muted);
    font-size: 0.72rem;
  }

  .chip[data-tone="ok"] { color: var(--ok); border-color: rgba(143, 227, 170, 0.32); }
  .chip[data-tone="warn"] { color: var(--warn); border-color: rgba(240, 201, 121, 0.32); }
  .chip[data-tone="danger"] { color: var(--danger); border-color: rgba(255, 154, 154, 0.32); }

  .sideGrid,
  .actionGrid,
  .packageGrid {
    display: grid;
    gap: 10px;
  }

  .sideGrid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .sideTitle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .inlineControls > select {
    width: min(220px, 100%);
  }

  .packageGrid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .packageGrid .formActions {
    grid-column: 1 / -1;
  }

  .eventSection {
    display: grid;
    gap: 8px;
  }

  .events {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .event {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 10px;
    background: rgba(255, 255, 255, 0.025);
  }

  .eventName {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-family: var(--mono);
    font-size: 0.78rem;
  }

  pre {
    margin: 8px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.72rem;
  }

  .emptyBlock {
    color: var(--muted);
    font-size: 0.82rem;
    padding: 12px;
  }

  @media (max-width: 900px) {
    .workspace,
    .transferLayout,
    .proposalGrid,
    .sideGrid,
    .packageGrid {
      grid-template-columns: 1fr;
    }

    .transferList {
      max-height: none;
      border-right: 0;
      border-bottom: 1px solid var(--line);
    }
  }
"#];
