pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

// Stage 8b rebuild: a self-contained kanban board. Columns are flex tracks, cards
// are draggable tiles. No sidepanel — record detail is delegated to the Record
// Info sand over the recordClicked ABI event (Track B of the migration).
const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #0c1117;
    --panel: rgba(17, 22, 29, 0.92);
    --panel-soft: rgba(14, 19, 25, 0.96);
    --line: rgba(120, 140, 170, 0.18);
    --ink: #e7edf5;
    --muted: #9fb0c6;
    --accent: #6ea8fe;
    --accent-soft: rgba(110, 168, 254, 0.16);
    --ok: #57d38c;
    --busy: #f5c451;
    --error: #ff6b6b;
  }

  * { box-sizing: border-box; }

  html, body {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--ink);
    font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }

  .kanbanWidget {
    display: flex;
    flex-direction: column;
    height: 100vh;
    min-height: 0;
    overflow: hidden;
  }

  .kanbanTopLine {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .kanbanTitle { font-weight: 600; letter-spacing: 0.02em; }
  .kanbanTopActions { display: flex; align-items: center; gap: 8px; }

  .kanbanStatus {
    width: 10px;
    height: 10px;
    border-radius: 999px;
    background: var(--muted);
    box-shadow: 0 0 0 3px rgba(159, 176, 198, 0.12);
  }
  .kanbanStatus[data-tone="ok"] { background: var(--ok); box-shadow: 0 0 0 3px rgba(87, 211, 140, 0.14); }
  .kanbanStatus[data-tone="busy"] { background: var(--busy); box-shadow: 0 0 0 3px rgba(245, 196, 81, 0.14); }
  .kanbanStatus[data-tone="error"] { background: var(--error); box-shadow: 0 0 0 3px rgba(255, 107, 107, 0.14); }

  .kanbanButton {
    border: 1px solid var(--line);
    background: transparent;
    color: var(--ink);
    border-radius: 8px;
    padding: 5px 10px;
    font: inherit;
    cursor: pointer;
  }
  .kanbanButton:hover { background: var(--accent-soft); border-color: var(--accent); }
  .kanbanButton--ghost { color: var(--muted); }

  .kanbanDetails {
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel-soft);
  }
  .kanbanDetailHeader {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .kanbanDetailTitle { font-weight: 600; }
  .kanbanDetailGrid { display: flex; flex-wrap: wrap; gap: 6px; }
  .kanbanPill {
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 3px 9px;
    color: var(--muted);
    font-size: 12px;
  }

  .kanbanBoard {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 10px;
    padding: 12px;
    overflow-x: auto;
    overflow-y: hidden;
    align-items: stretch;
  }

  .kanbanEmpty {
    margin: auto;
    color: var(--muted);
  }

  .kanbanColumn {
    display: flex;
    flex-direction: column;
    min-width: 200px;
    max-width: 260px;
    flex: 0 0 220px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
  }
  .kanbanColumn.isDropTarget { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft) inset; }

  .kanbanColumnHead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .kanbanColumnName { font-weight: 600; }
  .kanbanColumnCount { color: var(--muted); font-size: 12px; }

  .kanbanColumnCards {
    flex: 1;
    min-height: 24px;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .kanbanCard {
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel-soft);
    padding: 8px 10px;
    cursor: grab;
    user-select: none;
  }
  .kanbanCard:hover { border-color: var(--accent); }
  .kanbanCard.isSelected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
  .kanbanCard.isDragging { opacity: 0.5; }
  .kanbanCardTitle { font-weight: 500; word-break: break-word; }
  .kanbanCardTitle .cardTitleEditor {
    width: 100%;
    font: inherit;
    color: var(--ink);
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 6px;
    padding: 2px 4px;
  }
  .kanbanCardMeta { color: var(--muted); font-size: 11px; margin-top: 4px; display: flex; gap: 8px; }

  .kanbanCardRow { display: flex; align-items: flex-start; gap: 6px; }
  .kanbanCardRow .kanbanCardTitle { flex: 1; }
  .kanbanCardDelete {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 2px;
  }
  .kanbanCardDelete:hover { color: var(--error); }

  .kanbanColumnAdd {
    border: 1px dashed var(--line);
    background: transparent;
    color: var(--muted);
    border-radius: 8px;
    padding: 6px;
    margin: 8px;
    cursor: pointer;
    font: inherit;
  }
  .kanbanColumnAdd:hover { border-color: var(--accent); color: var(--ink); }

  .kanbanToasts {
    position: fixed;
    right: 12px;
    bottom: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    z-index: 20;
  }
  .kanbanToast {
    background: rgba(255, 107, 107, 0.16);
    border: 1px solid var(--error);
    color: var(--ink);
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 12px;
  }
"#;
