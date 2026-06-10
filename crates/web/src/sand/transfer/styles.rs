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
    padding: 12px;
  }

  .settingsPanel {
    position: fixed;
    inset: 0;
    z-index: 20;
    pointer-events: none;
  }

  .settingsPanel[data-open="true"] {
    pointer-events: auto;
  }

  .settingsBackdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.46);
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .settingsPanel[data-open="true"] .settingsBackdrop {
    opacity: 1;
  }

  .settingsDrawer {
    position: absolute;
    top: 0;
    right: 0;
    width: min(380px, 100vw);
    height: 100dvh;
    max-height: 100%;
    overflow: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    border-left: 1px solid var(--line);
    background: var(--bg);
    transform: translateX(100%);
    transition: transform 140ms ease;
  }

  .settingsPanel[data-open="true"] .settingsDrawer {
    transform: translateX(0);
  }

  .drawerPanel {
    flex: 0 0 auto;
    overflow: hidden;
  }

  .modalLayer {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: none;
    place-items: center;
    padding: 16px;
    background: rgba(0, 0, 0, 0.58);
  }

  .modalLayer[data-open="true"] {
    display: grid;
  }

  .modalCard {
    width: min(420px, 100%);
    display: grid;
    gap: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    padding: 16px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.38);
  }

  .utilityBar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 50px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    padding: 8px 10px;
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
    grid-template-columns: minmax(260px, 300px) minmax(0, 1fr);
    gap: 12px;
  }

  .transferBrowser,
  .mainColumn {
    min-width: 0;
    min-height: 0;
    display: grid;
    align-content: start;
    gap: 12px;
  }

  .mainColumn {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .transferBrowser {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .searchInput {
    min-height: 38px;
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

  .browserHead {
    display: grid;
    gap: 8px;
    padding: 8px;
    border-bottom: 1px solid var(--line);
    background: var(--panel-2);
  }

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

  .createProposalGrid {
    padding: 0;
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

  .transferList {
    min-height: 0;
    max-height: calc(100vh - 92px);
    overflow: auto;
    display: grid;
    align-content: start;
    gap: 8px;
    padding: 12px;
  }

  .transferWorkspace {
    min-height: calc(100vh - 86px);
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

  .srOnly {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
  }

  .transferHero {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, auto);
    align-items: center;
    gap: 12px;
    border: 1px solid rgba(134, 199, 255, 0.5);
    border-radius: 8px;
    background: rgba(134, 199, 255, 0.12);
    padding: 10px 12px;
  }

  .transferHero h2 {
    font-size: 1.05rem;
  }

  .transferHeroInfo {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 18px;
    flex-wrap: wrap;
  }

  .sendControls {
    justify-self: end;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: nowrap;
  }

  .transferHero select {
    width: min(280px, 32vw);
  }

  .transferParties {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    align-items: start;
    gap: 12px;
  }

  .transferParty {
    display: grid;
    align-content: start;
    gap: 8px;
  }

  .partyLabel {
    color: var(--muted);
    font-size: 0.82rem;
    font-style: italic;
  }

  .processButtons {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px;
    background: rgba(255, 255, 255, 0.025);
  }

  .processButtons button {
    min-height: 54px;
    height: 100%;
    padding: 6px 8px;
    white-space: normal;
    line-height: 1.25;
  }

  .processReady {
    border-color: rgba(143, 227, 170, 0.55);
    color: var(--ok);
    background: rgba(18, 122, 78, 0.42);
  }

  .processDone {
    border-color: rgba(143, 227, 170, 0.45);
    color: var(--ok);
    background: rgba(18, 122, 78, 0.3);
  }

  .processWaiting {
    border-color: rgba(134, 199, 255, 0.28);
    color: var(--muted);
    background: rgba(134, 199, 255, 0.08);
  }

  .processIdle {
    color: var(--muted);
    background: rgba(255, 255, 255, 0.025);
  }

  .remoteDone {
    opacity: 0.62;
  }

  .partyCard {
    position: relative;
    min-height: 0;
    display: grid;
    align-content: start;
    gap: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.025);
    padding: 16px;
  }

  .duplicateCard {
    align-content: center;
  }

  .transferParty[data-local="true"] .partyCard {
    border-color: rgba(134, 199, 255, 0.45);
    background: rgba(115, 92, 168, 0.45);
  }

  .partyMeta,
  .partyItemLine {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    color: var(--muted);
    font-size: 0.78rem;
  }

  .partyItemLine {
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: end;
  }

  .partyItemLine strong {
    display: block;
    margin-top: 4px;
    color: var(--text);
    font-size: 1rem;
  }

  .fieldLabel {
    display: block;
    color: var(--muted);
    font-size: 0.76rem;
  }

  .qtyPair {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: 12px;
    white-space: nowrap;
  }

  .organMeta {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: end;
    justify-content: space-between;
    gap: 14px;
    color: var(--muted);
  }

  .organMeta div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .organMeta span {
    font-size: 0.66rem;
    text-transform: uppercase;
  }

  .organMeta strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
    font-size: 0.86rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .editableTerms {
    display: grid;
    gap: 10px;
  }

  .partyQtyInline {
    flex: 0 0 auto;
    width: max-content;
    color: var(--muted);
    font-size: 0.78rem;
  }

  .transferDetail[data-inactive="true"] .transferHero,
  .transferDetail[data-inactive="true"] .processButtons,
  .transferDetail[data-inactive="true"] .partyCard {
    filter: grayscale(1);
    opacity: 0.62;
  }

  .transferDetail[data-inactive="true"] .processButtons:has(button:not(:disabled)) {
    filter: none;
    opacity: 1;
  }

  .transferPackageArea {
    border: 1px solid rgba(143, 227, 170, 0.42);
    border-radius: 8px;
    background: rgba(13, 135, 93, 0.2);
    padding: 12px;
  }

  .transferPackageArea > summary {
    cursor: pointer;
    color: var(--ok);
    font-weight: 700;
  }

  .transferPackageArea[open] {
    display: grid;
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
    .proposalGrid,
    .transferHero,
    .transferParties,
    .sideGrid,
    .packageGrid {
      grid-template-columns: 1fr;
    }

    .transferList {
      max-height: none;
    }

    .transferHeroInfo,
    .sendControls,
    .transferHero select {
      width: 100%;
    }

    .sendControls {
      justify-self: stretch;
      flex-wrap: wrap;
    }

    .processButtons,
    .partyMeta {
      grid-template-columns: 1fr;
    }
  }
"#];
