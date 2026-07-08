// Stage 8b — the sand-settings "Data (Protein)" panel. Replaces the old server
// view picker: a full CRUD over saved Proteins (protein-records) plus choosing
// which Protein drives the card. Self-contained — it owns its DOM elements and a
// dedicated transport WebSocket, and writes the chosen Protein into the card's
// widgetState via the injected `patchCardState` (the sand then re-subscribes).
//
// Deps: getCard(cardId) -> { id, widgetState }, patchCardState(cardId, patch),
// syncFrames() -> push card state to the frames.

function transportUrl() {
  const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${window.location.host}/host/transport/ws`;
}

const LIST_PROTEIN = {
  source: "record",
  where: [{ all: [{ kind_eq: "protein" }, { quantity_gt: 0 }] }],
  limit: 200,
};

export function createProteinConfigPanel({ getCard, patchCardState, syncFrames }) {
  const el = (id) => document.getElementById(id);
  const driverSelect = el("widget-config-protein-driver");
  const nameInput = el("widget-config-protein-name");
  const titleInput = el("widget-config-protein-title");
  const astInput = el("widget-config-protein-ast");
  const validateBtn = el("widget-config-protein-validate");
  const saveBtn = el("widget-config-protein-save");
  const deleteBtn = el("widget-config-protein-delete");
  const help = el("widget-config-protein-help");

  const ready = Boolean(
    driverSelect && nameInput && titleInput && astInput && validateBtn && saveBtn && deleteBtn && help,
  );
  if (!ready) {
    // Degrade gracefully — the rest of the modal still works.
    return { open() {}, close() {} };
  }

  let cardId = null;
  let list = []; // [{ uid, slug, head, body }]
  let socket = null;
  let nextReq = 1;
  const pending = new Map();

  function setHelp(text, isError) {
    help.textContent = text || "";
    help.hidden = !text;
    help.classList.toggle("is-ok", Boolean(text) && !isError);
  }

  function ensureSocket() {
    if (socket && socket.readyState <= WebSocket.OPEN) return socket;
    socket = new WebSocket(transportUrl());
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ type: "subscribe", id: "protein-list", protein: LIST_PROTEIN }));
    });
    socket.addEventListener("message", (event) => {
      let msg = null;
      try { msg = JSON.parse(event.data); } catch { return; }
      if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-list") {
        list = (msg.rows || []).map((r) => ({ uid: r.uid, slug: r.slug, head: r.head, body: r.body }));
        renderOptions();
      } else if (msg.type === "action_ok" || msg.type === "error") {
        const resolve = pending.get(String(msg.id));
        if (resolve) { pending.delete(String(msg.id)); resolve(msg); }
      }
    });
    socket.addEventListener("close", () => { socket = null; });
    return socket;
  }

  function act(action) {
    return new Promise((resolve) => {
      const sock = ensureSocket();
      const id = "pa-" + nextReq++;
      pending.set(id, resolve);
      const send = () => sock.send(JSON.stringify({ type: "act", id, action }));
      if (sock.readyState === WebSocket.OPEN) send();
      else sock.addEventListener("open", send, { once: true });
      window.setTimeout(() => {
        if (pending.has(id)) { pending.delete(id); resolve({ type: "error", message: "timed out" }); }
      }, 15000);
    });
  }

  function currentDriver() {
    const ws = getCard(cardId)?.widgetState || {};
    if (ws.savedProtein) return { kind: "saved", value: String(ws.savedProtein) };
    if (ws.protein && typeof ws.protein === "object") return { kind: "ast", value: ws.protein };
    return { kind: "all" };
  }

  function option(value, label, selected) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = label;
    if (selected) opt.selected = true;
    return opt;
  }

  function renderOptions() {
    if (!cardId) return;
    const driver = currentDriver();
    driverSelect.replaceChildren();
    driverSelect.appendChild(option("__all__", "All records", driver.kind === "all"));
    driverSelect.appendChild(option("__inline__", "Inline AST (below)", driver.kind === "ast"));
    for (const p of list) {
      const slug = p.slug || "";
      driverSelect.appendChild(
        option("saved:" + slug, (p.head || slug || "(unnamed)") + " — " + slug,
          driver.kind === "saved" && driver.value === slug),
      );
    }
  }

  function parseAst() {
    try {
      const ast = JSON.parse(astInput.value);
      if (!ast || typeof ast !== "object" || Array.isArray(ast)) throw new Error("must be a JSON object");
      if (!ast.source) throw new Error('missing "source" (record | promise | decision)');
      return ast;
    } catch (error) {
      setHelp("Invalid Protein AST: " + error.message, true);
      return null;
    }
  }

  function loadEditorFromSaved(slug) {
    const found = list.find((p) => p.slug === slug);
    if (!found) return;
    nameInput.value = found.slug || "";
    titleInput.value = found.head || "";
    if (found.body) astInput.value = found.body; // body mirrors the AST
  }

  function drive(patch, label) {
    if (!cardId) return;
    patchCardState(cardId, patch);
    syncFrames();
    setHelp(label, false);
  }

  driverSelect.addEventListener("change", () => {
    const value = driverSelect.value;
    if (value === "__all__") {
      drive({ savedProtein: null, protein: null }, "Driving with all records.");
    } else if (value === "__inline__") {
      const ast = parseAst();
      if (ast) drive({ savedProtein: null, protein: ast }, "Driving with the inline AST.");
    } else if (value.startsWith("saved:")) {
      const slug = value.slice("saved:".length);
      loadEditorFromSaved(slug);
      drive({ savedProtein: slug, protein: null }, `Driving with "${slug}".`);
    }
  });

  validateBtn.addEventListener("click", () => {
    if (parseAst()) setHelp("Valid Protein AST.", false);
  });

  saveBtn.addEventListener("click", async () => {
    const ast = parseAst();
    if (!ast) return;
    const slug = String(nameInput.value || "").trim();
    if (!slug) { setHelp("Give the Protein a name (slug).", true); return; }
    const head = String(titleInput.value || "").trim() || slug;
    saveBtn.disabled = true;
    const res = await act({ action: "save-protein", slug, head, ast });
    saveBtn.disabled = false;
    if (res.type === "error") { setHelp("Save failed: " + (res.message || "rejected"), true); return; }
    setHelp(`Saved "${slug}". Pick it above to drive this card.`, false);
    // the list refreshes live via the subscription
  });

  deleteBtn.addEventListener("click", async () => {
    const slug = String(nameInput.value || "").trim();
    if (!slug) { setHelp("Enter the name of the saved Protein to delete.", true); return; }
    const res = await act({ action: "deactivate", target: slug });
    if (res.type === "error") { setHelp("Delete failed: " + (res.message || "rejected"), true); return; }
    // if the deleted Protein was driving this card, fall back to all records
    if (currentDriver().value === slug) drive({ savedProtein: null, protein: null }, `Deleted "${slug}".`);
    else setHelp(`Deleted "${slug}".`, false);
  });

  return {
    open(id) {
      cardId = id;
      setHelp("");
      const driver = currentDriver();
      nameInput.value = "";
      titleInput.value = "";
      astInput.value = driver.kind === "ast"
        ? JSON.stringify(driver.value, null, 2)
        : JSON.stringify({ source: "record", limit: 200 }, null, 2);
      ensureSocket();
      renderOptions();
    },
    close() {
      cardId = null;
    },
  };
}
