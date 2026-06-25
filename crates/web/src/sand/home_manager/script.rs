pub(super) fn script() -> String {
    r##"
(() => {
  const frame = window.frameElement;
  const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
  const storageKey = "home-manager/v1/" + instanceId;

  const nodes = {
    generationNumber: document.getElementById("generation-number"),
    generationLabel: document.getElementById("generation-label"),
    activateButton: document.getElementById("activate-button"),
    rollbackButton: document.getElementById("rollback-button"),
    filter: document.getElementById("module-filter"),
    profile: document.getElementById("profile-select"),
    modulesEnabled: document.getElementById("modules-enabled"),
    packagesCount: document.getElementById("packages-count"),
    servicesRunning: document.getElementById("services-running"),
    driftCount: document.getElementById("drift-count"),
    moduleCount: document.getElementById("module-count"),
    moduleList: document.getElementById("module-list"),
    packageForm: document.getElementById("package-form"),
    packageInput: document.getElementById("package-input"),
    packageList: document.getElementById("package-list"),
    serviceState: document.getElementById("service-state"),
    serviceList: document.getElementById("service-list"),
    activationState: document.getElementById("activation-state"),
    activationList: document.getElementById("activation-list"),
  };

  const defaults = {
    profile: "workstation",
    generation: 42,
    generationLabel: "stable workstation",
    modules: [
      { id: "shell", name: "Shell", path: "programs.zsh + starship", enabled: true },
      { id: "editor", name: "Editor", path: "programs.neovim + git integrations", enabled: true },
      { id: "desktop", name: "Desktop", path: "dconf, fonts, portals, theming", enabled: true },
      { id: "secrets", name: "Secrets", path: "sops-nix user keys", enabled: false },
      { id: "sync", name: "Sync", path: "syncthing folders and ignores", enabled: true },
      { id: "media", name: "Media", path: "mpv, pipewire controls, codecs", enabled: false },
    ],
    packages: ["ripgrep", "fd", "eza", "bat", "jq", "git"],
    services: [
      { id: "gpg", name: "gpg-agent", description: "SSH socket and pinentry environment", status: "running" },
      { id: "syncthing", name: "syncthing", description: "Personal folder replication", status: "paused" },
      { id: "ssh", name: "ssh-agent", description: "Forwarding disabled, identities loaded", status: "running" },
      { id: "timer", name: "home-manager-news", description: "Checks unread release notes", status: "queued" },
    ],
    tasks: [
      { title: "Evaluate flake inputs", copy: "Check lockfile age and profile-specific overlays.", state: "done" },
      { title: "Build user environment", copy: "Compile the activation package without switching.", state: "done" },
      { title: "Diff generation", copy: "Review package, service, and dotfile changes.", state: "ready" },
      { title: "Switch profile", copy: "Run activation and keep the previous generation available.", state: "pending" },
    ],
  };

  let state = loadState();

  function loadState() {
    try {
      const parsed = JSON.parse(localStorage.getItem(storageKey) || "null");
      if (parsed && Array.isArray(parsed.modules) && Array.isArray(parsed.packages)) {
        return { ...structuredClone(defaults), ...parsed };
      }
    } catch (_error) {}
    return structuredClone(defaults);
  }

  function saveState() {
    localStorage.setItem(storageKey, JSON.stringify(state));
  }

  function matchesFilter(...values) {
    const query = nodes.filter.value.trim().toLowerCase();
    if (!query) {
      return true;
    }
    return values.some((value) => String(value).toLowerCase().includes(query));
  }

  function toneForStatus(status) {
    if (status === "running" || status === "done") return "ok";
    if (status === "paused" || status === "ready") return "warn";
    if (status === "stopped" || status === "failed") return "off";
    return "info";
  }

  function render() {
    nodes.profile.value = state.profile;
    nodes.generationNumber.textContent = String(state.generation);
    nodes.generationLabel.textContent = state.generationLabel;

    const enabledModules = state.modules.filter((module) => module.enabled).length;
    const runningServices = state.services.filter((service) => service.status === "running").length;
    const drift = state.tasks.filter((task) => task.state !== "done").length;

    nodes.modulesEnabled.textContent = `${enabledModules} / ${state.modules.length}`;
    nodes.packagesCount.textContent = String(state.packages.length);
    nodes.servicesRunning.textContent = `${runningServices} / ${state.services.length}`;
    nodes.driftCount.textContent = String(drift);
    nodes.moduleCount.textContent = `${enabledModules} enabled`;
    nodes.serviceState.textContent = drift ? "pending switch" : "steady";
    nodes.activationState.textContent = drift ? "dirty" : "clean";

    renderModules();
    renderPackages();
    renderServices();
    renderTasks();
  }

  function renderModules() {
    nodes.moduleList.replaceChildren();
    state.modules
      .filter((module) => matchesFilter(module.name, module.path))
      .forEach((module) => {
        const item = document.createElement("article");
        item.className = "moduleItem";

        const top = document.createElement("div");
        top.className = "moduleTop";

        const name = document.createElement("div");
        name.className = "moduleName";
        name.textContent = module.name;

        const label = document.createElement("label");
        label.className = "switch";
        label.title = module.enabled ? "Disable module" : "Enable module";

        const input = document.createElement("input");
        input.type = "checkbox";
        input.checked = module.enabled;
        input.addEventListener("change", () => {
          module.enabled = input.checked;
          markTaskReady("Diff generation");
          saveState();
          render();
        });

        const slider = document.createElement("span");
        label.append(input, slider);
        top.append(name, label);

        const path = document.createElement("div");
        path.className = "modulePath";
        path.textContent = module.path;
        item.append(top, path);
        nodes.moduleList.append(item);
      });
  }

  function renderPackages() {
    nodes.packageList.replaceChildren();
    state.packages
      .filter((name) => matchesFilter(name))
      .forEach((name) => {
        const item = document.createElement("div");
        item.className = "packageItem";

        const label = document.createElement("span");
        label.className = "packageName";
        label.textContent = name;

        const button = document.createElement("button");
        button.className = "removeButton";
        button.type = "button";
        button.setAttribute("aria-label", `Remove ${name}`);
        button.textContent = "×";
        button.addEventListener("click", () => {
          state.packages = state.packages.filter((itemName) => itemName !== name);
          markTaskReady("Diff generation");
          saveState();
          render();
        });

        item.append(label, button);
        nodes.packageList.append(item);
      });
  }

  function renderServices() {
    nodes.serviceList.replaceChildren();
    state.services
      .filter((service) => matchesFilter(service.name, service.description, service.status))
      .forEach((service) => {
        const item = document.createElement("article");
        item.className = "serviceItem";

        const top = document.createElement("div");
        top.className = "serviceTop";

        const name = document.createElement("div");
        name.className = "serviceName";
        name.textContent = service.name;

        const pill = document.createElement("button");
        pill.className = "statusPill";
        pill.type = "button";
        pill.dataset.tone = toneForStatus(service.status);
        pill.textContent = service.status;
        pill.addEventListener("click", () => {
          const next = { running: "paused", paused: "stopped", stopped: "running", queued: "running" };
          service.status = next[service.status] || "running";
          markTaskReady("Switch profile");
          saveState();
          render();
        });

        top.append(name, pill);
        const copy = document.createElement("div");
        copy.className = "serviceCopy";
        copy.textContent = service.description;
        item.append(top, copy);
        nodes.serviceList.append(item);
      });
  }

  function renderTasks() {
    nodes.activationList.replaceChildren();
    state.tasks.forEach((task) => {
      const item = document.createElement("li");
      item.className = "activationItem";

      const top = document.createElement("div");
      top.className = "serviceTop";

      const title = document.createElement("div");
      title.className = "serviceName";
      title.textContent = task.title;

      const pill = document.createElement("span");
      pill.className = "statusPill";
      pill.dataset.tone = toneForStatus(task.state);
      pill.textContent = task.state;

      top.append(title, pill);
      const copy = document.createElement("div");
      copy.className = "activationCopy";
      copy.textContent = task.copy;
      item.append(top, copy);
      nodes.activationList.append(item);
    });
  }

  function markTaskReady(title) {
    const task = state.tasks.find((item) => item.title === title);
    if (task && task.state === "done") {
      task.state = "ready";
    }
  }

  nodes.packageForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const value = nodes.packageInput.value.trim();
    if (!value || state.packages.includes(value)) {
      return;
    }
    state.packages.push(value);
    nodes.packageInput.value = "";
    markTaskReady("Diff generation");
    saveState();
    render();
  });

  nodes.filter.addEventListener("input", render);

  nodes.profile.addEventListener("change", () => {
    state.profile = nodes.profile.value;
    state.generationLabel = state.profile + " profile";
    markTaskReady("Evaluate flake inputs");
    saveState();
    render();
  });

  nodes.activateButton.addEventListener("click", () => {
    state.generation += 1;
    state.generationLabel = state.profile + " switched";
    state.tasks = state.tasks.map((task) => ({ ...task, state: "done" }));
    saveState();
    render();
  });

  nodes.rollbackButton.addEventListener("click", () => {
    state.generation = Math.max(1, state.generation - 1);
    state.generationLabel = "rollback target";
    state.tasks = state.tasks.map((task) =>
      task.title === "Switch profile" ? { ...task, state: "ready" } : task
    );
    saveState();
    render();
  });

  render();
})();
"##
    .to_string()
}
