pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #101412;
    --panel: #171b19;
    --panel2: #202522;
    --line: rgba(255,255,255,.12);
    --text: #eef5ef;
    --muted: #9fb0a7;
    --green: #7bd88f;
    --blue: #80b7d8;
    --yellow: #d8c06f;
    --red: #e98989;
    --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
  }
  * { box-sizing: border-box; }
  html, body { min-height: 100%; margin: 0; background: transparent; }
  html { height: 100%; overflow: hidden; }
  body {
    height: 100vh;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  }
  button, input, select { font: inherit; }
  .homeManager {
    height: 100vh;
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    overflow: hidden;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 10px 0;
    border-bottom: 1px solid var(--line);
    background: #0d1110;
  }
  .tab {
    min-height: 34px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 2px 2px 0 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .tab.isActive {
    background: var(--bg);
    border-color: var(--line);
    color: var(--text);
    font-weight: 800;
  }
  .tabPanel {
    min-height: 0;
    height: 100%;
    padding: 12px;
    overflow: hidden;
  }
  .nutritionPanel {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: 10px;
  }
  .topbar, .panel, .metric {
    border: 1px solid var(--line);
    border-radius: 2px;
    background: var(--panel);
  }
  .topbar {
    min-height: 72px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
  }
  h1, h2, p { margin: 0; }
  h1 { font-size: 1.25rem; letter-spacing: 0; }
  h2 { font-size: .9rem; letter-spacing: 0; }
  .eyebrow, .metric span, label span, .status {
    color: var(--muted);
    font-family: var(--mono);
    font-size: .68rem;
    font-weight: 800;
    letter-spacing: 0;
    text-transform: uppercase;
  }
  .guideStrip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: end;
  }
  .guideStrip span {
    padding: 5px 7px;
    border: 1px solid var(--line);
    border-radius: 2px;
    font-size: .7rem;
    font-weight: 800;
  }
  .guideStrip [data-tone="base"] { color: var(--green); }
  .guideStrip [data-tone="use"] { color: var(--blue); }
  .guideStrip [data-tone="limit"] { color: var(--yellow); }
  .guideStrip [data-tone="avoid"] { color: var(--red); }
  .metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .metric {
    min-height: 58px;
    display: grid;
    align-content: center;
    gap: 4px;
    padding: 10px 12px;
  }
  .metric strong { font-size: 1rem; overflow-wrap: anywhere; }
  .workspace {
    min-height: 0;
    height: 100%;
    display: grid;
    grid-template-columns: 270px minmax(300px, 1.2fr) minmax(260px, 1fr);
    grid-template-rows: minmax(0, 1fr) minmax(180px, .55fr);
    gap: 10px;
    overflow: hidden;
  }
  .panel {
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    overflow: hidden;
  }
  .controlsPanel {
    grid-row: 1 / span 2;
    grid-template-rows: auto auto auto auto;
    align-content: start;
    overflow: auto;
  }
  .shoppingPanel { grid-column: 2 / span 2; }
  .panelHeader {
    min-height: 44px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 9px 11px;
    border-bottom: 1px solid var(--line);
  }
  .formGrid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    padding: 10px;
  }
  label { display: grid; gap: 4px; min-width: 0; }
  input, select {
    width: 100%;
    min-height: 34px;
    border: 1px solid var(--line);
    border-radius: 2px;
    background: var(--panel2);
    color: var(--text);
    padding: 0 9px;
  }
  .button, .iconButton {
    min-height: 34px;
    border: 1px solid var(--line);
    border-radius: 2px;
    background: var(--panel2);
    color: var(--text);
    cursor: pointer;
    padding: 0 11px;
  }
  .primary { border-color: rgba(115,223,161,.42); color: var(--green); }
  .subtle { color: var(--muted); }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; padding: 0 10px 10px; }
  .status { padding: 0 10px 10px; line-height: 1.35; text-transform: none; }
  .catalogPanel { grid-template-rows: auto auto minmax(0, 1fr); }
  .shoppingPanel { grid-template-rows: auto auto minmax(0, 1fr); }
  .catalogTools { display: flex; gap: 8px; padding: 10px; border-bottom: 1px solid var(--line); }
  .foodList, .marmitaList, .shoppingList {
    min-height: 0;
    overflow: auto;
    padding: 10px;
  }
  .foodItem, .marmitaItem, .shoppingItem {
    border: 1px solid var(--line);
    border-radius: 2px;
    background: rgba(255,255,255,.025);
    padding: 9px;
  }
  .foodItem + .foodItem, .marmitaItem + .marmitaItem, .shoppingItem + .shoppingItem { margin-top: 8px; }
  .foodTop, .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .foodName { min-width: 0; font-weight: 800; overflow-wrap: anywhere; }
  .foodMeta, .small { color: var(--muted); font-size: .74rem; line-height: 1.35; }
  .totalsGrid {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 6px;
    padding: 10px;
    border-bottom: 1px solid var(--line);
  }
  .totalCell {
    min-height: 42px;
    display: grid;
    gap: 3px;
    padding: 7px;
    border: 1px solid var(--line);
    background: rgba(255,255,255,.018);
  }
  .totalCell span {
    color: var(--muted);
    font-family: var(--mono);
    font-size: .58rem;
    font-weight: 800;
    text-transform: uppercase;
  }
  .totalCell strong {
    font-size: .78rem;
    overflow-wrap: anywhere;
  }
  .foodControls {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
    margin-top: 8px;
  }
  .foodControls label span { font-size: .6rem; }
  .check {
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
  }
  .dialog {
    width: min(720px, calc(100vw - 24px));
    border: 1px solid var(--line);
    border-radius: 2px;
    background: var(--panel);
    color: var(--text);
    padding: 0;
  }
  .dialog::backdrop { background: rgba(0,0,0,.55); }
  .dialogBody { display: grid; }
  .dialogGrid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .iconButton { width: 34px; padding: 0; }
  .emptyPanel {
    max-width: 520px;
    margin: 0 auto;
  }
  .emptyPanel p { padding: 14px; color: var(--muted); line-height: 1.45; }
  [hidden] { display: none !important; }
  @media (max-width: 980px) {
    .workspace, .metrics { grid-template-columns: 1fr; }
    .workspace { grid-template-rows: none; overflow: auto; }
    .controlsPanel, .shoppingPanel { grid-column: auto; grid-row: auto; }
    .nutritionPanel { overflow: auto; }
  }
  @media (max-width: 620px) {
    .topbar { align-items: stretch; flex-direction: column; }
    .guideStrip { justify-content: start; }
    .formGrid, .dialogGrid, .foodControls { grid-template-columns: 1fr; }
  }
"#;
