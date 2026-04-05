pub(super) fn script() -> String {
    r##"
(function () {
    const bridge = window.LinceWidgetHost || null;
    const frame = window.frameElement;
    const app = document.getElementById("app");
    const canvas = document.getElementById("graph");
    const ctx = canvas.getContext("2d");
    const editor = document.getElementById("editor");
    const sidePanel = document.getElementById("side-panel");
    const panelToggleButton = document.getElementById("panel-toggle");
    const panelCloseButton = document.getElementById("panel-close");
    const createOpenButton = document.getElementById("create-open");
    const modePill = document.getElementById("mode-pill");
    const originPill = document.getElementById("origin-pill");
    const statusPill = document.getElementById("status-pill");
    const rowPill = document.getElementById("row-pill");
    const linkPill = document.getElementById("link-pill");
    const filterPill = document.getElementById("filter-pill");
    const zoomPill = document.getElementById("zoom-pill");
    const emptyState = document.getElementById("empty-state");
    const originText = document.getElementById("origin-text");
    const createSummary = document.getElementById("create-summary");
    const createHeadInput = document.getElementById("create-head");
    const createBodyInput = document.getElementById("create-body");
    const createQuantityInput = document.getElementById("create-quantity");
    const createCategoryList = document.getElementById("create-category-list");
    const createClearButton = document.getElementById("create-clear");
    const createSubmitButton = document.getElementById("create-submit");
    const selectionPill = document.getElementById("selection-pill");
    const selectionEmpty = document.getElementById("selection-empty");
    const selectionContent = document.getElementById("selection-content");
    const selectionSummary = document.getElementById("selection-summary");
    const parentSearchInput = document.getElementById("parent-search-query");
    const parentSearchSummary = document.getElementById("parent-search-summary");
    const parentChoiceList = document.getElementById("parent-choice-list");
    const childList = document.getElementById("child-list");
    const parentHeadQuery = document.getElementById("parent-head-query");
    const categoryInput = document.getElementById("category-input");
    const categoryAddButton = document.getElementById("category-add");
    const selectedCategoryList = document.getElementById("selected-category-list");
    const categoryFilterList = document.getElementById("category-filter-list");
    const chargeInput = document.getElementById("physics-charge");
    const chargeValue = document.getElementById("physics-charge-value");
    const distanceInput = document.getElementById("physics-distance");
    const distanceValue = document.getElementById("physics-distance-value");
    const collisionInput = document.getElementById("physics-collision");
    const collisionValue = document.getElementById("physics-collision-value");
    const centerInput = document.getElementById("physics-center");
    const centerValue = document.getElementById("physics-center-value");
    const applyFiltersButton = document.getElementById("apply-filters");
    const clearFiltersButton = document.getElementById("clear-filters");
    const resetPhysicsButton = document.getElementById("reset-physics");
    const connectParentButton = document.getElementById("connect-parent");
    const disconnectParentButton = document.getElementById("disconnect-parent");
    const zoomInButton = document.getElementById("zoom-in");
    const zoomOutButton = document.getElementById("zoom-out");
    const zoomFitButton = document.getElementById("zoom-fit");

    const DEFAULT_PHYSICS = {
        charge: -220,
        linkDistance: 110,
        collisionRadius: 24,
        centerForce: 0.18,
    };
    const MIN_ZOOM = 0.35;
    const MAX_ZOOM = 3.5;
    const INITIAL_TICKS = 140;
    const LABEL_START_SCALE = 0.75;
    const LABEL_FULL_SCALE = 1.75;

    const state = {
        mode: "view",
        origin: {
            serverId: String(frame?.dataset?.linceServerId || "").trim(),
            viewId: Number(frame?.dataset?.linceViewId || 0) || null,
        },
        cardState: {},
        snapshot: null,
        rows: [],
        nodes: [],
        links: [],
        categories: [],
        availableCategories: [],
        draftFilters: {
            categories: [],
            parentHeadQuery: "",
        },
        physics: { ...DEFAULT_PHYSICS },
        selectedId: null,
        selectedParentId: null,
        parentSearchQuery: "",
        createDraft: {
            head: "",
            body: "",
            quantity: "0",
        },
        pendingCreate: false,
        pendingParentMutation: false,
        panelOpen: false,
        stream: null,
        streamGeneration: 0,
        error: "",
        width: 0,
        height: 0,
        resizeObserver: null,
        simulation: null,
        needsRedraw: false,
        viewport: {
            scale: 1,
            x: 0,
            y: 0,
            initialized: false,
        },
        pointer: null,
    };

    function instanceId() {
        return String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
    }

    function clampNumber(value, min, max, fallback) {
        const parsed = Number(value);
        if (!Number.isFinite(parsed)) {
            return fallback;
        }
        return Math.min(max, Math.max(min, parsed));
    }

    function escapeHtml(value) {
        return String(value ?? "")
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll("\"", "&quot;")
            .replaceAll("'", "&#39;");
    }

    function uniqueStrings(values) {
        const seen = new Set();
        const out = [];
        for (const raw of Array.isArray(values) ? values : []) {
            const value = String(raw || "").trim();
            if (!value) {
                continue;
            }
            const key = value.toLowerCase();
            if (seen.has(key)) {
                continue;
            }
            seen.add(key);
            out.push(value);
        }
        return out;
    }

    function normalizePhysics(raw) {
        const next = raw && typeof raw === "object" ? raw : {};
        return {
            charge: clampNumber(next.charge, -600, -20, DEFAULT_PHYSICS.charge),
            linkDistance: clampNumber(next.linkDistance, 30, 240, DEFAULT_PHYSICS.linkDistance),
            collisionRadius: clampNumber(next.collisionRadius, 10, 60, DEFAULT_PHYSICS.collisionRadius),
            centerForce: clampNumber(next.centerForce, 0, 1, DEFAULT_PHYSICS.centerForce),
        };
    }

    function normalizeDraftFilters(raw) {
        const next = {
            categories: [],
            parentHeadQuery: "",
        };
        const rows = Array.isArray(raw) ? raw : [];
        for (const row of rows) {
            const field = String(row?.field || "");
            if (field === "categories_any_json" && Array.isArray(row?.value)) {
                next.categories = uniqueStrings(row.value);
            } else if (field === "parent_head_query") {
                next.parentHeadQuery = String(row?.value || "").trim();
            }
        }
        return next;
    }

    function buildFilterRows() {
        const rows = [];
        if (state.draftFilters.categories.length) {
            rows.push({
                field: "categories_any_json",
                operator: "any_of",
                value: state.draftFilters.categories.slice(),
            });
        }
        if (state.draftFilters.parentHeadQuery.trim()) {
            rows.push({
                field: "parent_head_query",
                operator: "contains",
                value: state.draftFilters.parentHeadQuery.trim(),
            });
        }
        return rows;
    }

    function appliedFilterState() {
        return normalizeDraftFilters(state.cardState.filters || []);
    }

    function appliedCreateCategories() {
        return appliedFilterState().categories.slice();
    }

    function getMeta() {
        return bridge?.getMeta?.() || {};
    }

    function requestDraw() {
        if (state.needsRedraw) {
            return;
        }
        state.needsRedraw = true;
        window.requestAnimationFrame(() => {
            state.needsRedraw = false;
            drawGraph();
        });
    }

    function renderMode() {
        document.body.dataset.mode = state.mode;
        modePill.textContent = state.mode === "edit" ? "origin" : "view";
        editor.hidden = false;
    }

    function renderOrigin() {
        const serverId = state.origin.serverId || "local";
        const viewId = state.origin.viewId == null ? "none" : String(state.origin.viewId);
        originPill.textContent = `${serverId} / view ${viewId}`;
        originText.textContent = `serverId: ${serverId}\nviewId: ${viewId}\nmode: ${state.mode}`;
    }

    function renderStatus(label, tone, detail) {
        statusPill.textContent = label;
        statusPill.dataset.tone = tone || "neutral";
        statusPill.dataset.detail = detail || "";
    }

    function renderCounters() {
        rowPill.textContent = `${state.rows.length} nodes`;
        linkPill.textContent = `${state.links.length} links`;
        filterPill.textContent = `${buildFilterRows().length} filters`;
    }

    function renderZoomPill() {
        zoomPill.textContent = `${Math.round(state.viewport.scale * 100)}%`;
    }

    function renderPanel() {
        sidePanel.hidden = !state.panelOpen;
        panelToggleButton.textContent = state.panelOpen ? "Hide panel" : "Panel";
        panelToggleButton.dataset.open = state.panelOpen ? "true" : "false";
    }

    function renderCategoryChoices() {
        const categories = state.availableCategories.length ? state.availableCategories : state.categories;
        const selected = new Set(state.draftFilters.categories.map((value) => value.toLowerCase()));
        selectedCategoryList.innerHTML = state.draftFilters.categories.length
            ? state.draftFilters.categories
                  .map((category) => {
                      const safe = escapeHtml(category);
                      return `
                          <button class="chipButton" type="button" data-category-remove="${safe}" title="Remove category">
                              <span>${safe}</span>
                              <span aria-hidden="true">×</span>
                          </button>
                      `;
                  })
                  .join("")
            : '<span class="mutedCopy">No categories selected.</span>';

        if (!categories.length) {
            categoryFilterList.innerHTML = '<span class="mutedCopy">No categories in the current view.</span>';
            return;
        }

        categoryFilterList.innerHTML = categories
            .map((category) => {
                const checked = selected.has(category.toLowerCase()) ? "checked" : "";
                const safe = escapeHtml(category);
                return `
                    <label class="checkItem">
                        <input type="checkbox" value="${safe}" ${checked}>
                        <span>${safe}</span>
                    </label>
                `;
            })
            .join("");
    }

    function renderFilterControls() {
        parentHeadQuery.value = state.draftFilters.parentHeadQuery || "";
        if (categoryInput) {
            categoryInput.value = "";
        }
        renderCategoryChoices();
    }

    function renderCreateForm() {
        const categories = appliedCreateCategories();
        createHeadInput.value = String(state.createDraft.head || "");
        createBodyInput.value = String(state.createDraft.body || "");
        createQuantityInput.value = String(state.createDraft.quantity || "0");
        createCategoryList.innerHTML = categories.length
            ? categories
                  .map((category) => `<span class="pill">${escapeHtml(category)}</span>`)
                  .join("")
            : '<span class="mutedCopy">No applied categories. New records will be created without category links.</span>';
        createSummary.textContent = !state.origin.serverId
            ? "This widget needs a configured server before it can create records."
            : categories.length
              ? `New records will inherit ${categories.length} applied categor${categories.length === 1 ? "y" : "ies"} from this view.`
              : "No category filter is currently applied to this view.";
        createClearButton.disabled = state.pendingCreate;
        createSubmitButton.disabled = state.pendingCreate || !state.origin.serverId || !String(state.createDraft.head || "").trim();
    }

    function renderPhysicsControls() {
        chargeInput.value = String(state.physics.charge);
        chargeValue.textContent = String(state.physics.charge);
        distanceInput.value = String(state.physics.linkDistance);
        distanceValue.textContent = String(state.physics.linkDistance);
        collisionInput.value = String(state.physics.collisionRadius);
        collisionValue.textContent = String(state.physics.collisionRadius);
        centerInput.value = String(state.physics.centerForce);
        centerValue.textContent = String(state.physics.centerForce.toFixed(2));
    }

    function renderParentChoices(node) {
        if (!node) {
            parentChoiceList.innerHTML = "";
            parentSearchSummary.textContent = "Choose a possible father from the current graph.";
            return;
        }

        const blockedIds = collectDescendantIds(node.id);
        const needle = String(state.parentSearchQuery || "").trim().toLowerCase();
        const candidates = state.nodes
            .filter((candidate) => candidate.id !== node.id && !blockedIds.has(candidate.id))
            .filter((candidate) => {
                if (!needle) {
                    return true;
                }
                const haystacks = [
                    String(candidate.head || ""),
                    ...(Array.isArray(candidate.categories) ? candidate.categories : []),
                ];
                return haystacks.some((value) => String(value || "").toLowerCase().includes(needle));
            })
            .slice()
            .sort((left, right) =>
                String(left.head || "")
                    .toLowerCase()
                    .localeCompare(String(right.head || "").toLowerCase()) ||
                left.id - right.id,
            );

        const selectedParent = state.nodes.find((candidate) => candidate.id === state.selectedParentId) || null;
        parentSearchSummary.textContent = selectedParent
            ? `Target father: #${selectedParent.id} ${selectedParent.head || "Untitled"}`
            : node.parentId
              ? `Current father: #${node.parentId} ${node.parentHead || "Untitled"}`
              : "No father selected.";

        if (!candidates.length) {
            parentChoiceList.innerHTML = '<p class="mutedCopy">No possible fathers match the current search.</p>';
            return;
        }

        parentChoiceList.innerHTML = candidates
            .map((candidate) => {
                const isSelected = candidate.id === state.selectedParentId;
                const categories = Array.isArray(candidate.categories) && candidate.categories.length
                    ? candidate.categories.join(", ")
                    : "No categories";
                return `
                    <button class="parentChoice${isSelected ? " is-selected" : ""}" type="button" data-parent-choice="${candidate.id}">
                        <span class="parentChoice__head">#${candidate.id} ${escapeHtml(candidate.head || "Untitled")}</span>
                        <span class="parentChoice__meta">${escapeHtml(categories)}</span>
                    </button>
                `;
            })
            .join("");
    }

    function renderSelection() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            selectionPill.textContent = "None";
            selectionEmpty.hidden = false;
            selectionContent.hidden = true;
            childList.innerHTML = "";
            parentChoiceList.innerHTML = "";
            parentSearchSummary.textContent = "Choose a possible father from the current graph.";
            connectParentButton.disabled = true;
            disconnectParentButton.disabled = true;
            return;
        }

        if (state.selectedParentId != null && !state.nodes.some((item) => item.id === state.selectedParentId)) {
            state.selectedParentId = node.parentId || null;
        }
        if (state.selectedParentId == null) {
            state.selectedParentId = node.parentId || null;
        }

        selectionPill.textContent = `#${node.id}`;
        selectionEmpty.hidden = true;
        selectionContent.hidden = false;
        parentSearchInput.value = state.parentSearchQuery;

        const parent = state.nodes.find((item) => item.id === node.parentId) || null;
        const draftParent = state.nodes.find((item) => item.id === state.selectedParentId) || null;
        const children = Array.isArray(node.children) ? node.children : [];
        const categories = Array.isArray(node.categories) ? node.categories : [];
        const parentLabel = parent
            ? `#${parent.id} ${parent.head}`
            : node.parentHead || "none";
        const targetLabel = draftParent
            ? `#${draftParent.id} ${draftParent.head}`
            : node.parentId
              ? parentLabel
              : "none";

        selectionSummary.textContent = [
            `id: ${node.id}`,
            `head: ${node.head || "Untitled"}`,
            `quantity: ${node.quantity}`,
            `current parent: ${parentLabel}`,
            `target parent: ${targetLabel}`,
            `depth: ${node.depth == null ? "n/a" : node.depth}`,
            `categories: ${categories.length ? categories.join(", ") : "none"}`,
        ].join("\n");

        childList.innerHTML = children.length
            ? children
                  .map((child) => `<span class="pill">#${escapeHtml(child.id)} ${escapeHtml(child.head || "Untitled")}</span>`)
                  .join("")
            : '<span class="mutedCopy">No children.</span>';

        connectParentButton.disabled = state.pendingParentMutation || !draftParent || draftParent.id === node.parentId;
        disconnectParentButton.disabled = state.pendingParentMutation || !node.parentId;
        renderParentChoices(node);
    }

    function renderFilterPill() {
        renderCounters();
    }

    function syncFromBridge() {
        const meta = getMeta();
        const cardState = bridge?.getCardState?.() || {};
        state.mode = meta.mode === "edit" ? "edit" : "view";
        state.cardState = cardState && typeof cardState === "object" ? cardState : {};
        state.draftFilters = normalizeDraftFilters(state.cardState.filters || []);
        state.physics = normalizePhysics(
            state.cardState.relations_physics || state.cardState.physics || state.cardState.relationsPhysics,
        );
        state.origin.serverId = String(meta.serverId || state.origin.serverId || frame?.dataset?.linceServerId || "").trim();
        state.origin.viewId = Number(meta.viewId || state.origin.viewId || frame?.dataset?.linceViewId || 0) || null;
        renderMode();
        renderOrigin();
        renderFilterControls();
        renderCreateForm();
        renderPhysicsControls();
        renderFilterPill();
        renderPanel();
        requestDraw();
    }

    function updateCategoryStateFromControls() {
        const next = [];
        for (const input of categoryFilterList.querySelectorAll("input[type='checkbox']")) {
            if (input.checked) {
                next.push(input.value);
            }
        }
        state.draftFilters.categories = uniqueStrings(next);
        renderFilterPill();
        renderCategoryChoices();
    }

    function addCategoryFromInput() {
        const value = String(categoryInput?.value || "").trim();
        if (!value) {
            return;
        }
        state.draftFilters.categories = uniqueStrings([
            ...state.draftFilters.categories,
            value,
        ]);
        if (categoryInput) {
            categoryInput.value = "";
        }
        renderCategoryChoices();
        renderFilterPill();
    }

    function removeCategory(value) {
        const needle = String(value || "").trim().toLowerCase();
        if (!needle) {
            return;
        }
        state.draftFilters.categories = state.draftFilters.categories.filter(
            (category) => category.toLowerCase() !== needle,
        );
        renderCategoryChoices();
        renderFilterPill();
    }

    function persistPhysics() {
        bridge?.patchCardState?.({
            relations_physics: { ...state.physics },
        });
    }

    function applyPhysicsToUi() {
        chargeValue.textContent = String(state.physics.charge);
        distanceValue.textContent = String(state.physics.linkDistance);
        collisionValue.textContent = String(state.physics.collisionRadius);
        centerValue.textContent = String(state.physics.centerForce.toFixed(2));
    }

    function setPhysics(nextPhysics, shouldPersist) {
        state.physics = normalizePhysics(nextPhysics);
        renderPhysicsControls();
        applySimulationForces();
        if (shouldPersist) {
            persistPhysics();
            nudgeSimulation(0.18);
        }
        requestDraw();
    }

    function resetCreateDraft() {
        state.createDraft = {
            head: "",
            body: "",
            quantity: "0",
        };
    }

    function openCreatePanel() {
        state.panelOpen = true;
        renderPanel();
        window.requestAnimationFrame(() => {
            createHeadInput?.focus();
            createHeadInput?.select?.();
        });
    }

    function submitCreateRecord() {
        if (!state.origin.serverId) {
            renderStatus("Missing origin", "error", "The widget needs a server binding before it can create records.");
            renderCreateForm();
            return;
        }
        const head = String(state.createDraft.head || "").trim();
        if (!head) {
            renderStatus("Missing head", "error", "A new record needs a head.");
            renderCreateForm();
            createHeadInput?.focus();
            return;
        }

        const categories = appliedCreateCategories();
        const quantity = parseOptionalInteger(state.createDraft.quantity);
        state.pendingCreate = true;
        renderCreateForm();
        renderStatus("Creating", "status", "Record creation pending.");
        postAction("create-record", {
            record: {
                head,
                body: String(state.createDraft.body || "").trim(),
                quantity: quantity == null ? 0 : quantity,
            },
            taskType: null,
            categories,
            startAt: null,
            endAt: null,
            estimateSeconds: null,
            assigneeIds: [],
            parentId: null,
        })
            .then((outcome) => {
                state.pendingCreate = false;
                const nextRecordId = Number(outcome?.record_id || 0) || null;
                if (nextRecordId) {
                    state.selectedId = nextRecordId;
                }
                resetCreateDraft();
                renderCreateForm();
                renderStatus("Created", "live", "Record created.");
                restartStream();
            })
            .catch((error) => {
                state.pendingCreate = false;
                renderCreateForm();
                renderStatus("Create error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function parseCell(row, key) {
        const value = row && Object.prototype.hasOwnProperty.call(row, key) ? row[key] : undefined;
        if (value == null) {
            return "";
        }
        const text = String(value).trim();
        return /^null$/i.test(text) ? "" : text;
    }

    function parseOptionalInteger(value) {
        const text = String(value || "").trim();
        if (!text) {
            return null;
        }
        const parsed = Number.parseInt(text, 10);
        return Number.isFinite(parsed) ? parsed : null;
    }

    function parseJsonArray(raw) {
        const text = String(raw || "").trim();
        if (!text) {
            return [];
        }
        try {
            const parsed = JSON.parse(text);
            return Array.isArray(parsed) ? parsed : [];
        } catch {
            return [];
        }
    }

    function parseRow(row) {
        const id = Number(parseCell(row, "id"));
        const quantity = Number(parseCell(row, "quantity"));
        if (!Number.isFinite(id) || !Number.isFinite(quantity)) {
            return null;
        }
        const categories = uniqueStrings([
            ...parseJsonArray(parseCell(row, "categories_json")),
            ...parseJsonArray(parseCell(row, "categories")),
            parseCell(row, "primary_category"),
        ]);
        const declaredChildren = parseJsonArray(parseCell(row, "children_json"))
            .map((child) => ({
                id: Number(child?.id),
                head: String(child?.head || ""),
                quantity: Number(child?.quantity || 0),
                taskType: child?.task_type == null ? null : String(child.task_type),
            }))
            .filter((child) => Number.isFinite(child.id));
        const parsedParentId = parseOptionalInteger(parseCell(row, "parent_id"));
        const parentId = parsedParentId && parsedParentId > 0 ? parsedParentId : null;
        const parentHead = parseCell(row, "parent_head") || null;

        return {
            id,
            quantity,
            head: parseCell(row, "head").trim() || "Untitled",
            body: parseCell(row, "body"),
            taskType: parseCell(row, "task_type") || null,
            categories,
            parentId,
            parentHead,
            declaredChildren,
            children: [],
            childrenCount: parseOptionalInteger(parseCell(row, "children_count")) || declaredChildren.length,
            depth: parseOptionalInteger(parseCell(row, "depth")),
        };
    }

    function collectCategories(rows) {
        const seen = new Set();
        const out = [];
        for (const row of rows) {
            for (const category of row.categories || []) {
                const key = category.toLowerCase();
                if (seen.has(key)) {
                    continue;
                }
                seen.add(key);
                out.push(category);
            }
        }
        return out;
    }

    function mergeChild(parent, child) {
        if (!parent || !child) {
            return;
        }
        if (!Array.isArray(parent.children)) {
            parent.children = [];
        }
        if (parent.children.some((entry) => Number(entry?.id) === child.id)) {
            return;
        }
        parent.children.push({
            id: child.id,
            head: child.head,
            quantity: child.quantity,
            taskType: child.taskType,
        });
    }

    function inferParentIdFromHead(node, nodesById) {
        if (node.parentId || !node.parentHead) {
            return node.parentId;
        }
        const needle = String(node.parentHead || "").trim().toLowerCase();
        if (!needle) {
            return null;
        }

        let matchId = null;
        for (const candidate of nodesById.values()) {
            if (candidate.id === node.id) {
                continue;
            }
            if (String(candidate.head || "").trim().toLowerCase() !== needle) {
                continue;
            }
            if (matchId != null) {
                return null;
            }
            matchId = candidate.id;
        }
        return matchId;
    }

    function resolveDepths(nodes, nodesById) {
        const memo = new Map();
        const visiting = new Set();

        function visit(node) {
            if (!node) {
                return null;
            }
            if (node.depth != null) {
                return node.depth;
            }
            if (memo.has(node.id)) {
                return memo.get(node.id);
            }
            if (visiting.has(node.id)) {
                return null;
            }

            visiting.add(node.id);
            let nextDepth = 0;
            if (node.parentId && nodesById.has(node.parentId)) {
                const parentDepth = visit(nodesById.get(node.parentId));
                nextDepth = parentDepth == null ? null : parentDepth + 1;
            }
            visiting.delete(node.id);
            memo.set(node.id, nextDepth);
            return nextDepth;
        }

        for (const node of nodes) {
            node.depth = visit(node);
        }
    }

    function buildGraphData(snapshot) {
        const rows = Array.isArray(snapshot?.rows) ? snapshot.rows : [];
        const parsedRows = rows.map(parseRow).filter(Boolean);
        const nodesById = new Map(parsedRows.map((row) => [row.id, row]));
        const links = [];

        for (const node of parsedRows) {
            node.children = [];
            node.parentId = inferParentIdFromHead(node, nodesById);
        }

        for (const node of parsedRows) {
            if (node.parentId && nodesById.has(node.parentId)) {
                const parent = nodesById.get(node.parentId);
                links.push({
                    sourceId: node.id,
                    targetId: parent.id,
                });
                mergeChild(parent, node);
            }
        }

        for (const node of parsedRows) {
            node.children.sort((left, right) =>
                String(left?.head || "")
                    .toLowerCase()
                    .localeCompare(String(right?.head || "").toLowerCase()),
            );
            node.childrenCount = node.children.length;
        }
        resolveDepths(parsedRows, nodesById);

        return {
            rows: parsedRows,
            nodes: parsedRows,
            links,
            categories: collectCategories(parsedRows),
        };
    }

    function deterministicOffset(seed, spread) {
        const value = Math.sin(seed * 12.9898) * 43758.5453;
        const normalized = value - Math.floor(value);
        return (normalized - 0.5) * spread;
    }

    function nodeRadius(node) {
        const categories = Array.isArray(node.categories) ? node.categories.length : 0;
        return 11 + Math.min(8, categories * 1.5);
    }

    function applyInitialLayout(nodes) {
        const sorted = nodes.slice().sort((left, right) =>
            String(left.head || "")
                .toLowerCase()
                .localeCompare(String(right.head || "").toLowerCase()) ||
            left.id - right.id,
        );
        const goldenAngle = Math.PI * (3 - Math.sqrt(5));
        for (let index = 0; index < sorted.length; index += 1) {
            const node = sorted[index];
            const radius = 34 * Math.sqrt(index + 1);
            const angle = index * goldenAngle;
            node.x = Math.cos(angle) * radius + deterministicOffset(node.id, 14);
            node.y = Math.sin(angle) * radius + deterministicOffset(node.id + 17, 14);
            node.vx = 0;
            node.vy = 0;
            node.fx = null;
            node.fy = null;
        }
    }

    function currentCentroid(nodes) {
        const positioned = (nodes || []).filter((node) => Number.isFinite(node.x) && Number.isFinite(node.y));
        if (!positioned.length) {
            return { x: 0, y: 0 };
        }
        let x = 0;
        let y = 0;
        for (const node of positioned) {
            x += node.x;
            y += node.y;
        }
        return {
            x: x / positioned.length,
            y: y / positioned.length,
        };
    }

    function seedNewNodePosition(node, nodesById, previousNodesById) {
        if (Number.isFinite(node.x) && Number.isFinite(node.y)) {
            return;
        }

        const parent = node.parentId ? nodesById.get(node.parentId) || previousNodesById.get(node.parentId) : null;
        if (parent && Number.isFinite(parent.x) && Number.isFinite(parent.y)) {
            node.x = parent.x + 96 + deterministicOffset(node.id, 20);
            node.y = parent.y + deterministicOffset(node.id + 11, 70);
        } else {
            const childPoints = (Array.isArray(node.children) ? node.children : [])
                .map((child) => nodesById.get(Number(child?.id)) || previousNodesById.get(Number(child?.id)))
                .filter((candidate) => candidate && Number.isFinite(candidate.x) && Number.isFinite(candidate.y));
            if (childPoints.length) {
                const centroid = currentCentroid(childPoints);
                node.x = centroid.x - 78 + deterministicOffset(node.id, 20);
                node.y = centroid.y + deterministicOffset(node.id + 19, 56);
            } else {
                const centroid = currentCentroid(state.nodes);
                node.x = centroid.x + deterministicOffset(node.id, 120);
                node.y = centroid.y + deterministicOffset(node.id + 23, 120);
            }
        }

        node.vx = 0;
        node.vy = 0;
        node.fx = null;
        node.fy = null;
    }

    function graphSignature(nodes, links) {
        const nodeSignature = (Array.isArray(nodes) ? nodes : [])
            .map((node) => Number(node?.id || 0))
            .filter((id) => id > 0)
            .sort((left, right) => left - right)
            .join(",");
        const linkSignature = (Array.isArray(links) ? links : [])
            .map((link) => {
                const sourceId = Number(link?.sourceId || link?.source?.id || 0);
                const targetId = Number(link?.targetId || link?.target?.id || 0);
                return sourceId > 0 && targetId > 0 ? `${sourceId}>${targetId}` : "";
            })
            .filter(Boolean)
            .sort()
            .join(",");
        return `${nodeSignature}|${linkSignature}`;
    }

    function rebuildCurrentRelations() {
        const nodesById = new Map(state.nodes.map((node) => [node.id, node]));
        const links = [];
        for (const node of state.nodes) {
            node.children = [];
        }

        for (const node of state.nodes) {
            const parent = node.parentId ? nodesById.get(node.parentId) || null : null;
            if (parent && parent.id !== node.id) {
                node.parentHead = parent.head || null;
                links.push({ source: node, target: parent });
                mergeChild(parent, node);
            } else if (!node.parentId) {
                node.parentHead = null;
            }
        }

        for (const node of state.nodes) {
            node.children.sort((left, right) =>
                String(left?.head || "")
                    .toLowerCase()
                    .localeCompare(String(right?.head || "").toLowerCase()),
            );
            node.childrenCount = node.children.length;
            node.depth = null;
        }

        resolveDepths(state.nodes, nodesById);
        state.links = links;
        renderCounters();
        renderSelection();
        applySimulationForces();
        nudgeSimulation(0.08);
        requestDraw();
    }

    function captureParentState() {
        return {
            selectedParentId: state.selectedParentId,
            pendingParentMutation: state.pendingParentMutation,
            nodes: state.nodes.map((node) => ({
                id: node.id,
                parentId: node.parentId,
                parentHead: node.parentHead,
            })),
        };
    }

    function restoreParentState(snapshot) {
        const byId = new Map((snapshot?.nodes || []).map((node) => [node.id, node]));
        for (const node of state.nodes) {
            const saved = byId.get(node.id);
            if (!saved) {
                continue;
            }
            node.parentId = saved.parentId;
            node.parentHead = saved.parentHead;
        }
        state.selectedParentId = snapshot?.selectedParentId ?? null;
        state.pendingParentMutation = Boolean(snapshot?.pendingParentMutation);
        rebuildCurrentRelations();
    }

    function applyOptimisticParentChange(recordId, parentId) {
        const node = state.nodes.find((item) => item.id === recordId) || null;
        if (!node) {
            return false;
        }
        const parent = parentId ? state.nodes.find((item) => item.id === parentId) || null : null;
        node.parentId = parent ? parent.id : null;
        node.parentHead = parent ? parent.head || null : null;
        state.selectedParentId = parent ? parent.id : null;
        rebuildCurrentRelations();
        return true;
    }

    function reconcileGraph(graph) {
        const previousNodes = Array.isArray(state.nodes) ? state.nodes.slice() : [];
        const previousNodesById = new Map(previousNodes.map((node) => [node.id, node]));
        const nextNodes = [];

        for (const rawNode of graph.nodes) {
            const existing = previousNodesById.get(rawNode.id);
            if (existing) {
                existing.quantity = rawNode.quantity;
                existing.head = rawNode.head;
                existing.body = rawNode.body;
                existing.taskType = rawNode.taskType;
                existing.categories = rawNode.categories;
                existing.parentId = rawNode.parentId;
                existing.parentHead = rawNode.parentHead;
                existing.declaredChildren = rawNode.declaredChildren;
                existing.children = rawNode.children;
                existing.childrenCount = rawNode.childrenCount;
                existing.depth = rawNode.depth;
                nextNodes.push(existing);
            } else {
                nextNodes.push({
                    ...rawNode,
                    x: Number.NaN,
                    y: Number.NaN,
                    vx: 0,
                    vy: 0,
                    fx: null,
                    fy: null,
                });
            }
        }

        if (!previousNodes.length) {
            applyInitialLayout(nextNodes);
        }

        const nextNodesById = new Map(nextNodes.map((node) => [node.id, node]));
        for (const node of nextNodes) {
            seedNewNodePosition(node, nextNodesById, previousNodesById);
        }

        const nextLinks = graph.links
            .map((link) => {
                const source = nextNodesById.get(link.sourceId);
                const target = nextNodesById.get(link.targetId);
                if (!source || !target) {
                    return null;
                }
                return { source, target };
            })
            .filter(Boolean);

        state.rows = nextNodes;
        state.nodes = nextNodes;
        state.links = nextLinks;
        state.categories = graph.categories;
        state.availableCategories = graph.categories;

        if (!state.nodes.some((node) => node.id === state.selectedId)) {
            state.selectedId = state.nodes.length ? state.nodes[0].id : null;
            state.selectedParentId = null;
            state.parentSearchQuery = "";
        }
    }

    function updateOriginFromSnapshot(snapshot) {
        const viewId = snapshot?.view_id ?? snapshot?.viewId ?? state.origin.viewId;
        const name = snapshot?.name ? String(snapshot.name) : "Relations";
        const query = snapshot?.query ? String(snapshot.query) : "";
        originText.textContent = [
            `serverId: ${state.origin.serverId || "local"}`,
            `viewId: ${viewId == null ? "none" : viewId}`,
            `view: ${name}`,
            `query: ${query || "(none)"}`,
        ].join("\n");
    }

    function updateStatusFromSnapshot() {
        if (!state.error) {
            renderStatus("Live", "live", `${state.rows.length} nodes`);
        }
    }

    function updateEmptyState() {
        emptyState.hidden = state.rows.length > 0;
    }

    function graphUrl() {
        return "/host/widgets/" + encodeURIComponent(instanceId()) + "/stream";
    }

    function actionUrl(action) {
        return "/host/widgets/" + encodeURIComponent(instanceId()) + "/actions/" + encodeURIComponent(action);
    }

    function postAction(action, payload) {
        return fetch(actionUrl(action), {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload || {}),
        }).then(async (response) => {
            const text = await response.text();
            if (!response.ok) {
                throw new Error(text || response.statusText);
            }
            try {
                return JSON.parse(text);
            } catch {
                return { ok: true, raw: text };
            }
        });
    }

    function clearStream() {
        if (state.stream) {
            state.stream.close();
            state.stream = null;
        }
        state.streamGeneration += 1;
    }

    function restartStream() {
        clearStream();
        connectStream();
    }

    function ensureCanvasSize() {
        const rect = canvas.getBoundingClientRect();
        if (!rect.width || !rect.height) {
            return false;
        }
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.round(rect.width * dpr);
        canvas.height = Math.round(rect.height * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        state.width = rect.width;
        state.height = rect.height;
        return true;
    }

    function applySimulationForces() {
        if (!window.d3 || !state.nodes.length) {
            return;
        }
        if (!state.simulation) {
            state.simulation = window.d3.forceSimulation(state.nodes);
            state.simulation.alphaDecay(0.08);
            state.simulation.velocityDecay(0.3);
            state.simulation.on("tick", requestDraw);
            state.simulation.stop();
        } else {
            state.simulation.nodes(state.nodes);
        }

        const linkForce = state.simulation.force("link") || window.d3.forceLink().id((node) => node.id);
        linkForce
            .id((node) => node.id)
            .links(state.links)
            .distance(state.physics.linkDistance)
            .strength(0.82);

        state.simulation
            .force("link", linkForce)
            .force("charge", window.d3.forceManyBody().strength(state.physics.charge))
            .force(
                "collide",
                window.d3
                    .forceCollide()
                    .radius((node) => Math.max(state.physics.collisionRadius, nodeRadius(node) + 6))
                    .iterations(2),
            )
            .force("x", window.d3.forceX(0).strength(state.physics.centerForce))
            .force("y", window.d3.forceY(0).strength(state.physics.centerForce));
    }

    function nudgeSimulation(alpha) {
        if (!state.simulation) {
            return;
        }
        state.simulation.alpha(Math.max(state.simulation.alpha(), alpha)).restart();
    }

    function settleInitialLayout() {
        if (!state.simulation) {
            return;
        }
        state.simulation.alpha(1);
        for (let index = 0; index < INITIAL_TICKS; index += 1) {
            state.simulation.tick();
        }
        state.simulation.alpha(0);
        stopSimulation();
    }

    function stopSimulation() {
        if (state.simulation) {
            state.simulation.stop();
        }
    }

    function measureBounds(nodes) {
        if (!Array.isArray(nodes) || !nodes.length) {
            return null;
        }
        let minX = Number.POSITIVE_INFINITY;
        let minY = Number.POSITIVE_INFINITY;
        let maxX = Number.NEGATIVE_INFINITY;
        let maxY = Number.NEGATIVE_INFINITY;
        for (const node of nodes) {
            const radius = nodeRadius(node);
            minX = Math.min(minX, node.x - radius);
            minY = Math.min(minY, node.y - radius);
            maxX = Math.max(maxX, node.x + radius);
            maxY = Math.max(maxY, node.y + radius);
        }
        return {
            minX,
            minY,
            maxX,
            maxY,
            width: Math.max(1, maxX - minX),
            height: Math.max(1, maxY - minY),
        };
    }

    function fitViewportToNodes() {
        if (!state.nodes.length || !state.width || !state.height) {
            return;
        }
        const bounds = measureBounds(state.nodes);
        if (!bounds) {
            return;
        }
        const padding = 88;
        const scaleX = (state.width - padding * 2) / bounds.width;
        const scaleY = (state.height - padding * 2) / bounds.height;
        const nextScale = clampNumber(Math.min(scaleX, scaleY, 1.3), MIN_ZOOM, MAX_ZOOM, 1);
        state.viewport.scale = nextScale;
        state.viewport.x = (state.width - (bounds.minX + bounds.maxX) * nextScale) / 2;
        state.viewport.y = (state.height - (bounds.minY + bounds.maxY) * nextScale) / 2;
        state.viewport.initialized = true;
        renderZoomPill();
        requestDraw();
    }

    function worldToScreen(x, y) {
        return {
            x: x * state.viewport.scale + state.viewport.x,
            y: y * state.viewport.scale + state.viewport.y,
        };
    }

    function screenToWorld(x, y) {
        return {
            x: (x - state.viewport.x) / state.viewport.scale,
            y: (y - state.viewport.y) / state.viewport.scale,
        };
    }

    function zoomAt(screenX, screenY, factor) {
        const oldScale = state.viewport.scale;
        const nextScale = clampNumber(oldScale * factor, MIN_ZOOM, MAX_ZOOM, oldScale);
        if (Math.abs(nextScale - oldScale) < 0.0001) {
            return;
        }
        const worldPoint = screenToWorld(screenX, screenY);
        state.viewport.scale = nextScale;
        state.viewport.x = screenX - worldPoint.x * nextScale;
        state.viewport.y = screenY - worldPoint.y * nextScale;
        renderZoomPill();
        requestDraw();
    }

    function collectDescendantIds(rootId) {
        const blocked = new Set([rootId]);
        const queue = [rootId];
        while (queue.length) {
            const currentId = queue.shift();
            for (const link of state.links) {
                const sourceId = Number(link?.source?.id || 0);
                const targetId = Number(link?.target?.id || 0);
                if (targetId !== currentId || !sourceId) {
                    continue;
                }
                const childId = sourceId;
                if (!childId || blocked.has(childId)) {
                    continue;
                }
                blocked.add(childId);
                queue.push(childId);
            }
        }
        return blocked;
    }

    function syncSnapshot(snapshot) {
        state.snapshot = snapshot;
        const graph = buildGraphData(snapshot);
        const hadNodes = state.nodes.length > 0;
        const previousSignature = graphSignature(state.nodes, state.links);
        const nextSignature = graphSignature(graph.nodes, graph.links);
        reconcileGraph(graph);
        renderCounters();
        renderCategoryChoices();
        renderSelection();
        updateOriginFromSnapshot(snapshot);
        updateStatusFromSnapshot();
        updateEmptyState();
        ensureCanvasSize();
        applySimulationForces();
        if (!hadNodes) {
            settleInitialLayout();
            fitViewportToNodes();
            requestDraw();
        } else if (previousSignature !== nextSignature) {
            nudgeSimulation(0.16);
        } else {
            requestDraw();
        }
    }

    function resizeCanvas() {
        if (!ensureCanvasSize()) {
            return;
        }
        if (!state.viewport.initialized && state.nodes.length) {
            fitViewportToNodes();
        } else {
            renderZoomPill();
            requestDraw();
        }
    }

    function drawArrow(from, to, color) {
        const angle = Math.atan2(to.y - from.y, to.x - from.x);
        const size = 8 / state.viewport.scale;
        ctx.save();
        ctx.translate(to.x, to.y);
        ctx.rotate(angle);
        ctx.beginPath();
        ctx.moveTo(-size, -size * 0.5);
        ctx.lineTo(0, 0);
        ctx.lineTo(-size, size * 0.5);
        ctx.strokeStyle = color;
        ctx.lineWidth = 2 / state.viewport.scale;
        ctx.stroke();
        ctx.restore();
    }

    function drawDirectedLink(link, highlight) {
        const source = link.source;
        const target = link.target;
        if (!source || !target) {
            return;
        }
        const sourceRadius = nodeRadius(source) + 2 / state.viewport.scale;
        const targetRadius = nodeRadius(target) + 4 / state.viewport.scale;
        const dx = target.x - source.x;
        const dy = target.y - source.y;
        const distance = Math.hypot(dx, dy);
        if (!distance) {
            return;
        }
        const ux = dx / distance;
        const uy = dy / distance;
        const startX = source.x + ux * sourceRadius;
        const startY = source.y + uy * sourceRadius;
        const endX = target.x - ux * targetRadius;
        const endY = target.y - uy * targetRadius;
        const color = highlight ? "rgba(120, 215, 255, 0.56)" : "rgba(120, 215, 255, 0.34)";

        ctx.beginPath();
        ctx.moveTo(startX, startY);
        ctx.lineTo(endX, endY);
        ctx.strokeStyle = color;
        ctx.lineWidth = (highlight ? 2.6 : 1.8) / state.viewport.scale;
        ctx.stroke();
        drawArrow(
            { x: startX, y: startY },
            { x: endX, y: endY },
            color,
        );
    }

    function nodeFill(node) {
        if (node.id === state.selectedId) {
            return "#d8f5e8";
        }
        if (node.parentId) {
            return "#12212a";
        }
        return "#182733";
    }

    function nodeStroke(node) {
        if (node.id === state.selectedId) {
            return "#78d7ff";
        }
        if (node.quantity > 0) {
            return "#7ef0c6";
        }
        if (node.quantity < 0) {
            return "#f2bb78";
        }
        return "rgba(120, 215, 255, 0.4)";
    }

    function drawNodeLabel(node, radius) {
        const baseAlpha = clampNumber(
            (state.viewport.scale - LABEL_START_SCALE) / (LABEL_FULL_SCALE - LABEL_START_SCALE),
            0,
            1,
            0,
        );
        const alpha = node.id === state.selectedId ? 1 : Math.pow(baseAlpha, 1.5);
        if (alpha <= 0.01) {
            return;
        }
        const point = worldToScreen(node.x, node.y);
        const fontSize = clampNumber(11 + (state.viewport.scale - 1) * 2.4, 11, 16, 12);
        ctx.fillStyle = `rgba(230, 237, 243, ${alpha})`;
        ctx.font = `600 ${fontSize}px ${getComputedStyle(document.body).fontFamily}`;
        ctx.textBaseline = "middle";
        ctx.fillText(String(node.head || "Untitled"), point.x + radius * state.viewport.scale + 10, point.y + 1);
    }

    function drawGraph() {
        const width = state.width || canvas.clientWidth;
        const height = state.height || canvas.clientHeight;
        if (!width || !height) {
            return;
        }

        ctx.clearRect(0, 0, width, height);
        ctx.save();
        ctx.translate(state.viewport.x, state.viewport.y);
        ctx.scale(state.viewport.scale, state.viewport.scale);

        for (const link of state.links) {
            const source = link.source;
            const target = link.target;
            if (!source || !target) {
                continue;
            }
            const highlight = source.id === state.selectedId || target.id === state.selectedId;
            drawDirectedLink(link, highlight);
        }

        for (const node of state.nodes) {
            const radius = nodeRadius(node);
            ctx.beginPath();
            ctx.arc(node.x, node.y, radius, 0, Math.PI * 2);
            ctx.fillStyle = nodeFill(node);
            ctx.fill();
            ctx.lineWidth = (node.id === state.selectedId ? 2.4 : 1.3) / state.viewport.scale;
            ctx.strokeStyle = nodeStroke(node);
            ctx.stroke();

            if (node.id === state.selectedId) {
                ctx.beginPath();
                ctx.arc(node.x, node.y, radius + 5 / state.viewport.scale, 0, Math.PI * 2);
                ctx.strokeStyle = "rgba(120, 215, 255, 0.18)";
                ctx.lineWidth = 2 / state.viewport.scale;
                ctx.stroke();
            }
        }

        ctx.restore();

        for (const node of state.nodes) {
            drawNodeLabel(node, nodeRadius(node));
        }
    }

    function findNodeAtPoint(screenX, screenY) {
        const point = screenToWorld(screenX, screenY);
        if (state.simulation && typeof state.simulation.find === "function") {
            return state.simulation.find(point.x, point.y, 22 / state.viewport.scale) || null;
        }
        for (const node of state.nodes) {
            const dx = node.x - point.x;
            const dy = node.y - point.y;
            if (Math.hypot(dx, dy) <= nodeRadius(node) + 4 / state.viewport.scale) {
                return node;
            }
        }
        return null;
    }

    function selectNode(node) {
        if (!node) {
            state.selectedId = null;
            state.selectedParentId = null;
            state.parentSearchQuery = "";
            renderSelection();
            requestDraw();
            return;
        }

        state.selectedId = node.id;
        state.selectedParentId = node.parentId || null;
        state.parentSearchQuery = "";
        if (!state.panelOpen) {
            state.panelOpen = true;
            renderPanel();
        }
        renderSelection();
        requestDraw();
    }

    function connectParent() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            return;
        }
        const parentId = Number(state.selectedParentId || 0) || null;
        if (!parentId) {
            renderStatus("Choose father", "error", "Select a father before saving the relation.");
            return;
        }
        if (parentId === node.id) {
            renderStatus("Invalid father", "error", "A node cannot parent itself.");
            return;
        }
        const relationSnapshot = captureParentState();
        state.pendingParentMutation = true;
        applyOptimisticParentChange(node.id, parentId);
        renderStatus("Saving", "status", "Parent relation update pending.");
        postAction("set-parent", {
            recordId: node.id,
            parentId,
        })
            .then(() => {
                state.pendingParentMutation = false;
                renderSelection();
                renderStatus("Saved", "live", "Parent relation updated.");
                restartStream();
            })
            .catch((error) => {
                restoreParentState(relationSnapshot);
                renderStatus("Relation error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function disconnectParent() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            return;
        }
        const relationSnapshot = captureParentState();
        state.pendingParentMutation = true;
        applyOptimisticParentChange(node.id, null);
        renderStatus("Saving", "status", "Parent relation removal pending.");
        postAction("set-parent", {
            recordId: node.id,
            parentId: null,
        })
            .then(() => {
                state.pendingParentMutation = false;
                renderSelection();
                renderStatus("Saved", "live", "Parent relation removed.");
                restartStream();
            })
            .catch((error) => {
                restoreParentState(relationSnapshot);
                renderStatus("Relation error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function applyFilters() {
        updateCategoryStateFromControls();
        state.draftFilters.parentHeadQuery = String(parentHeadQuery.value || "").trim();
        const rows = buildFilterRows();
        postAction("apply-filters", { filters: rows })
            .then((outcome) => {
                const filtersVersion = Number(outcome?.detail?.filters_version || 0);
                bridge?.patchCardState?.({
                    filters: rows,
                    filters_version: filtersVersion,
                });
                state.cardState.filters = rows;
                state.cardState.filters_version = filtersVersion;
                renderCreateForm();
                renderFilterPill();
                renderStatus("Saved", "live", "View filters updated.");
                restartStream();
            })
            .catch((error) => {
                renderStatus("Filter error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function clearFilters() {
        state.draftFilters = { categories: [], parentHeadQuery: "" };
        renderFilterControls();
        applyFilters();
    }

    function resetPhysics() {
        setPhysics({ ...DEFAULT_PHYSICS }, true);
    }

    function handleSnapshotEvent(data) {
        try {
            const snapshot = JSON.parse(data);
            syncSnapshot(snapshot);
            renderStatus("Live", "live", `${state.rows.length} nodes`);
        } catch (error) {
            renderStatus("Snapshot error", "error", error instanceof Error ? error.message : String(error));
        }
    }

    function connectStream() {
        if (!state.origin.serverId) {
            renderStatus("Missing origin", "error", "The widget needs a server binding.");
            return;
        }

        clearStream();
        renderStatus("Connecting", "status", "Opening the widget stream...");
        const source = new EventSource(graphUrl());
        state.stream = source;

        source.addEventListener("snapshot", (event) => {
            handleSnapshotEvent(event.data);
        });

        source.addEventListener("kanban-error", (event) => {
            try {
                const payload = JSON.parse(event.data);
                renderStatus("Stream error", "error", payload?.message || "The host rejected the stream payload.");
            } catch (error) {
                renderStatus("Stream error", "error", error instanceof Error ? error.message : String(error));
            }
        });

        source.addEventListener("error", () => {
            renderStatus("Stream error", "error", "The widget stream disconnected.");
        });
    }

    function handlePointerDown(event) {
        const rect = canvas.getBoundingClientRect();
        const screenX = event.clientX - rect.left;
        const screenY = event.clientY - rect.top;
        const node = event.button === 0 ? findNodeAtPoint(screenX, screenY) : null;
        if (node) {
            state.pointer = {
                kind: "node",
                pointerId: event.pointerId,
                node,
                startX: screenX,
                startY: screenY,
                moved: false,
            };
            node.fx = node.x;
            node.fy = node.y;
            canvas.setPointerCapture(event.pointerId);
            canvas.classList.add("is-dragging");
            if (state.simulation) {
                state.simulation.alphaTarget(0.2).restart();
            }
            return;
        }

        if (event.button !== 0 && event.button !== 1) {
            return;
        }

        state.pointer = {
            kind: "pan",
            pointerId: event.pointerId,
            startX: screenX,
            startY: screenY,
            originX: state.viewport.x,
            originY: state.viewport.y,
            moved: false,
        };
        canvas.setPointerCapture(event.pointerId);
        canvas.classList.add("is-dragging");
    }

    function handlePointerMove(event) {
        const pointer = state.pointer;
        if (!pointer || pointer.pointerId !== event.pointerId) {
            return;
        }
        const rect = canvas.getBoundingClientRect();
        const screenX = event.clientX - rect.left;
        const screenY = event.clientY - rect.top;
        const deltaX = screenX - pointer.startX;
        const deltaY = screenY - pointer.startY;
        if (Math.hypot(deltaX, deltaY) > 3) {
            pointer.moved = true;
        }

        if (pointer.kind === "node") {
            const world = screenToWorld(screenX, screenY);
            pointer.node.x = world.x;
            pointer.node.y = world.y;
            pointer.node.fx = world.x;
            pointer.node.fy = world.y;
            pointer.node.vx = 0;
            pointer.node.vy = 0;
            requestDraw();
            return;
        }

        state.viewport.x = pointer.originX + deltaX;
        state.viewport.y = pointer.originY + deltaY;
        requestDraw();
    }

    function handlePointerUp(event) {
        const pointer = state.pointer;
        if (!pointer || pointer.pointerId !== event.pointerId) {
            return;
        }
        canvas.classList.remove("is-dragging");
        if (pointer.kind === "node") {
            pointer.node.fx = null;
            pointer.node.fy = null;
            if (!pointer.moved) {
                selectNode(pointer.node);
            }
        } else if (!pointer.moved) {
            selectNode(null);
        }
        if (state.simulation) {
            state.simulation.alpha(Math.max(state.simulation.alpha(), 0.16)).alphaTarget(0).restart();
        }
        state.pointer = null;
        if (canvas.hasPointerCapture(event.pointerId)) {
            canvas.releasePointerCapture(event.pointerId);
        }
    }

    function bindEvents() {
        createOpenButton.addEventListener("click", openCreatePanel);
        createHeadInput.addEventListener("input", () => {
            state.createDraft.head = String(createHeadInput.value || "");
            renderCreateForm();
        });
        createBodyInput.addEventListener("input", () => {
            state.createDraft.body = String(createBodyInput.value || "");
        });
        createQuantityInput.addEventListener("input", () => {
            state.createDraft.quantity = String(createQuantityInput.value || "0");
        });
        createClearButton.addEventListener("click", () => {
            resetCreateDraft();
            renderCreateForm();
        });
        createSubmitButton.addEventListener("click", submitCreateRecord);
        createHeadInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                event.preventDefault();
                submitCreateRecord();
            }
        });
        createBodyInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                event.preventDefault();
                submitCreateRecord();
            }
        });
        categoryFilterList.addEventListener("change", updateCategoryStateFromControls);
        categoryAddButton?.addEventListener("click", addCategoryFromInput);
        categoryInput?.addEventListener("keydown", (event) => {
            if (event.key === "Enter") {
                event.preventDefault();
                addCategoryFromInput();
            }
        });
        selectedCategoryList.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-category-remove]");
            if (!button) {
                return;
            }
            removeCategory(button.getAttribute("data-category-remove"));
        });
        parentHeadQuery.addEventListener("input", () => {
            state.draftFilters.parentHeadQuery = String(parentHeadQuery.value || "").trim();
            renderFilterPill();
        });
        parentSearchInput.addEventListener("input", () => {
            state.parentSearchQuery = String(parentSearchInput.value || "").trim();
            const node = state.nodes.find((item) => item.id === state.selectedId) || null;
            renderParentChoices(node);
        });
        parentChoiceList.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-parent-choice]");
            if (!button) {
                return;
            }
            state.selectedParentId = Number(button.getAttribute("data-parent-choice") || 0) || null;
            renderSelection();
        });
        applyFiltersButton.addEventListener("click", applyFilters);
        clearFiltersButton.addEventListener("click", clearFilters);
        resetPhysicsButton.addEventListener("click", resetPhysics);
        connectParentButton.addEventListener("click", connectParent);
        disconnectParentButton.addEventListener("click", disconnectParent);
        panelToggleButton.addEventListener("click", () => {
            state.panelOpen = !state.panelOpen;
            renderPanel();
        });
        panelCloseButton.addEventListener("click", () => {
            state.panelOpen = false;
            renderPanel();
        });
        zoomInButton.addEventListener("click", () => {
            zoomAt(state.width / 2, state.height / 2, 1.18);
        });
        zoomOutButton.addEventListener("click", () => {
            zoomAt(state.width / 2, state.height / 2, 1 / 1.18);
        });
        zoomFitButton.addEventListener("click", fitViewportToNodes);

        for (const [input, key] of [
            [chargeInput, "charge"],
            [distanceInput, "linkDistance"],
            [collisionInput, "collisionRadius"],
            [centerInput, "centerForce"],
        ]) {
            input.addEventListener("input", () => {
                state.physics[key] = Number(input.value);
                applyPhysicsToUi();
                applySimulationForces();
                nudgeSimulation(0.08);
                requestDraw();
            });
            input.addEventListener("change", () => {
                state.physics[key] = Number(input.value);
                setPhysics({ ...state.physics }, true);
            });
        }

        canvas.addEventListener("wheel", (event) => {
            event.preventDefault();
            const rect = canvas.getBoundingClientRect();
            const screenX = event.clientX - rect.left;
            const screenY = event.clientY - rect.top;
            const sensitivity = event.ctrlKey ? 0.0024 : 0.0016;
            const factor = Math.exp(-event.deltaY * sensitivity);
            zoomAt(screenX, screenY, factor);
        }, { passive: false });

        canvas.addEventListener("pointerdown", handlePointerDown);
        canvas.addEventListener("pointermove", handlePointerMove);
        canvas.addEventListener("pointerup", handlePointerUp);
        canvas.addEventListener("pointercancel", handlePointerUp);
        window.addEventListener("resize", resizeCanvas);
    }

    function start() {
        renderMode();
        renderPanel();
        renderStatus("Booting", "status", "Waiting for the widget stream.");
        bindEvents();
        syncFromBridge();
        bridge?.subscribe?.(() => syncFromBridge());
        bridge?.requestState?.();
        renderCreateForm();
        renderPhysicsControls();
        renderFilterControls();
        renderSelection();
        renderZoomPill();
        connectStream();
        state.resizeObserver = new ResizeObserver(() => {
            resizeCanvas();
        });
        state.resizeObserver.observe(app);
        resizeCanvas();
    }

    start();
    window.RelationsWidget = {
        state,
        restartStream,
        applyFilters,
        connectParent,
        disconnectParent,
        fitViewportToNodes,
    };
})();
"##.to_string()
}
