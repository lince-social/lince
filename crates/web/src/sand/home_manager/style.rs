pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #101316;
    --surface: #171d22;
    --surface-soft: #1d252b;
    --surface-strong: #25303a;
    --line: rgba(255, 255, 255, 0.11);
    --line-strong: rgba(255, 255, 255, 0.2);
    --text: #edf2f1;
    --muted: #9ba8a7;
    --green: #6ee7a8;
    --cyan: #73d5ee;
    --yellow: #e6c56e;
    --red: #ff8b91;
    --violet: #b59cff;
    --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
  }

  * { box-sizing: border-box; }

  html,
  body {
    min-height: 100%;
    margin: 0;
    background: transparent;
  }

  body {
    min-height: 100vh;
    background: var(--bg);
    color: var(--text);
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  }

  button,
  input,
  select {
    font: inherit;
  }

  .homeManager {
    min-height: 100vh;
    padding: 14px;
    display: grid;
    grid-template-rows: auto auto auto minmax(0, 1fr);
    gap: 12px;
  }

  .hero,
  .toolbar,
  .panel,
  .summaryTile {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
  }

  .hero {
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    gap: 14px;
    padding: 16px;
  }

  .eyebrow,
  .label,
  .meta {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    margin-top: 5px;
    font-size: clamp(1.35rem, 5vw, 2rem);
    line-height: 1.05;
    letter-spacing: 0;
  }

  h2 {
    font-size: 0.9rem;
    letter-spacing: 0;
  }

  .lede {
    margin-top: 8px;
    max-width: 58ch;
    color: var(--muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .generationCard {
    min-width: 152px;
    display: grid;
    align-content: center;
    gap: 4px;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-soft);
  }

  .generationCard strong {
    color: var(--green);
    font-family: var(--mono);
    font-size: 1.35rem;
  }

  .generationCard span:last-child {
    color: var(--muted);
    font-size: 0.78rem;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px;
    padding: 10px;
  }

  .button,
  .iconButton,
  input,
  select {
    min-height: 36px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface-soft);
    color: var(--text);
  }

  .button,
  .iconButton {
    cursor: pointer;
    padding: 0 12px;
  }

  .button:hover,
  .iconButton:hover {
    border-color: var(--line-strong);
    background: var(--surface-strong);
  }

  .buttonPrimary {
    border-color: rgba(110, 231, 168, 0.36);
    background: rgba(110, 231, 168, 0.13);
    color: var(--green);
    font-weight: 800;
  }

  .iconButton {
    width: 36px;
    padding: 0;
    font-size: 1.15rem;
    line-height: 1;
  }

  .searchWrap,
  .selectWrap {
    display: grid;
    gap: 4px;
  }

  .searchWrap {
    flex: 1 1 220px;
  }

  input,
  select {
    width: 100%;
    padding: 0 10px;
  }

  .summaryGrid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }

  .summaryTile {
    padding: 12px;
    display: grid;
    gap: 6px;
  }

  .summaryTile strong {
    font-size: 1.1rem;
  }

  .workspace {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(250px, 1fr) minmax(250px, 1fr);
    grid-template-rows: minmax(220px, 1fr) minmax(200px, 0.9fr);
    gap: 12px;
  }

  .panel {
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    overflow: hidden;
  }

  .packagePanel {
    grid-template-rows: auto auto minmax(0, 1fr);
  }

  .panelHeader {
    min-height: 46px;
    padding: 10px 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-bottom: 1px solid var(--line);
  }

  .moduleList,
  .packageList,
  .serviceList,
  .activationList {
    min-height: 0;
    overflow: auto;
    padding: 10px;
  }

  .moduleItem,
  .packageItem,
  .serviceItem,
  .activationItem {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.025);
  }

  .moduleItem,
  .serviceItem,
  .activationItem {
    display: grid;
    gap: 7px;
    padding: 10px;
  }

  .moduleItem + .moduleItem,
  .serviceItem + .serviceItem,
  .activationItem + .activationItem {
    margin-top: 8px;
  }

  .moduleTop,
  .serviceTop,
  .packageItem {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .moduleName,
  .serviceName,
  .packageName {
    min-width: 0;
    overflow-wrap: anywhere;
    font-weight: 800;
    font-size: 0.86rem;
  }

  .modulePath,
  .serviceCopy,
  .activationCopy {
    color: var(--muted);
    font-size: 0.75rem;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .switch {
    position: relative;
    width: 42px;
    height: 24px;
    flex: 0 0 auto;
  }

  .switch input {
    position: absolute;
    opacity: 0;
    inset: 0;
  }

  .switch span {
    display: block;
    width: 100%;
    height: 100%;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: #2a3037;
  }

  .switch span::after {
    content: "";
    position: absolute;
    top: 4px;
    left: 4px;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    background: var(--muted);
    transition: transform 150ms ease, background 150ms ease;
  }

  .switch input:checked + span {
    border-color: rgba(110, 231, 168, 0.42);
    background: rgba(110, 231, 168, 0.17);
  }

  .switch input:checked + span::after {
    transform: translateX(18px);
    background: var(--green);
  }

  .packageForm {
    display: flex;
    gap: 8px;
    padding: 10px;
    border-bottom: 1px solid var(--line);
  }

  .packageForm input {
    min-width: 0;
    flex: 1 1 auto;
  }

  .packageList {
    display: flex;
    align-content: flex-start;
    flex-wrap: wrap;
    gap: 8px;
  }

  .packageItem {
    min-height: 34px;
    padding: 6px 8px 6px 10px;
    background: rgba(115, 213, 238, 0.08);
  }

  .removeButton {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .removeButton:hover {
    color: var(--red);
    background: rgba(255, 139, 145, 0.1);
  }

  .statusPill {
    border-radius: 999px;
    padding: 4px 8px;
    background: var(--surface-soft);
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.68rem;
    font-weight: 800;
  }

  .statusPill[data-tone="ok"] { color: var(--green); }
  .statusPill[data-tone="warn"] { color: var(--yellow); }
  .statusPill[data-tone="off"] { color: var(--red); }
  .statusPill[data-tone="info"] { color: var(--cyan); }

  .activationList {
    margin: 0;
    list-style: none;
    counter-reset: activation;
  }

  .activationItem {
    position: relative;
    padding-left: 42px;
    counter-increment: activation;
  }

  .activationItem::before {
    content: counter(activation);
    position: absolute;
    left: 10px;
    top: 10px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 999px;
    background: var(--surface-soft);
    color: var(--cyan);
    font-family: var(--mono);
    font-size: 0.72rem;
    font-weight: 800;
  }

  @media (max-width: 760px) {
    .hero,
    .toolbar {
      align-items: stretch;
      flex-direction: column;
    }

    .summaryGrid,
    .workspace {
      grid-template-columns: 1fr;
    }

    .workspace {
      grid-template-rows: none;
    }

    .panel {
      min-height: 240px;
    }
  }
"#;
