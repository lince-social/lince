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
    [".lynx-check", "Checkbox"],
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

  function alignTooltip(event) {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (!target) return;
    const boundary = target.closest("[data-lynx-tooltip-boundary], .lynx-field, .sand") || document.documentElement;
    const targetBox = target.getBoundingClientRect();
    const boundaryBox = boundary.getBoundingClientRect();
    const viewportWidth = document.documentElement.clientWidth;
    const viewportHeight = document.documentElement.clientHeight;
    const width = Math.min(180, Math.max(1, boundaryBox.width - 4), Math.max(1, viewportWidth - 4), target.dataset.lynxTooltip.length * 6 + 10);
    const height = Math.ceil(target.dataset.lynxTooltip.length * 7 / width) * 15 + 10;
    target.style.setProperty("--lynx-tooltip-max-width", `${width}px`);
    delete target.dataset.lynxTooltipAlign;
    delete target.dataset.lynxTooltipSide;
    if (targetBox.left + targetBox.width / 2 - width / 2 < boundaryBox.left) target.dataset.lynxTooltipAlign = "left";
    if (targetBox.left + targetBox.width / 2 + width / 2 > boundaryBox.right) target.dataset.lynxTooltipAlign = "right";
    const cannotFitAbove = targetBox.top - height - 4 < Math.max(0, boundaryBox.top);
    const fitsBelow = targetBox.bottom + height + 4 <= Math.min(viewportHeight, boundaryBox.bottom);
    if (cannotFitAbove && fitsBelow) target.dataset.lynxTooltipSide = "bottom";
  }

  document.addEventListener("pointerover", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target) delete target.dataset.lynxTooltipDismissed;
    alignTooltip(event);
  });
  document.addEventListener("pointerout", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target && !target.contains(event.relatedTarget)) delete target.dataset.lynxTooltipDismissed;
  });
  document.addEventListener("focusin", alignTooltip);
  document.addEventListener("focusout", (event) => {
    const target = event.target.closest?.("[data-lynx-tooltip]");
    if (target && !target.contains(event.relatedTarget)) delete target.dataset.lynxTooltipDismissed;
  });

  document.addEventListener("click", (event) => {
    event.target.closest?.("[data-lynx-tooltip]")?.setAttribute("data-lynx-tooltip-dismissed", "");
    const target = event.target.closest("[data-lynx-dialog-open], [data-lynx-dialog-close], [data-lynx-dropdown-button], [data-lynx-select-option], [data-lynx-tab]");
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
  });

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

  global.LynxUI = Object.freeze({ icon, iconButton, icons: Object.keys(paths), inspect, setSelectValue });
})(window);
