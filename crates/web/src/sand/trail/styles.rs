pub(crate) const INLINE_STYLES: &[&str] = &[r#"
:root {
    color-scheme: dark;
    --bg: #071017;
    --panel: rgba(14, 19, 24, 0.96);
    --panel-soft: rgba(17, 23, 30, 0.96);
    --line: rgba(255, 255, 255, 0.08);
    --line-strong: rgba(255, 255, 255, 0.16);
    --text: #e6edf3;
    --muted: #9aa8b6;
    --soft: #c8d1db;
    --accent: #78d7ff;
    --accent-2: #7ef0c6;
    --warn: #f2bb78;
    --danger: #ff8fa3;
    --shadow: 0 20px 55px rgba(0, 0, 0, 0.28);
}

* {
    box-sizing: border-box;
}

html, body {
    height: 100%;
    margin: 0;
    background:
        radial-gradient(circle at top left, rgba(120, 215, 255, 0.10), transparent 30%),
        radial-gradient(circle at top right, rgba(126, 240, 198, 0.08), transparent 26%),
        linear-gradient(180deg, #09131a 0%, #05080d 100%);
    color: var(--text);
    font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

body {
    overflow: hidden;
}

button, input, select {
    font: inherit;
}

[hidden] {
    display: none !important;
}

.trailApp {
    position: relative;
    height: 100%;
    padding: 10px;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    gap: 10px;
}

.hero, .panel, .section {
    border: 1px solid var(--line);
    border-radius: 16px;
    background: linear-gradient(180deg, var(--panel), rgba(8, 12, 16, 0.98));
    box-shadow: var(--shadow);
}

.hero {
    position: absolute;
    top: 10px;
    right: 10px;
    left: auto;
    z-index: 10;
    width: min(980px, calc(100% - 20px));
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    align-items: center;
    backdrop-filter: blur(16px);
}

.heroCopy, .heroMeta, .panelHead, .panelToolbar, .panelChips, .toolbarButtons, .sectionHead, .actionRow {
    display: flex;
    align-items: center;
    gap: 10px;
}

.heroCopy {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    min-width: 0;
    gap: 8px;
}

.heroMeta, .panelChips, .toolbarButtons, .panelToolbar {
    flex-wrap: wrap;
}

.heroMeta {
    justify-content: flex-end;
}

.eyebrow, .sectionLabel {
    color: var(--muted);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
}

.title, .panelTitle, .sectionTitle {
    margin: 0;
    letter-spacing: -0.03em;
}

.title {
    font-size: 1.08rem;
    font-weight: 700;
}

.panelTitle {
    font-size: 0.96rem;
    font-weight: 700;
}

.sectionTitle {
    font-size: 0.88rem;
    font-weight: 700;
}

.copy {
    margin: 0;
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.45;
}

.pill, .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 32px;
    padding: 0 11px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    text-decoration: none;
    white-space: nowrap;
}

.pill {
    font-size: 0.7rem;
    color: var(--muted);
}

.pillStatus {
    color: #d7ecff;
    border-color: rgba(120, 215, 255, 0.22);
    background: rgba(15, 32, 43, 0.72);
}

.button {
    cursor: pointer;
    transition: border-color 120ms ease, background 120ms ease, transform 120ms ease;
}

.button:hover {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.06);
}

.button:disabled {
    opacity: 0.55;
    cursor: default;
}

.buttonPrimary {
    border-color: rgba(120, 215, 255, 0.26);
    background: rgba(120, 215, 255, 0.10);
    color: #d8f3ff;
}

.buttonGhost {
    color: var(--muted);
}

.graphPanel {
    min-width: 0;
    min-height: 0;
    display: block;
    height: 100%;
    padding: 0;
}

.graphWorkspace {
    position: relative;
    min-width: 0;
    min-height: 0;
    width: 100%;
    height: 100%;
}

.graphStage {
    position: relative;
    min-width: 0;
    min-height: 0;
    width: 100%;
    height: 100%;
    border-radius: 20px;
    overflow: hidden;
    border: 1px solid var(--line);
    background:
        radial-gradient(circle at 20% 20%, rgba(120, 215, 255, 0.06), transparent 30%),
        linear-gradient(180deg, rgba(8, 12, 18, 0.96), rgba(5, 7, 12, 0.98));
}

.graphOverlay {
    position: absolute;
    z-index: 8;
}

.graphOverlay--title {
    top: 12px;
    left: 12px;
    padding: 0;
    border: 0;
    background: none;
    backdrop-filter: none;
    pointer-events: none;
}

.graphOverlay--title .title {
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--muted);
}

.graphHud--topRight {
    position: absolute;
    top: 12px;
    right: 12px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 10px;
    z-index: 9;
}

.graphHud--topRight .panelToolbar {
    padding: 0;
    border: 0;
    background: none;
    backdrop-filter: none;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
}

.graphHud--topRight .panelChips,
.graphHud--topRight .toolbarButtons {
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
}

.graphHud--topRight .pill,
.graphHud--topRight .button {
    min-height: 26px;
    padding: 0 9px;
    font-size: 0.68rem;
}

.graphHud--topRight .buttonGhost {
    color: #f2bb78;
}

.overlayDeck {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 10px;
    z-index: 9;
}

.quantityDock {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
}

.graphSvg {
    width: 100%;
    height: 100%;
    display: block;
}

.graphHint {
    position: absolute;
    left: 16px;
    bottom: 16px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    pointer-events: none;
}

.graphHintPill {
    background: rgba(8, 12, 18, 0.72);
}

.emptyState {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
}

.emptyBox {
    padding: 18px 20px;
    border: 1px dashed var(--line-strong);
    border-radius: 16px;
    background: rgba(255, 255, 255, 0.02);
    text-align: center;
    max-width: 32ch;
}

.emptyTitle {
    margin: 6px 0 4px;
    font-size: 0.98rem;
    letter-spacing: -0.03em;
}

.sidePanel {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    padding-right: 4px;
}

.section {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
}

.section--dock {
    position: relative;
    overflow: visible;
    width: 44px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    box-shadow: none;
}

.sectionDockPanel {
    position: absolute;
    right: 52px;
    bottom: 0;
    width: min(320px, calc(100vw - 80px));
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: rgba(14, 19, 24, 0.96);
    box-shadow: var(--shadow);
    overflow: hidden;
    opacity: 0;
    pointer-events: none;
    transform: translateY(8px) scale(0.98);
    transform-origin: bottom right;
    transition: transform 160ms ease, opacity 160ms ease, box-shadow 160ms ease;
}

.section--dock:hover .sectionDockPanel,
.section--dock:focus-within .sectionDockPanel,
.section--dock.is-open .sectionDockPanel {
    opacity: 1;
    pointer-events: auto;
    transform: translateY(0) scale(1);
}

.sectionDockButton {
    width: 44px;
    height: 44px;
    padding: 0;
    border-radius: 14px;
    color: var(--accent-2);
    border-color: rgba(126, 240, 198, 0.20);
    background: rgba(126, 240, 198, 0.10);
    flex: 0 0 auto;
}

.sectionDockButton::before {
    content: "";
    width: 12px;
    height: 12px;
    border: 1px solid currentColor;
    border-radius: 3px;
    opacity: 0.82;
}

.sectionDockButton:hover {
    border-color: rgba(126, 240, 198, 0.34);
    background: rgba(126, 240, 198, 0.18);
}

.sectionGrid {
    display: grid;
    gap: 10px;
}

.field, .autocompleteHost {
    display: grid;
    gap: 6px;
    position: relative;
}

.input {
    min-height: 40px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: rgba(8, 11, 18, 0.96);
    color: var(--text);
    padding: 9px 11px;
}

.input:focus {
    outline: none;
    border-color: rgba(120, 215, 255, 0.32);
    box-shadow: 0 0 0 1px rgba(120, 215, 255, 0.18);
}

.choiceFieldset {
    margin: 0;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.02);
}

.choiceGroup {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
}

.checkboxField {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.03);
    cursor: pointer;
}

.checkboxField input {
    margin: 0;
}

.selectionBox {
    padding: 12px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.02);
}

.selectionTitle {
    font-size: 0.9rem;
    font-weight: 700;
    color: var(--soft);
}

.quantityButton {
    min-height: 28px;
    padding: 0 10px;
    font-size: 0.70rem;
    transition:
        border-color 120ms ease,
        background 120ms ease,
        color 120ms ease,
        box-shadow 120ms ease,
        transform 120ms ease;
}

.quantityButton--pass {
    color: var(--accent-2);
    border-color: rgba(126, 240, 198, 0.22);
    background: rgba(126, 240, 198, 0.06);
}

.quantityButton--far {
    color: var(--soft);
    border-color: rgba(255, 255, 255, 0.10);
    background: rgba(255, 255, 255, 0.03);
}

.quantityButton--step {
    color: var(--warn);
    border-color: rgba(242, 187, 120, 0.24);
    background: rgba(242, 187, 120, 0.08);
}

.quantityButton.is-active {
    box-shadow: 0 0 0 1px currentColor inset;
    transform: translateY(-1px);
}

.quantityButton--pass.is-active {
    background: rgba(126, 240, 198, 0.16);
}

.quantityButton--far.is-active {
    background: rgba(255, 255, 255, 0.09);
}

.quantityButton--step.is-active {
    background: rgba(242, 187, 120, 0.16);
}

.resultShell {
    max-height: 220px;
    overflow: auto;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.02);
}

.resultList {
    display: grid;
    gap: 8px;
}

.resultCard {
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 10px;
    background: rgba(255, 255, 255, 0.02);
    cursor: pointer;
}

.resultCard:hover {
    border-color: var(--line-strong);
}

.resultCard.isSelected {
    border-color: rgba(126, 240, 198, 0.34);
    box-shadow: inset 0 0 0 1px rgba(126, 240, 198, 0.18);
}

.resultMeta {
    margin-top: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
}

.resultExcerpt {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 0.82rem;
    line-height: 1.45;
}

.suggestionPanel {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    z-index: 20;
    display: grid;
    gap: 6px;
    max-height: 200px;
    overflow: auto;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel-soft);
    box-shadow: var(--shadow);
}

.suggestionButton {
    width: 100%;
    text-align: left;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    padding: 10px 12px;
    cursor: pointer;
}

.suggestionButton:hover {
    border-color: var(--line-strong);
}

.suggestionMeta {
    display: block;
    margin-top: 4px;
    color: var(--muted);
    font-size: 0.84rem;
}

.node-link {
    stroke: rgba(120, 215, 255, 0.52);
    stroke-width: 1.85px;
    stroke-linecap: round;
}

.node-circle {
    stroke: rgba(6, 10, 14, 0.92);
    stroke-width: 2px;
}

.node-circle.is-selected {
    stroke: rgba(255, 255, 255, 0.92);
    stroke-width: 3px;
}

.node-label {
    fill: var(--soft);
    font-size: 12px;
    font-weight: 600;
    text-anchor: middle;
    pointer-events: none;
}

@media (max-width: 1120px) {
    .graphWorkspace {
        grid-template-columns: 1fr;
        grid-template-rows: minmax(320px, 1fr) auto;
    }

    .sidePanel {
        max-height: 48vh;
    }
}
"#];
