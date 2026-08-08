(function (global) {
  "use strict";

  const paths = Object.freeze({
    alert: '<path d="M8 2 14 14H2L8 2Z"></path><path d="M8 6v3.5M8 12h.01"></path>',
    attach: '<path transform="translate(0 2)" d="M8.2 8.8 13 4a3 3 0 0 0-4.2-4.2L3.1 5.5a4.2 4.2 0 0 0 5.9 5.9l5.2-5.2"></path>',
    check: '<path d="m3 8.5 3 3 7-7"></path>',
    chevronDown: '<path d="m3 6 5 5 5-5"></path>',
    close: '<path d="m3 3 10 10M13 3 3 13"></path>',
    database: '<ellipse cx="8" cy="3.5" rx="5.5" ry="2.2"></ellipse><path d="M2.5 3.5v4c0 1.2 2.5 2.2 5.5 2.2s5.5-1 5.5-2.2v-4M2.5 7.5v4c0 1.2 2.5 2.2 5.5 2.2s5.5-1 5.5-2.2v-4"></path>',
    download: '<path d="M8 2v8M5 7l3 3 3-3M3 13h10"></path>',
    edit: '<path d="M9.8 3.1 12.9 6.2"></path><path d="M11.3 1.9a1.45 1.45 0 0 1 2.1 2.1L5.7 11.7 3 12.4l.7-2.7 7.6-7.8Z"></path>',
    expand: '<path d="M6 2H2v4M10 2h4v4M14 10v4h-4M6 14H2v-4M2 6l4-4M10 2l4 4M14 10l-4 4M6 14l-4-4"></path>',
    hash: '<path d="M6 2 4.5 14M11.5 2 10 14M2 6h12M1.5 10h12"></path>',
    info: '<circle cx="8" cy="8" r="6"></circle><path d="M8 7v4M8 4.5h.01"></path>',
    layout: '<rect x="2" y="2" width="12" height="12"></rect><path d="M6 2v12M6 6h8"></path>',
    menu: '<path d="M2 4h12M2 8h12M2 12h12"></path>',
    moon: '<path d="M13.4 10.6A6 6 0 0 1 5.4 2.6 6 6 0 1 0 13.4 10.6Z"></path>',
    move: '<path d="M8 1v14M1 8h14M8 1 6 3M8 1l2 2M15 8l-2-2M15 8l-2 2M8 15l-2-2M8 15l2-2M1 8l2-2M1 8l2 2"></path>',
    people: '<circle cx="6" cy="5" r="2.2"></circle><path d="M2 13c.3-2.6 1.6-4 4-4s3.7 1.4 4 4M10.5 3.2a2 2 0 0 1 0 3.8M11 9c1.8.2 2.8 1.5 3 3.5"></path>',
    plus: '<path d="M8 2v12M2 8h12"></path>',
    search: '<circle cx="7" cy="7" r="4.5"></circle><path d="m10.5 10.5 3.5 3.5"></path>',
    scan: '<path d="M5 2H2v3M11 2h3v3M14 11v3h-3M5 14H2v-3"></path><rect x="5" y="5" width="6" height="6"></rect>',
    save: '<path d="M3 2h8l2 2v10H3V2Z"></path><path d="M5 2v4h5V2M5 14V9h6v5"></path>',
    send: '<path d="m2 2 12 6-12 6 2-6-2-6ZM4 8h10"></path>',
    settings: '<circle cx="8" cy="8" r="2.2"></circle><path d="M6.7 1.7h2.6l.5 1.8 1.4.8 1.8-.5 1.3 2.3-1.3 1.3v1.2l1.3 1.3-1.3 2.3-1.8-.5-1.4.8-.5 1.8H6.7l-.5-1.8-1.4-.8-1.8.5-1.3-2.3L3 8.6V7.4L1.7 6.1 3 3.8l1.8.5 1.4-.8.5-1.8Z"></path>',
    sun: '<circle cx="8" cy="8" r="2.7"></circle><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6 13 13M13 3l-1.4 1.4M4.4 11.6 3 13"></path>',
    trash: '<path d="M3 4h10M6 4V2h4v2M4 4l.7 10h6.6L12 4M6.5 7v4M9.5 7v4"></path>',
    user: '<circle cx="8" cy="5" r="2.5"></circle><path d="M3 14c.4-3 2-4.5 5-4.5s4.6 1.5 5 4.5"></path>',
  });

  function escapeAttribute(value) {
    return String(value).replace(/[&"<>]/g, (character) => ({
      "&": "&amp;", '"': "&quot;", "<": "&lt;", ">": "&gt;",
    })[character]);
  }

  function icon(name, options = {}) {
    const label = options.label ? ` role="img" aria-label="${escapeAttribute(options.label)}"` : ' aria-hidden="true"';
    return `<svg class="lynx-icon" viewBox="-1 -1 18 18"${label}>${paths[name] || ""}</svg>`;
  }

  function iconButton(name, label, options = {}) {
    const className = ["lynx-button", "lynx-icon-button", options.className || ""].filter(Boolean).join(" ");
    const pressed = options.pressed === undefined ? "" : ` aria-pressed="${Boolean(options.pressed)}"`;
    return `<button class="${escapeAttribute(className)}" type="button" aria-label="${escapeAttribute(label)}" data-lynx-tooltip="${escapeAttribute(label)}"${pressed}>${icon(name)}</button>`;
  }

  function toast(options = {}) {
    let host = document.querySelector(".lynx-toast-host");
    if (!host) {
      host = document.createElement("div");
      host.className = "lynx-toast-host lynx-ui";
      host.setAttribute("aria-live", "polite");
      document.body.appendChild(host);
    }
    const clickable = typeof options.onClick === "function";
    const element = document.createElement(clickable ? "button" : "div");
    element.className = "lynx-toast";
    if (clickable) element.type = "button";
    const title = document.createElement("strong");
    title.textContent = options.title || "Notification";
    const body = document.createElement("span");
    body.textContent = options.body || "";
    element.append(title, body);
    if (clickable) element.addEventListener("click", options.onClick);
    host.appendChild(element);
    const dismiss = () => {
      element.remove();
      if (!host.childElementCount) host.remove();
    };
    window.setTimeout(dismiss, Math.max(0, Number(options.duration) || 5000));
    return Object.freeze({ element, dismiss });
  }

  function setTabs(tab) {
    const list = tab.closest("[data-lynx-tabs]");
    if (!list) return;
    const name = tab.dataset.lynxTab;
    for (const candidate of list.querySelectorAll("[data-lynx-tab]")) {
      const selected = candidate === tab;
      candidate.setAttribute("aria-selected", String(selected));
      candidate.tabIndex = selected ? 0 : -1;
    }
    const scope = list.parentElement;
    for (const panel of scope.querySelectorAll("[data-lynx-panel]")) {
      panel.hidden = panel.dataset.lynxPanel !== name;
    }
  }

  function setSelectValue(select, value) {
    if (!select) return;
    const options = [...select.querySelectorAll("[data-lynx-select-option]")];
    const option = options.find((candidate) => candidate.dataset.value === String(value)) || options[0];
    if (!option) return;
    select.dataset.value = option.dataset.value;
    const valueLabel = select.querySelector("[data-lynx-select-value]");
    if (valueLabel) valueLabel.textContent = option.textContent.trim();
    const input = select.querySelector("[data-lynx-select-input]");
    if (input) input.value = option.dataset.value;
    for (const candidate of options) candidate.setAttribute("aria-selected", String(candidate === option));
  }

  // ── Combobox: a live-filtered autocomplete, instantiated per element
  // (unlike the delegated-click components above, its option list is
  // usually Protein-backed and changes at query time, so each instance
  // keeps its own small bit of state). Record's predicate/object inputs and
  // Kanban's column/concept pickers are both this with a different
  // `getOptions`.
  function combobox(container, config = {}) {
    container.classList.add("lynx-combobox");
    const input = container.querySelector("input") || container.querySelector("[data-lynx-combobox-input]");
    let menu = container.querySelector("[data-lynx-combobox-menu]");
    if (!menu) {
      menu = document.createElement("div");
      menu.className = "lynx-menu";
      menu.setAttribute("data-lynx-combobox-menu", "");
      menu.setAttribute("role", "listbox");
      menu.hidden = true;
      container.appendChild(menu);
    }
    input.setAttribute("role", "combobox");
    input.setAttribute("aria-expanded", "false");
    input.setAttribute("aria-autocomplete", "list");
    if (!input.hasAttribute("autocomplete")) input.setAttribute("autocomplete", "off");

    const getLabel = config.getLabel || ((option) => String(option));
    const getValue = config.getValue || getLabel;
    const limit = config.limit || 20;
    let current = [];
    let active = -1;

    function close() {
      menu.hidden = true;
      input.setAttribute("aria-expanded", "false");
      active = -1;
    }

    function highlight(index) {
      for (const button of menu.querySelectorAll("[data-lynx-combobox-index]")) {
        delete button.dataset.lynxComboboxActive;
      }
      active = index;
      const button = menu.querySelector(`[data-lynx-combobox-index="${active}"]`);
      if (button) {
        button.dataset.lynxComboboxActive = "true";
        button.scrollIntoView({ block: "nearest" });
      }
    }

    function select(option) {
      input.value = getLabel(option);
      close();
      config.onSelect?.(option, getValue(option));
    }

    function open(query) {
      const source = typeof config.getOptions === "function" ? config.getOptions(query) : (config.options || []);
      const needle = (query || "").toLowerCase();
      current = (needle
        ? source.filter((option) => getLabel(option).toLowerCase().includes(needle))
        : source
      ).slice(0, limit);
      menu.innerHTML = "";
      if (!current.length) {
        const empty = document.createElement("div");
        empty.className = "lynx-combobox__empty";
        empty.textContent = config.emptyLabel || "No matches";
        menu.appendChild(empty);
        active = -1;
      } else {
        current.forEach((option, index) => {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "lynx-button lynx-combobox__option";
          button.setAttribute("role", "option");
          button.textContent = getLabel(option);
          button.dataset.lynxComboboxIndex = String(index);
          // Selecting with the pointer must not steal focus from the input
          // before the click fires, or the click never lands.
          button.addEventListener("mousedown", (event) => event.preventDefault());
          button.addEventListener("click", () => select(option));
          menu.appendChild(button);
        });
        active = 0;
        highlight(0);
      }
      menu.hidden = false;
      input.setAttribute("aria-expanded", "true");
    }

    input.addEventListener("input", () => open(input.value));
    input.addEventListener("focus", () => open(input.value));
    input.addEventListener("keydown", (event) => {
      if (menu.hidden) {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); open(input.value); }
        return;
      }
      if (event.key === "ArrowDown") { event.preventDefault(); highlight(Math.min(active + 1, current.length - 1)); }
      else if (event.key === "ArrowUp") { event.preventDefault(); highlight(Math.max(active - 1, 0)); }
      else if (event.key === "Enter") { if (active >= 0 && current[active]) { event.preventDefault(); select(current[active]); } }
      else if (event.key === "Escape") { close(); }
    });
    // A pointerdown on an option fires before this blur, so the click above
    // still lands; the delay just outlives that pointerdown/click pair.
    input.addEventListener("blur", () => window.setTimeout(close, 120));

    return Object.freeze({ close, refresh: () => open(input.value) });
  }

  // ── Token picker: a combobox that ADDS to a list of removable `.lynx-
  // status` badges instead of replacing the input's value — assignees,
  // selected assertions, thread predicates. `config.getSelected()` is
  // re-read on every `refresh()`, so the caller stays the source of truth;
  // this only renders it and wires add/remove.
  function tokens(container, config = {}) {
    container.classList.add("lynx-tokens");
    let comboWrap = container.querySelector("[data-lynx-tokens-input]");
    let input;
    if (!comboWrap) {
      comboWrap = document.createElement("span");
      comboWrap.setAttribute("data-lynx-tokens-input", "");
      input = document.createElement("input");
      input.type = "text";
      input.className = "lynx-input";
      if (config.placeholder) input.placeholder = config.placeholder;
      comboWrap.appendChild(input);
      container.appendChild(comboWrap);
    } else {
      input = comboWrap.querySelector("input");
    }

    function renderBadges() {
      for (const node of [...container.children]) {
        if (node !== comboWrap) node.remove();
      }
      const selected = config.getSelected ? config.getSelected() : [];
      for (const item of selected) {
        const badge = document.createElement("span");
        badge.className = "lynx-status";
        const label = config.getLabel ? config.getLabel(item) : String(item);
        const text = document.createElement("span");
        text.textContent = label;
        badge.appendChild(text);
        const remove = document.createElement("button");
        remove.type = "button";
        remove.setAttribute("aria-label", `Remove ${label}`);
        remove.innerHTML = icon("close");
        remove.addEventListener("click", () => config.onRemove?.(item));
        badge.appendChild(remove);
        container.insertBefore(badge, comboWrap);
      }
    }

    const control = combobox(comboWrap, {
      getOptions: config.getOptions,
      options: config.options,
      getLabel: config.getLabel,
      getValue: config.getValue,
      limit: config.limit,
      emptyLabel: config.emptyLabel,
      onSelect: (option, value) => {
        input.value = "";
        config.onAdd?.(option, value);
        renderBadges();
      },
    });
    renderBadges();
    return Object.freeze({ refresh: renderBadges, comboboxControl: control });
  }

  // ── Duration: minutes underneath, "1h 30m" is how a person thinks about
  // it. Record's work estimate/worklog values own the minute number; this
  // only formats it and reads a compact `Xh Ym` editor back into minutes.
  function formatDuration(totalMinutes) {
    const minutes = Math.max(0, Math.round(Number(totalMinutes) || 0));
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (!hours) return `${rest}m`;
    if (!rest) return `${hours}h`;
    return `${hours}h ${rest}m`;
  }

  function parseDuration(text) {
    const source = String(text || "").trim().toLowerCase();
    if (!source) return 0;
    if (/^\d+$/.test(source)) return parseInt(source, 10); // a bare number is minutes
    let minutes = 0;
    let matched = false;
    for (const match of source.matchAll(/(\d+(?:\.\d+)?)\s*(h|hr|hour|hours|m|min|minute|minutes)/g)) {
      matched = true;
      const value = parseFloat(match[1]);
      if (match[2].startsWith("h")) minutes += value * 60;
      else minutes += value;
    }
    return matched ? Math.round(minutes) : 0;
  }

  // ── Compact absolute/relative date-time display. Not a locale/timezone
  // library — Record work metadata and Kanban card metadata only need "how
  // do I show one instant compactly," which `Intl` already answers.
  function formatDateTime(iso, options = {}) {
    if (!iso) return "";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return "";
    if (options.relative) {
      const diffMs = date.getTime() - Date.now();
      const diffMinutes = Math.round(diffMs / 60000);
      const divisions = [
        [60, "minute"], [24, "hour"], [7, "day"], [4.345, "week"], [12, "month"], [Infinity, "year"],
      ];
      let value = diffMinutes;
      let unit = "minute";
      for (const [amount, nextUnit] of divisions) {
        if (Math.abs(value) < amount) { unit = nextUnit; break; }
        value = Math.round(value / amount);
        unit = nextUnit;
      }
      try {
        return new Intl.RelativeTimeFormat(undefined, { numeric: "auto" }).format(value, unit);
      } catch (_) {
        return date.toLocaleString();
      }
    }
    const sameDay = date.toDateString() === new Date().toDateString();
    return sameDay
      ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
      : `${date.toLocaleDateString()} ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  }

  // ── File attachment: picker trigger + upload/busy state + attachment row
  // + remove. Record owns message-attachment semantics (what an attachment
  // MEANS); this only owns the row's shape and busy/remove wiring.
  function attachmentRow(name, options = {}) {
    const row = document.createElement("div");
    row.className = "lynx-attach__row";
    if (options.busy) row.dataset.lynxAttachBusy = "true";
    const nameEl = document.createElement("span");
    nameEl.className = "lynx-attach__name";
    nameEl.textContent = name;
    row.appendChild(nameEl);
    if (options.busy) {
      const spinner = document.createElement("span");
      spinner.className = "lynx-spinner";
      spinner.setAttribute("role", "status");
      spinner.setAttribute("aria-label", "Uploading");
      row.appendChild(spinner);
    } else if (typeof options.onRemove === "function") {
      const remove = iconButton("close", options.removeLabel || "Remove attachment");
      const wrap = document.createElement("div");
      wrap.innerHTML = remove;
      const button = wrap.firstElementChild;
      button.addEventListener("click", options.onRemove);
      row.appendChild(button);
    }
    return row;
  }

  // ── Destructive confirmation: a composition of the existing `.lynx-
  // dialog`, not a new component — Kanban bulk deletion and Record hard
  // deletion both want "are you sure, here is what breaks," never a bare
  // confirm(). Returns a Promise<boolean>; Escape and the cancel button both
  // resolve false, never leave the caller hanging.
  function confirmDialog(options = {}) {
    return new Promise((resolve) => {
      const dialog = document.createElement("dialog");
      dialog.className = "lynx-dialog";
      const head = document.createElement("div");
      head.className = "lynx-dialog__head lynx-box";
      const heading = document.createElement("h2");
      heading.className = "lynx-title";
      heading.textContent = options.title || "Are you sure?";
      head.appendChild(heading);
      const body = document.createElement("div");
      body.className = "lynx-box";
      body.textContent = options.body || "This cannot be undone.";
      const actions = document.createElement("div");
      actions.className = "lynx-toolbar";
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.className = "lynx-button";
      cancel.textContent = options.cancelLabel || "Cancel";
      const confirm = document.createElement("button");
      confirm.type = "button";
      confirm.className = `lynx-button ${options.danger === false ? "lynx-button--primary" : "lynx-button--danger"}`;
      confirm.textContent = options.confirmLabel || "Delete";
      actions.append(cancel, confirm);
      dialog.append(head, body, actions);
      document.body.appendChild(dialog);
      function done(result) {
        dialog.close();
        dialog.remove();
        resolve(result);
      }
      cancel.addEventListener("click", () => done(false));
      confirm.addEventListener("click", () => done(true));
      dialog.addEventListener("cancel", () => done(false)); // native Escape handling
      dialog.showModal();
      confirm.focus();
    });
  }

  const componentSelectors = Object.freeze([
    [".lynx-icon", "Icon"],
    [".lynx-icon-button", "Icon button"],
    [".lynx-tab", "Tab"],
    [".lynx-button-group", "Button group"],
    [".lynx-button", "Button"],
    [".lynx-title", "Title"],
    [".lynx-field", "Field"],
    [".lynx-label", "Label"],
    [".lynx-input", "Input"],
    [".lynx-textarea", "Textarea"],
    [".lynx-select", "Select"],
    [".lynx-number", "Number"],
    [".lynx-check", "Checkbox"],
    [".lynx-toggle", "Toggle"],
    [".lynx-radio", "Radio"],
    [".lynx-help", "Help text"],
    [".lynx-error", "Error text"],
    [".lynx-error-indicator", "Error indicator"],
    [".lynx-box", "Box"],
    [".lynx-panel", "Panel"],
    [".lynx-stack", "Stack"],
    [".lynx-row", "Row"],
    [".lynx-grid", "Grid"],
    [".lynx-toolbar", "Toolbar"],
    [".lynx-divider", "Divider"],
    [".lynx-split__divider", "Split divider"],
    [".lynx-split", "Split"],
    [".lynx-status", "Badge"],
    [".lynx-callout", "Callout"],
    [".lynx-empty", "Empty state"],
    [".lynx-table", "Table"],
    [".lynx-list", "List"],
    [".lynx-dropdown", "Dropdown"],
    [".lynx-menu", "Menu"],
    [".lynx-dialog", "Dialog"],
    [".lynx-tabs", "Tabs"],
    [".lynx-tab-panel", "Tab panel"],
    [".lynx-disclosure", "Disclosure"],
    [".lynx-toast", "Toast"],
    [".lynx-combobox", "Combobox"],
    [".lynx-tokens", "Token picker"],
    [".lynx-meta", "Metadata list"],
    [".lynx-duration", "Duration field"],
    [".lynx-attach", "Attachment"],
    [".lynx-attach__row", "Attachment row"],
    [".lynx-spinner", "Spinner"],
    [".lynx-progress", "Progress"],
  ]);

  function inspect(root = document, enabled = true) {
    document.documentElement.dataset.lynxInspect = String(enabled);
    for (const element of root.querySelectorAll('[class*="lynx-"]')) {
      const match = componentSelectors.find(([selector]) => element.matches(selector));
      if (match) element.dataset.lynxComponent = match[1];
    }
  }

  const inspectorTooltip = document.createElement("div");
  inspectorTooltip.className = "lynx-inspector-tooltip";
  inspectorTooltip.hidden = true;
  document.addEventListener("pointerover", (event) => {
    if (document.documentElement.dataset.lynxInspect !== "true") return;
    const target = event.target.closest?.("[data-lynx-component]");
    if (!target) return;
    const box = target.getBoundingClientRect();
    inspectorTooltip.textContent = `LynxUI · ${target.dataset.lynxComponent}`;
    inspectorTooltip.hidden = false;
    document.body.append(inspectorTooltip);
    const tooltipBox = inspectorTooltip.getBoundingClientRect();
    inspectorTooltip.style.left = `${Math.max(2, Math.min(innerWidth - tooltipBox.width - 2, box.left))}px`;
    inspectorTooltip.style.top = `${Math.max(2, box.top - tooltipBox.height - 2)}px`;
  });
  document.addEventListener("pointerout", (event) => {
    if (!event.relatedTarget?.closest?.("[data-lynx-component]")) inspectorTooltip.hidden = true;
  });

  // Fixed-positioned in viewport pixels (see lynx-ui.css), so this only has
  // to keep the box inside document.documentElement's box — no ancestor
  // overflow/clip walk needed, and clamping left/top to the viewport here
  // is also what stops the tooltip from ever widening the document enough
  // to open a horizontal scrollbar.
  function alignTooltip(event) {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (!target) return;
    const targetBox = target.getBoundingClientRect();
    const viewportWidth = document.documentElement.clientWidth;
    const viewportHeight = document.documentElement.clientHeight;
    // A sand can widen the shared 180px cap for its own long explanations
    // (organ.html does) — read what will ACTUALLY render before guessing a
    // height from the wrong, narrower width, or a long tooltip's box comes
    // out shorter and wider than estimated and the fit decision below is
    // made against a box that was never going to exist.
    const cssMaxWidth = parseFloat(getComputedStyle(target, "::after").maxWidth) || 180;
    const text = target.dataset.lynxTooltip;
    const width = Math.min(cssMaxWidth, Math.max(1, viewportWidth - 4), text.length * 6 + 10);
    const height = Math.ceil(text.length * 7 / width) * 15 + 10;
    const idealLeft = targetBox.left + targetBox.width / 2 - width / 2;
    const left = Math.max(2, Math.min(viewportWidth - width - 2, idealLeft));
    const fitsAbove = targetBox.top - height - 4 >= 0;
    const idealTop = fitsAbove ? targetBox.top - height - 4 : targetBox.bottom + 4;
    const top = Math.max(2, Math.min(viewportHeight - height - 2, idealTop));
    target.style.setProperty("--lynx-tooltip-max-width", `${width}px`);
    target.style.setProperty("--lynx-tooltip-left", `${left}px`);
    target.style.setProperty("--lynx-tooltip-top", `${top}px`);
  }

  // ── One tooltip at a time ──────────────────────────────────────────────
  // The CSS shows a tooltip on :hover and on :focus-visible, and more than one
  // element can satisfy that at once: :hover matches every ancestor under the
  // pointer, and a keyboard-focused button keeps its tooltip while the pointer
  // walks away to something else. Each sand is also its own document, so a
  // hovered kanban tool cannot see the tooltip the edit popover is still
  // showing — that pair goes through the board, which is the only thing that
  // can see every frame.
  //
  // This marks the LOSERS rather than showing the winner, so a sand that pulls
  // lynx-ui.css without lynx-ui.js (instinct.html) keeps plain CSS tooltips
  // instead of losing them entirely.
  const TOOLTIP_SHOWN = "lince:tooltip-shown";
  const tooltipDocumentId = `${Date.now()}-${Math.random()}`;
  function suppressEveryTooltipExcept(target) {
    for (const other of document.querySelectorAll("[data-lynx-tooltip]")) {
      if (other === target) other.removeAttribute("data-lynx-tooltip-suppressed");
      else other.setAttribute("data-lynx-tooltip-suppressed", "");
    }
  }
  function showOnlyTooltip(event) {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (!target) return;
    suppressEveryTooltipExcept(target);
    // A sandboxed frame may not be able to reach the board at all; per-document
    // exclusivity still holds, we just lose the cross-sand half.
    try {
      window.parent?.postMessage({ type: TOOLTIP_SHOWN, id: tooltipDocumentId }, "*");
    } catch {}
  }
  window.addEventListener("message", (event) => {
    const data = event.data;
    if (!data || data.type !== TOOLTIP_SHOWN || data.id === tooltipDocumentId) return;
    suppressEveryTooltipExcept(null);
  });

  document.addEventListener("pointerover", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target) delete target.dataset.lynxTooltipDismissed;
    showOnlyTooltip(event);
    alignTooltip(event);
  });
  document.addEventListener("pointerout", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target && !target.contains(event.relatedTarget)) delete target.dataset.lynxTooltipDismissed;
  });
  document.addEventListener("focusin", (event) => {
    showOnlyTooltip(event);
    alignTooltip(event);
  });
  document.addEventListener("focusout", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target && !target.contains(event.relatedTarget)) delete target.dataset.lynxTooltipDismissed;
  });

  document.addEventListener("click", (event) => {
    event.target.closest?.("[data-lynx-tooltip]")?.setAttribute("data-lynx-tooltip-dismissed", "");
    const target = event.target.closest("[data-lynx-dialog-open], [data-lynx-dialog-close], [data-lynx-dropdown-button], [data-lynx-select-option], [data-lynx-tab], [data-lynx-number-step]");
    if (!target) return;
    if (target.hasAttribute("data-lynx-select-option")) {
      const select = target.closest("[data-lynx-select]");
      setSelectValue(select, target.dataset.value);
      const button = select?.querySelector("[data-lynx-dropdown-button]");
      const menu = select?.querySelector(".lynx-menu");
      if (menu) menu.hidden = true;
      button?.setAttribute("aria-expanded", "false");
      select?.dispatchEvent(new Event("change", { bubbles: true }));
      button?.focus();
      return;
    }
    if (target.dataset.lynxDialogOpen) document.getElementById(target.dataset.lynxDialogOpen)?.showModal();
    if (target.hasAttribute("data-lynx-dialog-close")) target.closest("dialog")?.close();
    if (target.hasAttribute("data-lynx-dropdown-button")) {
      const menu = target.nextElementSibling;
      const open = menu?.hidden === true;
      if (menu) menu.hidden = !open;
      target.setAttribute("aria-expanded", String(open));
    }
    if (target.dataset.lynxTab) setTabs(target);
    if (target.dataset.lynxNumberStep) stepNumber(target);
  });

  // ── Anchored action/context menu: the same `.lynx-menu` a `.lynx-select`/
  // `.lynx-dropdown` already shows, but positioned at a point (a right-click,
  // a "…" button not glued to a sibling menu) instead of a fixed sibling.
  // Record's links/attachments and Kanban's cards are both "a menu anchored
  // to THIS row," not a toolbar dropdown — same behavior, different anchor.
  let openAnchoredMenu = null;
  function closeAnchoredMenu() {
    if (!openAnchoredMenu) return;
    openAnchoredMenu.hidden = true;
    openAnchoredMenu.removeAttribute("data-lynx-menu-open");
    openAnchoredMenu = null;
  }
  // `items`: [{ label, onSelect, danger }]. Builds and shows a `.lynx-menu`
  // at (x, y), clamped to the viewport the same way tooltips are.
  function openMenu(x, y, items) {
    closeAnchoredMenu();
    const menu = document.createElement("div");
    menu.className = "lynx-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("data-lynx-menu-open", "");
    for (const item of items) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "lynx-button" + (item.danger ? " lynx-button--danger" : "");
      button.setAttribute("role", "menuitem");
      button.textContent = item.label;
      button.addEventListener("click", () => {
        closeAnchoredMenu();
        item.onSelect?.();
      });
      menu.appendChild(button);
    }
    menu.style.position = "fixed";
    menu.style.left = "0px";
    menu.style.top = "0px";
    document.body.appendChild(menu);
    const box = menu.getBoundingClientRect();
    const left = Math.max(2, Math.min(document.documentElement.clientWidth - box.width - 2, x));
    const top = Math.max(2, Math.min(document.documentElement.clientHeight - box.height - 2, y));
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;
    openAnchoredMenu = menu;
    return menu;
  }
  // Wires a right-click (and a Shift+F10/ContextMenu keypress, for parity
  // without a pointer) on `target` to open `buildItems()`'s menu at the
  // pointer/element position.
  function contextMenu(target, buildItems) {
    target.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      openMenu(event.clientX, event.clientY, buildItems());
    });
    target.addEventListener("keydown", (event) => {
      if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
      event.preventDefault();
      const box = target.getBoundingClientRect();
      openMenu(box.left, box.bottom, buildItems());
    });
  }
  document.addEventListener("pointerdown", (event) => {
    if (openAnchoredMenu && !openAnchoredMenu.contains(event.target)) closeAnchoredMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && openAnchoredMenu) closeAnchoredMenu();
  });

  // ── Number: the up/down affordance for a `.lynx-number`'s input.
  //
  // `stepUp`/`stepDown` throw on a non-numeric current value or a step that
  // would carry it past `min`/`max`, so the input is left exactly as it was
  // rather than silently doing nothing useful.
  function stepNumber(button) {
    const input = button.closest(".lynx-number")?.querySelector(".lynx-input");
    if (!input || input.disabled || input.readOnly) return;
    const direction = button.dataset.lynxNumberStep === "-1" ? -1 : 1;
    try {
      if (direction > 0) input.stepUp();
      else input.stepDown();
    } catch (_) {
      return;
    }
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function numbers(root = document) {
    for (const input of root.querySelectorAll(".lynx-number > .lynx-input:not([type='number'])")) {
      input.type = "number";
    }
    for (const wrap of root.querySelectorAll(".lynx-number")) {
      if (wrap.querySelector(".lynx-number__steps")) continue;
      const steps = document.createElement("span");
      steps.className = "lynx-number__steps";
      for (const step of [1, -1]) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "lynx-number__step";
        button.tabIndex = -1;
        button.dataset.lynxNumberStep = String(step);
        button.setAttribute("aria-hidden", "true");
        button.innerHTML = icon("chevronDown");
        steps.append(button);
      }
      wrap.append(steps);
    }
  }

  document.addEventListener("keydown", (event) => {
    const tab = event.target.closest("[data-lynx-tab]");
    if (!tab || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tabs = [...tab.closest("[data-lynx-tabs]").querySelectorAll("[data-lynx-tab]")];
    let index = tabs.indexOf(tab);
    if (event.key === "ArrowLeft") index = (index - 1 + tabs.length) % tabs.length;
    if (event.key === "ArrowRight") index = (index + 1) % tabs.length;
    if (event.key === "Home") index = 0;
    if (event.key === "End") index = tabs.length - 1;
    event.preventDefault();
    setTabs(tabs[index]);
    tabs[index].focus();
  });

  // ── Split: two panes sharing a height, dragged apart by their divider.
  //
  // The bottom pane is the sized one and the top takes the rest, so a list
  // above keeps growing with the panel while the section below stays where
  // the user put it. Dragging writes a pixel height; the CSS default (a
  // third) applies until then.
  function splitOf(element) {
    return element?.closest?.(".lynx-split") || null;
  }

  function bottomPane(split) {
    const panes = split.querySelectorAll(":scope > .lynx-split__pane");
    return panes[panes.length - 1] || null;
  }

  // A closed disclosure in the bottom pane means there is nothing to size:
  // hold the divider open around an empty box and the list above loses a
  // third of its height for no content.
  function syncSplitCollapsed(split) {
    const pane = bottomPane(split);
    if (!pane) return;
    const disclosures = [...pane.querySelectorAll("details")];
    const collapsed = disclosures.length > 0 && disclosures.every((d) => !d.open);
    split.toggleAttribute("data-lynx-split-collapsed", collapsed);
  }

  function resizeSplit(split, height) {
    const box = split.getBoundingClientRect();
    // Both panes stay usable: neither side can be dragged out of existence.
    const clamped = Math.max(28, Math.min(height, box.height - 48));
    split.style.setProperty("--lynx-split-basis", `${Math.round(clamped)}px`);
  }

  let dragging = null;
  document.addEventListener("pointerdown", (event) => {
    const divider = event.target.closest?.(".lynx-split__divider");
    const split = splitOf(divider);
    if (!split || split.hasAttribute("data-lynx-split-collapsed")) return;
    dragging = split;
    divider.setPointerCapture?.(event.pointerId);
    event.preventDefault();
  });
  document.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    resizeSplit(dragging, dragging.getBoundingClientRect().bottom - event.clientY);
  });
  for (const done of ["pointerup", "pointercancel"]) {
    document.addEventListener(done, () => { dragging = null; });
  }

  document.addEventListener("keydown", (event) => {
    const divider = event.target.closest?.(".lynx-split__divider");
    const split = splitOf(divider);
    if (!split || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
    const pane = bottomPane(split);
    if (!pane) return;
    event.preventDefault();
    const step = event.key === "ArrowUp" ? 16 : -16;
    resizeSplit(split, pane.getBoundingClientRect().height + step);
  });

  // `toggle` does not bubble, so it is captured rather than delegated.
  document.addEventListener("toggle", (event) => {
    const split = splitOf(event.target);
    if (split) syncSplitCollapsed(split);
  }, true);

  function splits(root = document) {
    for (const split of root.querySelectorAll(".lynx-split")) syncSplitCollapsed(split);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => { splits(); numbers(); });
  } else {
    splits();
    numbers();
  }

  global.LynxUI = Object.freeze({
    icon, iconButton, toast, icons: Object.keys(paths), inspect, setSelectValue, splits, numbers,
    combobox, tokens, formatDuration, parseDuration, formatDateTime, attachmentRow,
    confirmDialog, openMenu, contextMenu, closeAnchoredMenu,
  });
})(window);
