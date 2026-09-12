(function () {
  "use strict";
  const { icon, iconButton, toast } = window.LynxUI;

  function actions(names) {
    return `<div class="record-actions">${names.map(([name, label]) => iconButton(name, label)).join("")}</div>`;
  }

  function tools() {
    return `<div class="sand-tools"><span class="page-corner" aria-hidden="true"></span><div class="sand-tools__row">
      ${iconButton("database", "Choose Protein")}${iconButton("layout", "Change layout")}${iconButton("expand", "Expand Sand")}
      <button class="lynx-button lynx-icon-button" type="button" aria-label="Open LynxUI gallery" data-lynx-tooltip="Open LynxUI gallery" data-lynx-dialog-open="component-gallery">${icon("settings")}</button>
    </div></div>`;
  }

  function baseTools() {
    return `<div class="sand-tools base-tools"><span class="page-corner" aria-hidden="true"></span><div class="sand-tools__row">
      ${iconButton("moon", "Use light mode", { className: "theme-toggle" })}
      ${iconButton("scan", "Highlight LynxUI components", { className: "inspect-toggle", pressed: false })}
    </div></div>`;
  }

  function selectControl(name, selected, options, attributes = "") {
    const items = options.map(([value, label]) => `<button class="lynx-button" type="button" role="option" aria-selected="${value === selected}" data-lynx-select-option data-value="${value}">${label}</button>`).join("");
    const label = options.find(([value]) => value === selected)?.[1] || options[0][1];
    return `<div class="lynx-select lynx-dropdown" data-lynx-select data-value="${selected}" ${attributes}><button class="lynx-button" type="button" aria-haspopup="listbox" aria-expanded="false" data-lynx-dropdown-button><span data-lynx-select-value>${label}</span>${icon("chevronDown")}</button><div class="lynx-menu" role="listbox" hidden>${items}</div><input type="hidden" name="${name}" value="${selected}" data-lynx-select-input></div>`;
  }

  function record(title, text, state, owner, declared = false) {
    return `<li class="record lynx-shadow${declared ? " record--declared" : ""}" tabindex="0">
      ${actions([["check", "Complete record"], ["edit", "Edit record"], ["move", "Move record"]])}
      <h3>${title}</h3>${text ? `<p>${text}</p>` : ""}
      <div class="record-meta"><span class="${state.startsWith("−") ? "state-mark state-mark--need" : "state-mark state-mark--peace"}">${state}</span><span class="record-meta__end">${owner}</span></div>
    </li>`;
  }

  function componentShowcase() {
    return `<aside class="component-showcase" aria-label="LynxUI component showcase">
      <header class="lynx-toolbar component-showcase__head"><h1 class="lynx-title">LynxUI</h1><button class="lynx-button" type="button" data-lynx-dialog-open="component-gallery">All components</button></header>
      <div class="lynx-stack component-showcase__body">
        <div class="lynx-button-group"><button class="lynx-button" type="button">${icon("edit")} Edit</button><button class="lynx-button lynx-button--primary" type="button">${icon("check")} Save</button>${iconButton("trash", "Delete")}</div>
        <label class="lynx-field"><span class="lynx-label">Field</span><input class="lynx-input" value="Record title"><span class="lynx-help">Help text and errors stay with their field.</span></label>
        <div class="lynx-field"><span class="lynx-label">Select</span>${selectControl("showcase-owner", "ana", [["ana", "Ana"], ["luiz", "Luiz"]])}</div>
        <div class="lynx-row"><label class="lynx-check"><input type="checkbox" checked> Notify</label><label class="lynx-radio"><input type="radio" checked name="showcase-density"> Compact</label></div>
        <div class="lynx-row"><span class="lynx-status">Declared</span><span class="lynx-status">${icon("check")}Settled</span></div>
        <div class="lynx-callout">${icon("info")}<span>Every primitive is also shown in the full gallery.</span></div>
        <button class="lynx-button show-toast" type="button">Show toast</button>
        <details class="lynx-disclosure"><summary>Disclosure</summary><div class="lynx-box">Box, panel, list, table, menu, tabs, tooltip, and dialog are in the full gallery.</div></details>
        <div class="lynx-empty">No unreviewed components.</div>
      </div>
    </aside>`;
  }

  function recordSand() {
    return `<section class="sand record-sand" aria-label="Record Sand mock preview">
      <header class="record-sand__head"><h2>Record</h2><span class="record-sand__live" data-lynx-tooltip="Live mock data"></span>${iconButton("close", "Collapse Record")}</header>
      <article class="record-sand__record">
        <header><strong>Build LynxUI fields</strong><span>5</span>${iconButton("edit", "Edit Record")}</header>
        <dl class="record-sand__metadata"><div><dt>Slug</dt><dd>lynxui.fields</dd></div><div><dt>Quantity</dt><dd>5</dd></div><div><dt>Due</dt><dd>2026-08-05</dd></div></dl>
        <p>Native inputs with shared tokens and clear errors.</p>
        <details class="lynx-disclosure" open><summary>Work</summary><div class="lynx-row"><span>Estimate · 90 min</span><span>Elapsed · 35 min</span><button class="lynx-button lynx-button--primary" type="button">${icon("check")} Complete</button></div></details>
        <details class="lynx-disclosure" open><summary>Assignees</summary><div class="lynx-row"><span class="lynx-status">Ana</span><span class="lynx-status">Luiz</span>${iconButton("plus", "Assign person")}</div></details>
        <details class="lynx-disclosure" open><summary>Threads</summary><article class="record-sand__message"><strong>Ana</strong><time>10:12</time><p>Use the shared field primitives here first.</p></article><div class="record-sand__composer"><input class="lynx-input" aria-label="Message" placeholder="Message…"><button class="lynx-button lynx-button--primary" type="button">${icon("send")} Post</button></div></details>
      </article>${tools()}</section>`;
  }

  document.body.insertAdjacentHTML("afterbegin", `
    <main class="demo">
      <section class="sand-stack" aria-label="Seeded Sand previews">
      <section class="sand kanban-sand" aria-label="Kanban Sand mock preview"><header class="sand-preview-head"><h2>Kanban</h2><span>Mock Protein · 7 Records</span>${iconButton("plus", "Create Record")}</header><div class="kanban">
        <section class="kanban-column"><header class="column-head lynx-shadow"><h2>Backlog</h2><span>3</span>${iconButton("plus", "Create record")}</header><ol class="lynx-list record-list">
          ${record("Map the onboarding path", "Keep the first session under four minutes.", "−3", "Ana", true)}
          ${record("Review palette contrast", "Check text, focus and status pairs.", "0", "Mina")}
          ${record("Write Sand author notes", "", "−1", "Docs")}
        </ol></section>
        <section class="kanban-column"><header class="column-head lynx-shadow"><h2>In progress</h2><span>2</span>${iconButton("plus", "Create record")}</header><ol class="lynx-list record-list">
          ${record("Build LynxUI fields", "Native inputs with shared tokens and clear errors.", "5", "Luiz")}
          ${record("Per-Sand style override", "Inherit global or choose one CSS file.", "0", "Rafa", true)}
        </ol></section>
        <section class="kanban-column"><header class="column-head lynx-shadow"><h2>Done</h2><span>2</span>${iconButton("plus", "Create record")}</header><ol class="lynx-list record-list">
          ${record("Shared transport", "One connection for Protein, Actions and lanes.", "8", "Jo")}
          ${record("Record pin", "", "0", "Settled")}
        </ol></section>
      </div>${tools()}</section>

      ${recordSand()}

      <section class="sand message-sand" aria-label="Messages"><div class="messages">
        <nav class="channels lynx-shadow" aria-label="Conversations">
          <button class="channel is-active" type="button">${icon("hash")}<span>design</span></button>
          <button class="channel" type="button">${icon("hash")}<span>product</span></button>
          <button class="channel" type="button">${icon("people")}<span>Ana</span></button>
        </nav>
        <section class="conversation" aria-label="design conversation">
          <header class="conversation-head"><h2># design</h2><span>3</span>${iconButton("search", "Search messages")}</header>
          <div class="message-stream">
            <article class="message"><span class="avatar">AN</span><div><div class="message-line"><strong>Ana</strong><time>09:42</time></div><p>The interface should disappear until I need it.</p></div></article>
            <article class="message"><span class="avatar">LU</span><div><div class="message-line"><strong>Luiz</strong><time>09:47</time></div><p>Components now share one source.</p><div class="message-quote">Use LynxUI where it is the simplest option.</div></div></article>
            <article class="message"><span class="avatar">MI</span><div><div class="message-line"><strong>Mina</strong><time>10:03</time></div><p>Input padding now stays close to the letter height.</p></div></article>
          </div>
          <form class="composer">${iconButton("attach", "Attach file")}<input class="lynx-input" aria-label="Message" placeholder="Message #design" autocomplete="off">${iconButton("send", "Send message", { className: "lynx-button--primary" })}</form>
        </section>
      </div>${tools()}</section>

      <section class="sand workflow-sand inventory-sand" id="inventory" aria-label="Inventory operations">
        <div class="lynx-toolbar workflow-head"><h2 class="lynx-title">Inventory</h2><div class="lynx-button-group"><button class="lynx-button" type="button">${icon("download")} Export</button><button class="lynx-button lynx-button--primary" type="button">${icon("plus")} Receive</button></div></div>
        <div class="lynx-callout">${icon("info")}<span>Two materials need review before tomorrow's assembly run.</span></div>
        <div class="lynx-grid inventory-grid">
          <div class="lynx-panel inventory-table"><header class="lynx-title">Material balance</header><div class="lynx-panel__body"><table class="lynx-table"><thead><tr><th>Material</th><th>Available</th><th>Badge</th></tr></thead><tbody><tr><td>Cobalt sheet</td><td>18</td><td><span class="lynx-status">${icon("check")}Ready</span></td></tr><tr><td>Ice glass</td><td>−3</td><td><span class="lynx-status">Need</span></td></tr><tr><td>Gray fastener</td><td>42</td><td><span class="lynx-status">Declared</span></td></tr></tbody></table></div><footer class="lynx-row"><span class="lynx-status">3 materials</span><span>Updated 10:12</span></footer></div>
          <div class="lynx-split inventory-side" style="height: 260px">
            <div class="lynx-split__pane lynx-stack">
            <div class="lynx-dropdown"><button class="lynx-button" type="button" aria-expanded="false" data-lynx-dropdown-button>Batch actions ${icon("chevronDown")}</button><div class="lynx-menu" hidden><button class="lynx-button" type="button">Recount</button><button class="lynx-button" type="button">Move</button></div></div>
            <ul class="lynx-list"><li>Dock A · 8 crates</li><li>Dock B · waiting</li></ul>
            <div class="lynx-empty">No rejected deliveries.</div>
            </div>
            <button class="lynx-split__divider" type="button" aria-label="Resize receiving notes"></button>
            <div class="lynx-split__pane">
              <details class="lynx-disclosure" open><summary>Receiving notes</summary><p>Confirm quantities against the signed delivery. Drag the divider to give this
                section more room; close it and the split shrinks to the summary.</p></details>
            </div>
          </div>
        </div>
        ${tools()}
      </section>

      <section class="sand workflow-sand intake-sand" id="request-review" aria-label="Request intake">
        <form class="lynx-stack intake-form">
          <div class="lynx-toolbar workflow-head"><h2 class="lynx-title">Request review</h2><span class="lynx-status">Draft</span></div>
          <div class="lynx-grid intake-grid">
            <label class="lynx-field"><span class="lynx-label">Request</span><input class="lynx-input" value="Prepare field kit"><span class="lynx-help">Use a short action-oriented title.</span></label>
            <div class="lynx-field"><span class="lynx-label">Owner</span>${selectControl("owner", "ana", [["ana", "Ana"], ["luiz", "Luiz"]])}</div>
            <label class="lynx-field intake-description"><span class="lynx-label">Context</span><textarea class="lynx-textarea">Pack the tools needed for the north site.</textarea></label>
            <label class="lynx-field"><span class="lynx-label">Required date</span><span class="lynx-input-wrap"><input class="lynx-input" aria-invalid="true" aria-describedby="date-error" placeholder="YYYY-MM-DD"><span class="lynx-error-indicator" tabindex="0" role="img" aria-label="Choose a valid date." data-lynx-tooltip="Choose a valid date.">${icon("alert")}</span></span><span class="lynx-error lynx-visually-hidden" id="date-error">Choose a valid date.</span></label>
          </div>
          <div class="lynx-box choice-box"><label class="lynx-check"><input type="checkbox" checked> Notify the owner</label><div class="lynx-row"><span>Priority</span><label class="lynx-radio"><input type="radio" name="priority" checked> Normal</label><label class="lynx-radio"><input type="radio" name="priority"> Urgent</label></div></div>
          <hr class="lynx-divider">
          <div class="lynx-toolbar"><span class="lynx-help">Changes stay local until submitted.</span><div class="lynx-button-group"><button class="lynx-button" type="button">${icon("save")} Save draft</button><button class="lynx-button lynx-button--primary" type="submit">${icon("check")} Submit</button></div></div>
        </form>
        ${tools()}
      </section>
      </section>
      ${componentShowcase()}
    </main>
    ${baseTools()}

    <dialog class="lynx-dialog" id="component-gallery" aria-labelledby="gallery-title">
      <header class="lynx-toolbar lynx-dialog__head"><strong class="lynx-title" id="gallery-title">LynxUI components</strong><button class="lynx-button lynx-icon-button" type="button" aria-label="Close" data-lynx-dialog-close>${icon("close")}</button></header>
      <div class="lynx-tabs" role="tablist" data-lynx-tabs>
        <button class="lynx-button lynx-tab" type="button" role="tab" aria-selected="true" data-lynx-tab="forms">Forms</button>
        <button class="lynx-button lynx-tab" type="button" role="tab" aria-selected="false" tabindex="-1" data-lynx-tab="data">Data</button>
        <button class="lynx-button lynx-tab" type="button" role="tab" aria-selected="false" tabindex="-1" data-lynx-tab="states">States</button>
      </div>
      <section class="lynx-tab-panel lynx-grid gallery-grid" role="tabpanel" data-lynx-panel="forms">
        <div class="lynx-stack">
          <label class="lynx-field"><span class="lynx-label">Title</span><input class="lynx-input" value="Review components"><span class="lynx-help">A short Record title.</span></label>
          <label class="lynx-field"><span class="lynx-label">Description</span><textarea class="lynx-textarea">Compact by default.</textarea></label>
          <div class="lynx-field"><span class="lynx-label">Status</span>${selectControl("status", "progress", [["progress", "In progress"], ["done", "Done"]])}</div>
          <div class="lynx-field"><span class="lynx-label">Style</span>${selectControl("style", "lynx", [["lynx", "Lynx"], ["catppuccin-macchiato", "Catppuccin Macchiato"]], "data-style-select")}</div>
          <label class="lynx-field"><span class="lynx-label">Invalid field</span><span class="lynx-input-wrap"><input class="lynx-input" aria-invalid="true" aria-describedby="field-error"><span class="lynx-error-indicator" tabindex="0" role="img" aria-label="A value is required." data-lynx-tooltip="A value is required.">${icon("alert")}</span></span><span class="lynx-error lynx-visually-hidden" id="field-error">A value is required.</span></label>
        </div>
        <div class="lynx-stack"><label class="lynx-check"><input type="checkbox" checked> Notify members</label><label class="lynx-radio"><input type="radio" name="density" checked> Compact</label><label class="lynx-radio"><input type="radio" name="density"> Comfortable</label><hr class="lynx-divider"><div class="lynx-button-group"><button class="lynx-button lynx-button--primary" type="button">${icon("check")} Save</button><button class="lynx-button" type="button">${icon("close")} Cancel</button><button class="lynx-button lynx-button--danger" type="button">${icon("trash")} Delete</button></div></div>
      </section>
      <section class="lynx-tab-panel lynx-stack" role="tabpanel" data-lynx-panel="data" hidden>
        <div class="lynx-panel"><header class="lynx-title">Record quantities</header><div class="lynx-panel__body"><table class="lynx-table"><thead><tr><th>Record</th><th>Need</th><th>Badge</th></tr></thead><tbody><tr><td>Palette</td><td>−3</td><td><span class="lynx-status">Declared</span></td></tr><tr><td>Components</td><td>5</td><td><span class="lynx-status">${icon("check")}Settled</span></td></tr></tbody></table></div><footer>2 Records</footer></div>
        <ul class="lynx-list"><li>List row one</li><li>List row two</li></ul>
        <div class="lynx-empty">No archived Records.</div>
      </section>
      <section class="lynx-tab-panel lynx-stack" role="tabpanel" data-lynx-panel="states" hidden>
        <div class="lynx-row"><span class="lynx-status">Plain</span><span class="lynx-status">${icon("info")}With icon</span><span class="lynx-status">Ready</span><span class="lynx-status">Blocked</span></div>
        <div class="lynx-callout">${icon("info")}<span>Callouts explain a state without relying on color.</span></div>
        <div class="lynx-dropdown"><button class="lynx-button" type="button" aria-expanded="false" data-lynx-dropdown-button>Dropdown ${icon("chevronDown")}</button><div class="lynx-menu" hidden><button class="lynx-button" type="button">Edit</button><button class="lynx-button" type="button">Duplicate</button></div></div>
        <details class="lynx-disclosure"><summary>Disclosure</summary><p>Native details with LynxUI spacing.</p></details>
        <div class="lynx-box">Box</div>
      </section>
    </dialog>
  `);

  const root = document.documentElement;
  const toggle = document.querySelector(".theme-toggle");
  const inspectToggle = document.querySelector(".inspect-toggle");
  const styleSelect = document.querySelector("[data-style-select]");
  document.querySelector(".show-toast").addEventListener("click", () => {
    toast({ title: "LynxUI toast", body: "Dismisses after five seconds.", duration: 5000 });
  });
  function syncTheme() {
    const macchiato = root.dataset.style === "catppuccin-macchiato";
    const dark = root.dataset.mode === "dark";
    const label = macchiato ? "Catppuccin Macchiato is dark" : dark ? "Use light mode" : "Use dark mode";
    toggle.innerHTML = icon(dark ? "sun" : "moon");
    toggle.setAttribute("aria-label", label);
    toggle.dataset.lynxTooltip = label;
    toggle.setAttribute("aria-pressed", String(dark));
    toggle.disabled = macchiato;
  }
  toggle.addEventListener("click", () => {
    root.dataset.mode = root.dataset.mode === "dark" ? "light" : "dark";
    syncTheme();
  });
  window.LynxUI.setSelectValue(styleSelect, root.dataset.style);
  styleSelect.addEventListener("change", () => {
    root.dataset.style = styleSelect.dataset.value;
    if (root.dataset.style === "catppuccin-macchiato") root.dataset.mode = "dark";
    syncTheme();
  });
  let inspecting = new URLSearchParams(location.search).get("inspect") === "true";
  function syncInspector() {
    window.LynxUI.inspect(document, inspecting);
    inspectToggle.setAttribute("aria-pressed", String(inspecting));
    const label = inspecting ? "Stop highlighting LynxUI components" : "Highlight LynxUI components";
    inspectToggle.setAttribute("aria-label", label);
    inspectToggle.dataset.lynxTooltip = label;
  }
  inspectToggle.addEventListener("click", () => {
    inspecting = !inspecting;
    syncInspector();
  });
  syncTheme();
  syncInspector();

  const sands = [...document.querySelectorAll(".sand")];
  function focusSand(activeSand) {
    for (const sand of sands) sand.classList.toggle("is-focused", sand === activeSand);
  }
  for (const sand of sands) {
    sand.addEventListener("pointerenter", () => focusSand(sand));
    sand.addEventListener("focusin", () => focusSand(sand));
  }

  for (const channel of document.querySelectorAll(".channel")) channel.addEventListener("click", () => {
    for (const candidate of document.querySelectorAll(".channel")) candidate.classList.remove("is-active");
    channel.classList.add("is-active");
    document.querySelector(".conversation-head h2").textContent = channel.textContent.trim();
  });
})();
