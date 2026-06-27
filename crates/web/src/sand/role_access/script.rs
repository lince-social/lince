pub(crate) fn script() -> String {
    r#"
(() => {
  const frame = window.frameElement;
  const state = {
    serverId: String(frame?.dataset?.linceServerId || "").trim(),
    roles: [],
    users: [],
    permissions: [],
    rolePermissions: [],
    selectedRoleId: null,
    busy: false,
    modalMode: null,
  };

  const els = {
    serverPill: document.getElementById("server-pill"),
    refresh: document.getElementById("refresh-button"),
    status: document.getElementById("status"),
    roleList: document.getElementById("role-list"),
    roleTitle: document.getElementById("role-title"),
    roleCount: document.getElementById("role-count"),
    permissionGrid: document.getElementById("permission-grid"),
    userList: document.getElementById("user-list"),
    newRole: document.getElementById("new-role-button"),
    newUser: document.getElementById("new-user-button"),
    modalBackdrop: document.getElementById("modal-backdrop"),
    modalTitle: document.getElementById("modal-title"),
    modalClose: document.getElementById("modal-close"),
    modalCancel: document.getElementById("modal-cancel"),
    modalForm: document.getElementById("modal-form"),
    modalFields: document.getElementById("modal-fields"),
  };

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (char) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    })[char]);
  }

  function tableUrl(table, id = null) {
    if (!state.serverId) {
      throw new Error("Configure a server in the card first.");
    }
    let url = "/host/integrations/servers/" + encodeURIComponent(state.serverId) +
      "/table/" + encodeURIComponent(table);
    if (id !== null && id !== undefined) {
      url += "/" + encodeURIComponent(String(id));
    }
    return url;
  }

  async function api(table, options = {}) {
    const response = await fetch(tableUrl(table, options.id), {
      method: options.method || "GET",
      headers: options.body ? { "content-type": "application/json" } : undefined,
      body: options.body ? JSON.stringify(options.body) : undefined,
    });
    if (response.status === 401) {
      window.LinceWidgetHost?.invalidateServerAuth?.(state.serverId);
    }
    const text = await response.text();
    let payload = null;
    if (text.trim()) {
      try {
        payload = JSON.parse(text);
      } catch {
        payload = text;
      }
    }
    if (!response.ok) {
      const message = payload?.message || payload?.error || text || response.statusText;
      throw new Error(message);
    }
    return payload;
  }

  function setStatus(message, tone = "idle") {
    els.status.textContent = message;
    els.status.dataset.tone = tone;
  }

  function selectedRole() {
    return state.roles.find((role) => role.id === state.selectedRoleId) || state.roles[0] || null;
  }

  function rolePermissionRows(roleId) {
    return state.rolePermissions.filter((row) => Number(row.role_id) === Number(roleId));
  }

  function roleHasPermission(roleId, permissionId) {
    return rolePermissionRows(roleId).some((row) => Number(row.permission_id) === Number(permissionId));
  }

  function roleForUser(user) {
    return state.roles.find((role) => Number(role.id) === Number(user.role_id)) || null;
  }

  function renderRoles() {
    if (!state.roles.length) {
      els.roleList.innerHTML = `<div class="empty">No roles found.</div>`;
      return;
    }
    const active = selectedRole();
    els.roleList.innerHTML = state.roles.map((role) => {
      const count = rolePermissionRows(role.id).length;
      return `
        <button class="roleCard ${active?.id === role.id ? "isActive" : ""}" type="button" data-role-id="${role.id}">
          <div class="roleName">${escapeHtml(role.name)}</div>
          <div class="roleMeta">#${escapeHtml(role.id)} · ${count} permissions</div>
        </button>
      `;
    }).join("");
  }

  function renderPermissions() {
    const role = selectedRole();
    if (!role) {
      els.roleTitle.textContent = "No role selected";
      els.roleCount.textContent = "0 permissions";
      els.permissionGrid.innerHTML = `<div class="empty">Create or select a role first.</div>`;
      return;
    }
    const count = rolePermissionRows(role.id).length;
    els.roleTitle.textContent = role.name;
    els.roleCount.textContent = count + " permissions";
    if (!state.permissions.length) {
      els.permissionGrid.innerHTML = `<div class="empty">No permissions are installed in this backend.</div>`;
      return;
    }
    els.permissionGrid.innerHTML = state.permissions.map((permission) => {
      const checked = roleHasPermission(role.id, permission.id);
      return `
        <label class="permissionItem">
          <input type="checkbox" data-permission-id="${permission.id}" ${checked ? "checked" : ""}>
          <span>
            <span class="permissionSubject">${escapeHtml(permission.subject)}</span>
            <span class="permissionName">${escapeHtml(permission.action)}</span>
            <span class="permissionDescription">${escapeHtml(permission.description || "")}</span>
          </span>
        </label>
      `;
    }).join("");
  }

  function renderUsers() {
    if (!state.users.length) {
      els.userList.innerHTML = `<div class="empty">No users found.</div>`;
      return;
    }
    const roleOptions = state.roles.map((role) =>
      `<option value="${role.id}">${escapeHtml(role.name)}</option>`
    ).join("");
    els.userList.innerHTML = state.users.map((user) => {
      const role = roleForUser(user);
      return `
        <article class="userCard" data-user-id="${user.id}">
          <div>
            <div class="userName">${escapeHtml(user.name || user.username)}</div>
            <div class="userMeta">@${escapeHtml(user.username)} · #${escapeHtml(user.id)} · ${escapeHtml(role?.name || "no role")}</div>
          </div>
          <select class="field" data-user-role="${user.id}">
            ${roleOptions}
          </select>
        </article>
      `;
    }).join("");
    for (const user of state.users) {
      const select = els.userList.querySelector(`[data-user-role="${CSS.escape(String(user.id))}"]`);
      if (select) select.value = String(user.role_id);
    }
  }

  function render() {
    els.serverPill.textContent = state.serverId ? `server ${state.serverId}` : "server unset";
    renderRoles();
    renderPermissions();
    renderUsers();
  }

  async function load() {
    if (!state.serverId) {
      setStatus("Configure server", "warn");
      render();
      return;
    }
    setStatus("Loading access data", "idle");
    try {
      const [roles, users, permissions, rolePermissions] = await Promise.all([
        api("role"),
        api("app_user"),
        api("permission"),
        api("role_permission"),
      ]);
      state.roles = Array.isArray(roles) ? roles : [];
      state.users = Array.isArray(users) ? users : [];
      state.permissions = Array.isArray(permissions) ? permissions : [];
      state.rolePermissions = Array.isArray(rolePermissions) ? rolePermissions : [];
      if (!state.roles.some((role) => role.id === state.selectedRoleId)) {
        state.selectedRoleId = state.roles[0]?.id ?? null;
      }
      render();
      setStatus("Access data loaded", "ok");
    } catch (error) {
      setStatus(error.message || "Failed to load access data", "error");
      render();
    }
  }

  async function setRolePermission(permissionId, enabled) {
    const role = selectedRole();
    if (!role) return;
    const existing = state.rolePermissions.find((row) =>
      Number(row.role_id) === Number(role.id) && Number(row.permission_id) === Number(permissionId)
    );
    try {
      if (enabled && !existing) {
        await api("role_permission", {
          method: "POST",
          body: { role_id: role.id, permission_id: Number(permissionId) },
        });
      } else if (!enabled && existing) {
        await api("role_permission", { method: "DELETE", id: existing.id });
      }
      await load();
    } catch (error) {
      setStatus(error.message || "Permission update failed", "error");
      await load();
    }
  }

  async function assignUserRole(userId, roleId) {
    try {
      await api("app_user", {
        method: "PATCH",
        id: userId,
        body: { role_id: Number(roleId) },
      });
      await load();
      setStatus("User role updated", "ok");
    } catch (error) {
      setStatus(error.message || "User role update failed", "error");
      await load();
    }
  }

  function openModal(mode) {
    state.modalMode = mode;
    els.modalTitle.textContent = mode === "role" ? "Create role" : "Create user";
    els.modalFields.innerHTML = mode === "role"
      ? `<label>Role name<input class="field" name="name" autocomplete="off" required></label>`
      : `
        <label>Name<input class="field" name="name" autocomplete="off" required></label>
        <label>Username<input class="field" name="username" autocomplete="off" required></label>
        <label>Password<input class="field" name="password" type="password" autocomplete="new-password" required></label>
        <label>Role<select class="field" name="role_id">${state.roles.map((role) => `<option value="${role.id}">${escapeHtml(role.name)}</option>`).join("")}</select></label>
      `;
    els.modalBackdrop.hidden = false;
    els.modalFields.querySelector("input, select")?.focus();
  }

  function closeModal() {
    state.modalMode = null;
    els.modalBackdrop.hidden = true;
    els.modalForm.reset();
  }

  async function submitModal(event) {
    event.preventDefault();
    const data = new FormData(els.modalForm);
    const mode = state.modalMode;
    if (!mode) return;
    try {
      if (mode === "role") {
        await api("role", { method: "POST", body: { name: String(data.get("name") || "").trim() } });
      } else {
        await api("app_user", {
          method: "POST",
          body: {
            name: String(data.get("name") || "").trim(),
            username: String(data.get("username") || "").trim(),
            password: String(data.get("password") || ""),
            role_id: Number(data.get("role_id")),
          },
        });
      }
      closeModal();
      await load();
      setStatus(mode === "role" ? "Role created" : "User created", "ok");
    } catch (error) {
      setStatus(error.message || "Create failed", "error");
    }
  }

  els.refresh.addEventListener("click", load);
  els.newRole.addEventListener("click", () => openModal("role"));
  els.newUser.addEventListener("click", () => openModal("user"));
  els.modalClose.addEventListener("click", closeModal);
  els.modalCancel.addEventListener("click", closeModal);
  els.modalForm.addEventListener("submit", submitModal);
  els.roleList.addEventListener("click", (event) => {
    const button = event.target.closest("[data-role-id]");
    if (!button) return;
    state.selectedRoleId = Number(button.dataset.roleId);
    render();
  });
  els.permissionGrid.addEventListener("change", (event) => {
    const input = event.target.closest("[data-permission-id]");
    if (!input) return;
    void setRolePermission(Number(input.dataset.permissionId), input.checked);
  });
  els.userList.addEventListener("change", (event) => {
    const select = event.target.closest("[data-user-role]");
    if (!select) return;
    void assignUserRole(Number(select.dataset.userRole), Number(select.value));
  });

  const observer = new MutationObserver(() => {
    const next = String(frame?.dataset?.linceServerId || "").trim();
    if (next !== state.serverId) {
      state.serverId = next;
      void load();
    }
  });
  if (frame) {
    observer.observe(frame, { attributes: true, attributeFilter: ["data-lince-server-id"] });
  }

  void load();
})();
"#.to_string()
}
