(function () {
  const ICONS = {
    edit: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.8 3.1 12.9 6.2"></path><path d="M11.3 1.9a1.45 1.45 0 0 1 2.1 2.1L5.7 11.7 3 12.4l.7-2.7 7.6-7.8Z"></path></svg>',
    delete: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 4.5h9"></path><path d="M6.5 2.75h3"></path><path d="M5 4.5v7"></path><path d="M8 4.5v7"></path><path d="M11 4.5v7"></path><path d="M4.5 4.5 5 13h6l.5-8.5"></path></svg>',
    chevronDown: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"></path></svg>',
  };

  function createElement(tagName, options = {}) {
    const element = document.createElement(tagName);
    if (options.className) {
      element.className = options.className;
    }
    if (options.text != null) {
      element.textContent = String(options.text);
    }
    if (options.html != null) {
      element.innerHTML = String(options.html);
    }
    if (options.attributes) {
      for (const [name, value] of Object.entries(options.attributes)) {
        if (value === false || value == null) {
          continue;
        }
        if (value === true) {
          element.setAttribute(name, "");
        } else {
          element.setAttribute(name, String(value));
        }
      }
    }
    if (options.dataset) {
      Object.assign(element.dataset, options.dataset);
    }
    if (options.children) {
      element.append(...options.children.filter(Boolean));
    }
    return element;
  }

  function button(options = {}) {
    return createElement("button", {
      className: options.className || "lynx-button",
      html: options.html,
      text: options.text,
      attributes: {
        type: options.type || "button",
        "aria-label": options.label,
        disabled: options.disabled === true,
        ...options.attributes,
      },
      dataset: options.dataset,
      children: options.children,
    });
  }

  function icon(name) {
    return ICONS[name] || "";
  }

  function iconButton(options = {}) {
    return button({
      ...options,
      className: options.className || "lynx-icon-button",
      html: options.html || icon(options.icon),
    });
  }

  function popover(options = {}) {
    return createElement("div", {
      className: options.className || "lynx-popover",
      attributes: {
        id: options.id,
        role: options.role,
        "aria-label": options.label,
      },
      children: options.children,
    });
  }

  function setPopoverOpen(element, open, openClass = "isOpen") {
    if (!element) {
      return;
    }
    element.classList.toggle(openClass, Boolean(open));
  }

  function tooltip(target, text) {
    if (!target || !text) {
      return target;
    }
    target.dataset.lynxTooltip = String(text);
    target.setAttribute("aria-label", target.getAttribute("aria-label") || String(text));
    return target;
  }

  window.LynxDS = {
    button,
    createElement,
    icon,
    iconButton,
    popover,
    setPopoverOpen,
    tooltip,
  };
})();
