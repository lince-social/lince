pub(crate) const INLINE_STYLES: &[&str] = &[r#"
:root {
  color-scheme: dark;
  --bg: #111412;
  --panel: #181d1a;
  --panel-soft: #202720;
  --line: rgba(255,255,255,.13);
  --line-strong: rgba(255,255,255,.24);
  --text: #edf3ee;
  --muted: #99a69d;
  --accent: #9ddfbd;
  --accent-soft: rgba(157,223,189,.13);
  --danger: #ff9da8;
  --warn: #efd287;
  --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
}

* { box-sizing: border-box; }
html, body { margin: 0; min-height: 100%; background: transparent; }
body {
  min-height: 100vh;
  color: var(--text);
  background: var(--bg);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
}
button, input, select { font: inherit; }

.accessApp {
  min-height: 100vh;
  padding: 12px;
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr);
  gap: 10px;
}

.topbar, .panel, .modal {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}

.topbar {
  padding: 14px;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

h1, h2 { margin: 0; letter-spacing: 0; }
h1 { margin-top: 3px; font-size: 1.05rem; }
h2 { font-size: .92rem; }
.eyebrow {
  color: var(--muted);
  font: 700 .67rem var(--mono);
  text-transform: uppercase;
  letter-spacing: 0;
}
.topActions, .panelHeader, .modalActions, .userMeta, .roleMeta {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.topActions { justify-content: flex-end; }
.split { justify-content: space-between; }

.status, .pill {
  min-height: 28px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 0 10px;
  color: var(--muted);
  background: rgba(255,255,255,.03);
  font: 700 .7rem var(--mono);
}
.status {
  border-radius: 8px;
  padding: 8px 10px;
}
.status[data-tone="ok"] { color: var(--accent); border-color: rgba(157,223,189,.35); }
.status[data-tone="error"] { color: var(--danger); border-color: rgba(255,157,168,.36); }
.status[data-tone="warn"] { color: var(--warn); border-color: rgba(239,210,135,.36); }

.button, .iconButton, .field {
  min-height: 34px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: var(--panel-soft);
  color: var(--text);
}
.button { padding: 0 11px; cursor: pointer; }
.iconButton {
  width: 34px;
  padding: 0;
  cursor: pointer;
}
.button:hover, .iconButton:hover { border-color: var(--line-strong); }
.buttonPrimary {
  color: var(--accent);
  background: var(--accent-soft);
  border-color: rgba(157,223,189,.35);
  font-weight: 700;
}
.buttonDanger { color: var(--danger); }
.field {
  width: 100%;
  padding: 0 10px;
}

.layout {
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(190px, .8fr) minmax(280px, 1.4fr) minmax(240px, 1fr);
  gap: 10px;
}
.panel {
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
}
.panelHeader {
  padding: 12px;
  border-bottom: 1px solid var(--line);
  justify-content: space-between;
}

.roleList, .userList, .permissionGrid {
  min-height: 0;
  overflow: auto;
  padding: 10px;
}
.roleCard, .userCard, .permissionItem {
  border: 1px solid var(--line);
  border-radius: 8px;
  background: rgba(255,255,255,.025);
}
.roleCard {
  width: 100%;
  padding: 10px;
  text-align: left;
  color: var(--text);
  cursor: pointer;
}
.roleCard + .roleCard, .userCard + .userCard, .permissionItem + .permissionItem { margin-top: 8px; }
.roleCard.isActive { border-color: rgba(157,223,189,.45); background: var(--accent-soft); }
.roleName, .userName, .permissionName { font-weight: 700; }
.roleMeta, .userMeta, .permissionDescription {
  margin-top: 4px;
  color: var(--muted);
  font-size: .76rem;
}

.permissionGrid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  align-content: start;
  gap: 8px;
}
.permissionItem {
  margin: 0;
  padding: 10px;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 8px;
  align-items: start;
}
.permissionItem input { margin-top: 2px; }
.permissionSubject {
  color: var(--accent);
  font: 700 .68rem var(--mono);
}

.userCard {
  padding: 10px;
  display: grid;
  gap: 8px;
}

.empty {
  padding: 14px;
  color: var(--muted);
  border: 1px dashed var(--line);
  border-radius: 8px;
}

.modalBackdrop {
  position: fixed;
  inset: 0;
  z-index: 10;
  display: grid;
  place-items: center;
  padding: 16px;
  background: rgba(0,0,0,.58);
}
.modalBackdrop[hidden] { display: none; }
.modal {
  width: min(440px, 100%);
  padding: 14px;
}
.modalHeader {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
}
.form { display: grid; gap: 12px; margin-top: 12px; }
.formFields { display: grid; gap: 10px; }
.form label { display: grid; gap: 5px; color: var(--muted); font-size: .78rem; }
.modalActions { justify-content: flex-end; }

@media (max-width: 820px) {
  .layout { grid-template-columns: 1fr; }
  .panel { min-height: 260px; }
}
"#];
