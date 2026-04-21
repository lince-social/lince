pub(super) fn script() -> String {
    r##"
(function () {
    const bridge = window.LinceWidgetHost || null;
    const frame = window.frameElement;
    const app = document.getElementById("app");
    const canvas = document.getElementById("graph");
    const ctx = canvas.getContext("2d");
    const createPanel = document.getElementById("create-panel");
    const createCloseButton = document.getElementById("create-close");
    const controlsPanel = document.getElementById("controls-panel");
    const recordPanel = document.getElementById("record-panel");
    const panelToggleButton = document.getElementById("panel-toggle");
    const panelCloseButton = document.getElementById("panel-close");
    const recordCloseButton = document.getElementById("record-close");
    const createOpenButton = document.getElementById("create-open");
    const modePill = document.getElementById("mode-pill");
    const originPill = document.getElementById("origin-pill");
    const rowPill = document.getElementById("row-pill");
    const linkPill = document.getElementById("link-pill");
    const filterPill = document.getElementById("filter-pill");
    const emptyState = document.getElementById("empty-state");
    const originText = document.getElementById("origin-text");
    const viewSqlText = document.getElementById("view-sql");
    const createSummary = document.getElementById("create-summary");
    const createHeadInput = document.getElementById("create-head");
    const createBodyInput = document.getElementById("create-body");
    const createQuantityInput = document.getElementById("create-quantity");
    const createParentSearchInput = document.getElementById("create-parent-search");
    const createParentSummary = document.getElementById("create-parent-summary");
    const createParentChoiceList = document.getElementById("create-parent-choice-list");
    const createParentClearButton = document.getElementById("create-parent-clear");
    const createCategoryList = document.getElementById("create-category-list");
    const createClearButton = document.getElementById("create-clear");
    const createSubmitButton = document.getElementById("create-submit");
    const recordIdDisplay = document.getElementById("record-id");
    const recordHeadInput = document.getElementById("record-head");
    const recordBodyInput = document.getElementById("record-body");
    const recordQuantityInput = document.getElementById("record-quantity");
    const recordCategoryInput = document.getElementById("record-category-input");
    const recordCategoryList = document.getElementById("record-category-list");
    const recordSaveButton = document.getElementById("record-save");
    const recordDeleteButton = document.getElementById("record-delete");
    const parentSearchInput = document.getElementById("parent-search-query");
    const parentSearchSummary = document.getElementById("parent-search-summary");
    const parentChoiceList = document.getElementById("parent-choice-list");
    const currentParentList = document.getElementById("current-parent-list");
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
    const controlsResizer = document.getElementById("controls-resizer");
    const recordResizer = document.getElementById("record-resizer");
    const zoomInButton = document.getElementById("zoom-in");
    const zoomOutButton = document.getElementById("zoom-out");
    const zoomFitButton = document.getElementById("zoom-fit");

    const DEFAULT_PHYSICS = {
        charge: -220,
        linkDistance: 110,
        collisionRadius: 24,
        centerForce: 0.04,
    };
    const CARD_STATE_KEY = "relations";
    const DEFAULT_PANEL_WIDTH = 380;
    const MIN_PANEL_WIDTH = 280;
    const MAX_PANEL_RATIO = 0.47;
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
            viewName: String(frame?.dataset?.linceViewName || "").trim(),
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
        selectedRemovalParentId: null,
        parentSearchQuery: "",
        recordDetail: null,
        recordDetailLoading: false,
        recordDetailError: "",
        recordDetailRequestId: 0,
        recordDraft: {
            recordId: null,
            head: "",
            body: "",
            quantity: "0",
            categories: [],
        },
        recordDraftDirty: false,
        recordCategoryInput: "",
        pendingRecordSave: false,
        pendingRecordDelete: false,
        recordDeleteArmed: false,
        createDraft: {
            head: "",
            body: "",
            quantity: "0",
            parentId: null,
        },
        createParentSearchQuery: "",
        pendingCreate: false,
        pendingParentMutation: false,
        panelWidths: {
            controls: readLocalPanelWidth("controls"),
            record: readLocalPanelWidth("record"),
        },
        panelResize: null,
        controlsPanelOpen: false,
        recordPanelOpen: false,
        createPanelOpen: false,
        stream: null,
        streamGeneration: 0,
        error: "",
        width: 0,
        height: 0,
        physicsPersistTimer: null,
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

    function uniqueIntegers(values) {
        const seen = new Set();
        const out = [];
        for (const raw of Array.isArray(values) ? values : []) {
            const value = Number(raw);
            if (!Number.isFinite(value) || value <= 0) {
                continue;
            }
            if (seen.has(value)) {
                continue;
            }
            seen.add(value);
            out.push(value);
        }
        return out;
    }

    function normalizeStringArray(values) {
        return uniqueStrings(Array.isArray(values) ? values : []);
    }

    function parseStringArray(raw) {
        if (Array.isArray(raw)) {
            return normalizeStringArray(raw);
        }

        const text = String(raw || "").trim();
        if (!text) {
            return [];
        }

        try {
            const parsed = JSON.parse(text);
            if (Array.isArray(parsed)) {
                return normalizeStringArray(parsed);
            }
        } catch {
        }

        return normalizeStringArray(text.split(","));
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

    function asObject(value) {
        return value && typeof value === "object" && !Array.isArray(value) ? value : {};
    }

    function readCardState(detail, sourceMeta) {
        const fromMeta = asObject(sourceMeta).cardState;
        const fromDetail = asObject(detail).cardState;
        const fromBridge = bridge?.getCardState?.();
        return asObject(fromMeta || fromDetail || fromBridge);
    }

    function readRelationsState(cardState) {
        return asObject(asObject(cardState)[CARD_STATE_KEY]);
    }

    function localPhysicsStorageKey() {
        return `lince.widget.relations.${instanceId()}.physics`;
    }

    function readLocalPhysics() {
        try {
            const raw = window.localStorage?.getItem?.(localPhysicsStorageKey());
            if (!raw) {
                return null;
            }
            const parsed = JSON.parse(raw);
            return parsed && typeof parsed === "object" ? parsed : null;
        } catch {
            return null;
        }
    }

    function writeLocalPhysics(physics) {
        try {
            window.localStorage?.setItem?.(
                localPhysicsStorageKey(),
                JSON.stringify({ ...normalizePhysics(physics) }),
            );
        } catch {
        }
    }

    function localPanelWidthStorageKey(side) {
        return `lince.widget.relations.${instanceId()}.panel-width.${side}`;
    }

    function panelWidthLimit() {
        const sandboxWidth = Math.max(
            0,
            Number(app?.getBoundingClientRect?.().width || 0) || Number(window.innerWidth || 0),
        );
        const ratioLimit = Math.floor(sandboxWidth * MAX_PANEL_RATIO);
        if (!Number.isFinite(ratioLimit) || ratioLimit <= 0) {
            return DEFAULT_PANEL_WIDTH;
        }
        return Math.max(MIN_PANEL_WIDTH, ratioLimit);
    }

    function normalizePanelWidth(value) {
        return clampNumber(value, MIN_PANEL_WIDTH, panelWidthLimit(), DEFAULT_PANEL_WIDTH);
    }

    function readLocalPanelWidth(side) {
        try {
            const raw = window.localStorage?.getItem?.(localPanelWidthStorageKey(side));
            if (!raw) {
                return DEFAULT_PANEL_WIDTH;
            }
            return normalizePanelWidth(Number(raw));
        } catch {
            return DEFAULT_PANEL_WIDTH;
        }
    }

    function writeLocalPanelWidth(side, width) {
        try {
            window.localStorage?.setItem?.(localPanelWidthStorageKey(side), String(normalizePanelWidth(width)));
        } catch {
        }
    }

    function readHostPhysics(cardState) {
        const scoped = readRelationsState(cardState);
        return (
            scoped.physics ||
            scoped.relations_physics ||
            scoped.relationsPhysics ||
            asObject(cardState).relations_physics ||
            asObject(cardState).physics ||
            asObject(cardState).relationsPhysics ||
            null
        );
    }

    function samePhysics(left, right) {
        const a = normalizePhysics(left);
        const b = normalizePhysics(right);
        return (
            a.charge === b.charge &&
            a.linkDistance === b.linkDistance &&
            a.collisionRadius === b.collisionRadius &&
            a.centerForce === b.centerForce
        );
    }

    function readPersistedPhysics(cardState) {
        const local = readLocalPhysics();
        if (local) {
            return normalizePhysics(local);
        }
        return normalizePhysics(readHostPhysics(cardState));
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

    function resetRecordEditorState() {
        state.recordDetail = null;
        state.recordDetailLoading = false;
        state.recordDetailError = "";
        state.recordDraft = {
            recordId: null,
            head: "",
            body: "",
            quantity: "0",
            categories: [],
        };
        state.recordDraftDirty = false;
        state.recordCategoryInput = "";
        state.pendingRecordSave = false;
        state.pendingRecordDelete = false;
        state.recordDeleteArmed = false;
    }

    function buildRecordDraft(node, detail) {
        const quantitySource = detail?.quantity ?? node?.quantity ?? 0;
        const categoriesSource = Array.isArray(detail?.categories)
            ? detail.categories
            : Array.isArray(node?.categories)
              ? node.categories
              : [];
        return {
            recordId: Number(node?.id || detail?.record_id || 0) || null,
            head: String(detail?.head ?? node?.head ?? ""),
            body: String(detail?.body ?? node?.body ?? ""),
            quantity: String(parseOptionalInteger(quantitySource) ?? 0),
            categories: uniqueStrings(categoriesSource),
        };
    }

    function sortedParentCandidates(needle, excludeIds) {
        const blocked = excludeIds instanceof Set ? excludeIds : new Set();
        const loweredNeedle = String(needle || "").trim().toLowerCase();
        return state.nodes
            .filter((candidate) => !blocked.has(candidate.id))
            .filter((candidate) => {
                if (!loweredNeedle) {
                    return true;
                }
                const haystacks = [
                    String(candidate.head || ""),
                    ...(Array.isArray(candidate.categories) ? candidate.categories : []),
                ];
                return haystacks.some((value) => String(value || "").toLowerCase().includes(loweredNeedle));
            })
            .slice()
            .sort((left, right) =>
                String(left.head || "")
                    .toLowerCase()
                    .localeCompare(String(right.head || "").toLowerCase()) ||
                left.id - right.id,
            );
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
    }

    function renderOrigin() {
        const serverId = state.origin.serverId || "local";
        const viewId = state.origin.viewId == null ? "none" : String(state.origin.viewId);
        const viewName = state.origin.viewName || "";
        originPill.textContent = viewName ? `${serverId} / ${viewName}` : `${serverId} / view ${viewId}`;
        originText.textContent = `serverId: ${serverId}\nviewId: ${viewId}\nviewName: ${viewName || "none"}\nmode: ${state.mode}`;
    }

    function renderStatus(label, tone, detail) {
        const safeLabel = String(label || "Status").trim() || "Status";
        const safeDetail = String(detail || "").trim();
        panelToggleButton.dataset.tone = tone || "neutral";
        panelToggleButton.dataset.detail = safeDetail;
        panelToggleButton.dataset.label = safeLabel;
        panelToggleButton.setAttribute("aria-label", safeDetail ? `${safeLabel}: ${safeDetail}` : safeLabel);
        panelToggleButton.title = safeDetail ? `${safeLabel}: ${safeDetail}` : safeLabel;
    }

    function renderCounters() {
        rowPill.textContent = `${state.rows.length} nodes`;
        linkPill.textContent = `${state.links.length} links`;
        filterPill.textContent = `${buildFilterRows().length} filters`;
    }

    function renderZoomPill() {
        zoomFitButton.textContent = `${Math.round(state.viewport.scale * 100)}%`;
        zoomFitButton.setAttribute("aria-label", "Fit to nodes");
        zoomFitButton.title = "Fit to nodes";
    }

    function applyPanelWidths() {
        if ((window.innerWidth || 0) <= 980) {
            controlsPanel?.style.removeProperty("width");
            recordPanel?.style.removeProperty("width");
            return;
        }
        if (controlsPanel) {
            controlsPanel.style.width = `${normalizePanelWidth(state.panelWidths.controls)}px`;
        }
        if (recordPanel) {
            recordPanel.style.width = `${normalizePanelWidth(state.panelWidths.record)}px`;
        }
    }

    function renderPanel() {
        applyPanelWidths();
        controlsPanel.hidden = !state.controlsPanelOpen;
        recordPanel.hidden = !state.recordPanelOpen || !state.selectedId;
        createPanel.hidden = !state.createPanelOpen;
        panelToggleButton.dataset.open = state.controlsPanelOpen ? "true" : "false";
        createOpenButton.dataset.open = state.createPanelOpen ? "true" : "false";
    }

    function startPanelResize(side, event) {
        if (!side) {
            return;
        }
        if ((window.innerWidth || 0) <= 980) {
            return;
        }
        event.preventDefault();
        state.panelResize = {
            side,
            pointerId: event.pointerId,
            startX: event.clientX,
            startWidth: state.panelWidths[side] || DEFAULT_PANEL_WIDTH,
        };
        document.body.classList.add("is-resizing-panels");
    }

    function updatePanelResize(event) {
        const resize = state.panelResize;
        if (!resize || resize.pointerId !== event.pointerId) {
            return;
        }
        const delta = event.clientX - resize.startX;
        const nextWidth = resize.side === "controls"
            ? resize.startWidth + delta
            : resize.startWidth - delta;
        state.panelWidths[resize.side] = normalizePanelWidth(nextWidth);
        applyPanelWidths();
    }

    function endPanelResize(event) {
        const resize = state.panelResize;
        if (!resize || resize.pointerId !== event.pointerId) {
            return;
        }
        state.panelResize = null;
        document.body.classList.remove("is-resizing-panels");
        state.panelWidths[resize.side] = normalizePanelWidth(state.panelWidths[resize.side]);
        writeLocalPanelWidth(resize.side, state.panelWidths[resize.side]);
        applyPanelWidths();
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
        const draftParent = state.nodes.find((node) => node.id === state.createDraft.parentId) || null;
        createHeadInput.value = String(state.createDraft.head || "");
        createBodyInput.value = String(state.createDraft.body || "");
        createQuantityInput.value = String(state.createDraft.quantity || "0");
        createParentSearchInput.value = String(state.createParentSearchQuery || "");
        createCategoryList.innerHTML = categories.length
            ? categories
                  .map((category) => `<span class="pill">${escapeHtml(category)}</span>`)
                  .join("")
            : '<span class="mutedCopy">No applied categories. New records will be created without category links.</span>';
        createParentSummary.textContent = draftParent
            ? `Selected: #${draftParent.id} ${draftParent.head || "Untitled"}`
            : "Selected: no parent";
        createSummary.textContent = !state.origin.serverId
            ? "This widget needs a configured server before it can create records."
            : categories.length
              ? `New records will inherit ${categories.length} applied categor${categories.length === 1 ? "y" : "ies"} from this view.`
              : "No category filter is currently applied to this view.";
        renderCreateParentChoices();
        createParentClearButton.disabled = state.pendingCreate || state.createDraft.parentId == null;
        createClearButton.disabled = state.pendingCreate;
        createSubmitButton.disabled = state.pendingCreate || !state.origin.serverId || !String(state.createDraft.head || "").trim();
    }

    function renderCreateParentChoices() {
        const candidates = sortedParentCandidates(state.createParentSearchQuery, new Set());
        if (!candidates.length) {
            createParentChoiceList.innerHTML = '<p class="mutedCopy">No possible fathers match the current search.</p>';
            return;
        }

        createParentChoiceList.innerHTML = candidates
            .map((candidate) => {
                const isSelected = candidate.id === state.createDraft.parentId;
                const categories = Array.isArray(candidate.categories) && candidate.categories.length
                    ? candidate.categories.join(", ")
                    : "No categories";
                return `
                    <button class="parentChoice${isSelected ? " is-selected" : ""}" type="button" data-create-parent-choice="${candidate.id}">
                        <span class="parentChoice__head">#${candidate.id} ${escapeHtml(candidate.head || "Untitled")}</span>
                        <span class="parentChoice__meta">${escapeHtml(categories)}</span>
                    </button>
                `;
            })
            .join("");
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

    function renderRecordCategories() {
        const selected = Array.isArray(state.recordDraft.categories) ? state.recordDraft.categories : [];
        recordCategoryList.innerHTML = selected.length
            ? selected
                  .map((category) => {
                      const safe = escapeHtml(category);
                      return `
                          <button class="chipButton" type="button" data-record-category-remove="${safe}" title="Remove category">
                              <span>${safe}</span>
                              <span aria-hidden="true">×</span>
                          </button>
                      `;
                  })
                  .join("")
            : "";
    }

    function renderRecordEditor(node) {
        const detail = Number(state.recordDetail?.record_id || 0) === Number(node?.id || 0)
            ? state.recordDetail
            : null;
        const ready = Boolean(node) && Boolean(detail);
        const pending = state.pendingRecordSave || state.pendingRecordDelete;

        if (recordIdDisplay) {
            recordIdDisplay.textContent = node ? String(node.id) : "-";
        }
        recordHeadInput.value = String(state.recordDraft.head || "");
        recordBodyInput.value = String(state.recordDraft.body || "");
        recordQuantityInput.value = String(state.recordDraft.quantity || "0");
        recordCategoryInput.value = String(state.recordCategoryInput || "");
        renderRecordCategories();

        const disabled = !node || pending;
        recordHeadInput.disabled = disabled;
        recordBodyInput.disabled = disabled;
        recordQuantityInput.disabled = disabled;
        recordCategoryInput.disabled = disabled;
        recordSaveButton.disabled = disabled || !ready || !String(state.recordDraft.head || "").trim();
        recordDeleteButton.disabled = !node || pending;
        recordDeleteButton.textContent = state.recordDeleteArmed ? "Confirm delete" : "Delete record";
    }

    function renderParentChoices(node) {
        if (!node) {
            parentChoiceList.innerHTML = "";
            parentSearchSummary.textContent = "Choose a possible father from the current graph.";
            return;
        }

        const blockedIds = collectDescendantIds(node.id);
        const currentParentIds = uniqueIntegers(
            Array.isArray(node.parentIds) ? node.parentIds : [node.parentId],
        );
        const candidates = sortedParentCandidates(
            state.parentSearchQuery,
            new Set([...blockedIds, node.id, ...currentParentIds]),
        );

        const selectedParent = state.nodes.find((candidate) => candidate.id === state.selectedParentId) || null;
        parentSearchSummary.textContent = selectedParent
            ? `Target father: #${selectedParent.id} ${selectedParent.head || "Untitled"}`
            : "Choose a possible father from the current graph.";

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
                    <div class="relationRow parentChoice${isSelected ? " is-selected" : ""}">
                        <div class="relationRow__body">
                            <span class="relationRow__head">#${candidate.id} ${escapeHtml(candidate.head || "Untitled")}</span>
                            <span class="relationRow__meta">${escapeHtml(categories)}</span>
                        </div>
                        <button class="relationRow__action relationRow__action--add" type="button" data-parent-choice="${candidate.id}" aria-label="Set parent ${escapeHtml(candidate.head || "Untitled")}" title="Set parent">
                            +
                        </button>
                    </div>
                `;
            })
            .join("");
    }

    function renderCurrentParentList(node) {
        if (!currentParentList) {
            return [];
        }
        const parentIds = uniqueIntegers(
            node ? (Array.isArray(node.parentIds) ? node.parentIds : [node.parentId]) : [],
        );
        const currentParents = parentIds
            .map((parentId) => state.nodes.find((candidate) => candidate.id === parentId) || null)
            .filter(Boolean);
        if (!currentParents.length) {
            currentParentList.innerHTML = '<span class="mutedCopy">No current parents.</span>';
            return currentParents;
        }

        currentParentList.innerHTML = currentParents
            .map((parent) => {
                const isSelected = parent.id === state.selectedRemovalParentId;
                const categories = Array.isArray(parent.categories) && parent.categories.length
                    ? parent.categories.join(", ")
                    : "No categories";
                return `
                    <div class="relationRow currentParent${isSelected ? " is-selected" : ""}">
                        <div class="relationRow__body">
                            <span class="relationRow__head">#${parent.id} ${escapeHtml(parent.head || "Untitled")}</span>
                            <span class="relationRow__meta">${escapeHtml(categories)}</span>
                        </div>
                        <button class="relationRow__action relationRow__action--remove" type="button" data-parent-remove="${parent.id}" aria-label="Remove parent ${escapeHtml(parent.head || "Untitled")}" title="Remove parent">
                            ×
                        </button>
                    </div>
                `;
            })
            .join("");
        return currentParents;
    }

    function loadRecordDetail(recordId) {
        const targetId = Number(recordId || 0);
        if (!targetId) {
            return;
        }
        const requestId = state.recordDetailRequestId + 1;
        state.recordDetailRequestId = requestId;
        state.recordDetailLoading = true;
        state.recordDetailError = "";
        renderRecordEditor(state.nodes.find((item) => item.id === state.selectedId) || null);

        postAction("load-record-detail", { recordId: targetId })
            .then((detail) => {
                if (state.recordDetailRequestId !== requestId || state.selectedId !== targetId) {
                    return;
                }
                state.recordDetail = detail && typeof detail === "object" ? detail : null;
                state.recordDetailLoading = false;
                state.recordDetailError = "";
                if (!state.recordDraftDirty || state.recordDraft.recordId !== targetId) {
                    const node = state.nodes.find((item) => item.id === targetId) || null;
                    state.recordDraft = buildRecordDraft(node, state.recordDetail);
                    state.recordDraftDirty = false;
                }
                renderSelection();
            })
            .catch((error) => {
                if (state.recordDetailRequestId !== requestId || state.selectedId !== targetId) {
                    return;
                }
                state.recordDetailLoading = false;
                state.recordDetailError = error instanceof Error ? error.message : String(error);
                renderSelection();
            });
    }

    function syncSelectionRecordEditor(node) {
        if (!node) {
            if (state.recordDraft.recordId != null || state.recordDetail || state.recordDetailLoading || state.recordDetailError) {
                resetRecordEditorState();
            }
            return;
        }

        if (state.recordDraft.recordId !== node.id) {
            state.recordDetail = null;
            state.recordDetailLoading = false;
            state.recordDetailError = "";
            state.recordDraft = buildRecordDraft(node, null);
            state.recordDraftDirty = false;
            state.recordCategoryInput = "";
            state.pendingRecordSave = false;
            state.pendingRecordDelete = false;
            state.recordDeleteArmed = false;
            loadRecordDetail(node.id);
            return;
        }

        if (!state.recordDetail && !state.recordDetailLoading && !state.recordDetailError) {
            loadRecordDetail(node.id);
        }
    }

    function renderSelection() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        syncSelectionRecordEditor(node);
        if (!node) {
            state.recordPanelOpen = false;
            childList.innerHTML = "";
            if (currentParentList) {
                currentParentList.innerHTML = "";
            }
            parentChoiceList.innerHTML = "";
            parentSearchSummary.textContent = "Choose a possible father from the current graph.";
            renderRecordEditor(null);
            renderPanel();
            return;
        }

        if (state.selectedParentId != null && !state.nodes.some((item) => item.id === state.selectedParentId)) {
            state.selectedParentId = null;
        }
        if (state.selectedRemovalParentId != null && !state.nodes.some((item) => item.id === state.selectedRemovalParentId)) {
            state.selectedRemovalParentId = null;
        }

        parentSearchInput.value = state.parentSearchQuery;

        renderCurrentParentList(node);
        const children = Array.isArray(node.children) ? node.children : [];

        childList.innerHTML = children.length
            ? children
                  .map((child) => `<span class="pill">#${escapeHtml(child.id)} ${escapeHtml(child.head || "Untitled")}</span>`)
                  .join("")
            : '<span class="mutedCopy">No children.</span>';

        renderRecordEditor(node);
        renderParentChoices(node);
    }

    function renderFilterPill() {
        renderCounters();
    }

    function syncFromBridge(detail) {
        const rawDetail = asObject(detail);
        const meta = rawDetail.meta && typeof rawDetail.meta === "object" ? rawDetail.meta : getMeta();
        const cardState = readCardState(rawDetail, meta);
        const hostPhysics = normalizePhysics(readHostPhysics(cardState));
        state.mode = meta.mode === "edit" ? "edit" : "view";
        state.cardState = cardState;
        state.draftFilters = normalizeDraftFilters(state.cardState.filters || []);
        state.physics = readPersistedPhysics(state.cardState);
        state.origin.serverId = String(meta.serverId || state.origin.serverId || frame?.dataset?.linceServerId || "").trim();
        state.origin.viewId = Number(meta.viewId || state.origin.viewId || frame?.dataset?.linceViewId || 0) || null;
        state.origin.viewName = String(
            meta.viewName ||
                state.origin.viewName ||
                frame?.dataset?.linceViewName ||
                "",
        ).trim();
        renderMode();
        renderOrigin();
        renderFilterControls();
        renderCreateForm();
        renderPhysicsControls();
        renderFilterPill();
        renderPanel();
        if (!samePhysics(state.physics, hostPhysics)) {
            persistPhysicsSoon();
        }
        if (!state.stream && state.origin.serverId) {
            connectStream();
        }
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

    function removeRecordCategory(value) {
        const needle = String(value || "").trim().toLowerCase();
        if (!needle) {
            return;
        }
        state.recordDraft.categories = state.recordDraft.categories.filter(
            (category) => category.toLowerCase() !== needle,
        );
        state.recordDraftDirty = true;
        state.recordDeleteArmed = false;
        renderRecordEditor(state.nodes.find((item) => item.id === state.selectedId) || null);
    }

    function applyOptimisticRecordDraft(node, draft) {
        if (!node || !draft) {
            return;
        }
        node.head = String(draft.head || "").trim() || "Untitled";
        node.body = String(draft.body || "");
        node.quantity = parseOptionalInteger(draft.quantity) ?? 0;
        node.categories = uniqueStrings(draft.categories);
        state.categories = collectCategories(state.nodes);
        state.availableCategories = state.categories;
        renderCategoryChoices();
        renderCreateForm();
        renderCounters();
        requestDraw();
    }

    function commitRecordCategoryInput() {
        const raw = String(recordCategoryInput?.value || "").trim();
        if (!raw) {
            return false;
        }

        const nextCategories = uniqueStrings([
            ...state.recordDraft.categories,
            ...raw.split(","),
        ]);
        state.recordDraft.categories = nextCategories;
        state.recordCategoryInput = "";
        if (recordCategoryInput) {
            recordCategoryInput.value = "";
        }
        state.recordDraftDirty = true;
        state.recordDeleteArmed = false;
        renderRecordCategories();
        return true;
    }

    function saveSelectedRecord() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        const detail = Number(state.recordDetail?.record_id || 0) === Number(node?.id || 0)
            ? state.recordDetail
            : null;
        if (!node || !detail) {
            renderStatus("Record not ready", "error", "Load the selected record before saving edits.");
            return;
        }

        commitRecordCategoryInput();
        if (!state.recordDraftDirty) {
            return;
        }

        const head = String(state.recordDraft.head || "").trim();
        if (!head) {
            renderStatus("Missing head", "error", "A record needs a head.");
            recordHeadInput?.focus();
            return;
        }

        const quantity = parseOptionalInteger(state.recordDraft.quantity) ?? 0;
        const assigneeIds = Array.isArray(detail.assignees)
            ? detail.assignees
                  .map((entry) => Number(entry?.id || 0))
                  .filter((value) => Number.isInteger(value) && value > 0)
            : [];

        state.pendingRecordSave = true;
        state.recordDeleteArmed = false;
        renderRecordEditor(node);
        renderStatus("Saving", "status", "Record update pending.");
        postAction("update-record", {
            recordId: node.id,
            head,
            body: String(state.recordDraft.body || "").trim(),
            quantity,
            taskType: detail.task_type || null,
            categories: state.recordDraft.categories.slice(),
            startAt: detail.start_at || null,
            endAt: detail.end_at || null,
            estimateSeconds: Number.isInteger(detail.estimate_seconds) ? detail.estimate_seconds : null,
            assigneeIds,
        })
            .then(() => {
                state.pendingRecordSave = false;
                state.recordDraftDirty = false;
                state.recordDeleteArmed = false;
                applyOptimisticRecordDraft(node, state.recordDraft);
                state.recordDetail = {
                    ...detail,
                    head,
                    body: String(state.recordDraft.body || "").trim() || null,
                    quantity,
                    categories: state.recordDraft.categories.slice(),
                    primary_category: state.recordDraft.categories[0] || null,
                };
                renderSelection();
                renderStatus("Saved", "live", "Record updated.");
                restartStream();
            })
            .catch((error) => {
                state.pendingRecordSave = false;
                state.recordDeleteArmed = false;
                renderRecordEditor(node);
                renderStatus("Update error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function deleteSelectedRecord() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            return;
        }
        if (!state.recordDeleteArmed) {
            state.recordDeleteArmed = true;
            renderRecordEditor(node);
            renderStatus("Confirm delete", "status", "Click delete again to remove the selected record.");
            return;
        }

        state.pendingRecordDelete = true;
        state.recordDeleteArmed = false;
        renderRecordEditor(node);
        renderStatus("Deleting", "status", "Record deletion pending.");
        postAction("delete-record", { recordId: node.id })
            .then(() => {
                state.pendingRecordDelete = false;
                state.selectedId = null;
                state.selectedParentId = null;
                state.parentSearchQuery = "";
                resetRecordEditorState();
                renderSelection();
                renderStatus("Deleted", "live", "Record removed.");
                restartStream();
            })
            .catch((error) => {
                state.pendingRecordDelete = false;
                renderRecordEditor(node);
                renderStatus("Delete error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function persistPhysics() {
        if (state.physicsPersistTimer) {
            window.clearTimeout(state.physicsPersistTimer);
            state.physicsPersistTimer = null;
        }
        writeLocalPhysics(state.physics);
        const nextRelationsState = {
            ...readRelationsState(state.cardState),
            physics: { ...state.physics },
        };
        state.cardState = {
            ...asObject(state.cardState),
            [CARD_STATE_KEY]: nextRelationsState,
        };
        bridge?.patchCardState?.({
            [CARD_STATE_KEY]: nextRelationsState,
        });
    }

    function persistPhysicsSoon() {
        if (state.physicsPersistTimer) {
            window.clearTimeout(state.physicsPersistTimer);
        }
        state.physicsPersistTimer = window.setTimeout(() => {
            state.physicsPersistTimer = null;
            persistPhysics();
        }, 140);
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
            parentId: null,
        };
        state.createParentSearchQuery = "";
    }

    function openCreatePanel() {
        if (state.createDraft.parentId == null && state.selectedId && state.nodes.some((node) => node.id === state.selectedId)) {
            state.createDraft.parentId = state.selectedId;
        }
        state.createPanelOpen = true;
        renderPanel();
        renderCreateForm();
        window.requestAnimationFrame(() => {
            document.getElementById("creator")?.scrollIntoView?.({ block: "start", behavior: "auto" });
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
            parentId: Number.isInteger(state.createDraft.parentId) && state.createDraft.parentId > 0
                ? state.createDraft.parentId
                : null,
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

    function parseIntegerArray(raw) {
        return uniqueIntegers(parseJsonArray(raw));
    }

    function parseRow(row) {
        const id = Number(parseCell(row, "id"));
        const quantity = Number(parseCell(row, "quantity"));
        if (!Number.isFinite(id) || !Number.isFinite(quantity)) {
            return null;
        }
        const categories = uniqueStrings([
            ...parseStringArray(parseCell(row, "categories_json")),
            ...parseStringArray(parseCell(row, "categories")),
            ...parseStringArray(parseCell(row, "primary_category")),
        ]);
        const declaredChildren = parseJsonArray(parseCell(row, "children_json"))
            .map((child) => ({
                id: Number(child?.id),
                head: String(child?.head || ""),
                quantity: Number(child?.quantity || 0),
                taskType: child?.task_type == null ? null : String(child.task_type),
            }))
            .filter((child) => Number.isFinite(child.id));
        const parentIds = uniqueIntegers([
            ...parseIntegerArray(parseCell(row, "parent_ids_json")),
            parseOptionalInteger(parseCell(row, "parent_id")),
        ]);
        const parentHeads = uniqueStrings([
            ...parseStringArray(parseCell(row, "parent_heads_json")),
            parseCell(row, "parent_head"),
        ]);
        const parentId = parentIds[0] || null;
        const parentHead = parentHeads[0] || null;

        return {
            id,
            quantity,
            head: parseCell(row, "head").trim() || "Untitled",
            body: parseCell(row, "body"),
            taskType: parseCell(row, "task_type") || null,
            categories,
            parentIds,
            parentHeads,
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

    function inferParentIdsFromNode(node, nodesById) {
        const explicitIds = uniqueIntegers([
            ...(Array.isArray(node.parentIds) ? node.parentIds : []),
            node.parentId,
        ]);
        if (explicitIds.length) {
            return explicitIds;
        }

        const heads = uniqueStrings([
            ...(Array.isArray(node.parentHeads) ? node.parentHeads : []),
            node.parentHead,
        ]);
        if (!heads.length) {
            return [];
        }

        const inferred = [];
        for (const head of heads) {
            const needle = String(head || "").trim().toLowerCase();
            if (!needle) {
                continue;
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
                    matchId = null;
                    break;
                }
                matchId = candidate.id;
            }

            if (matchId != null) {
                inferred.push(matchId);
            }
        }

        return uniqueIntegers(inferred);
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
            const parentIds = uniqueIntegers(
                Array.isArray(node.parentIds) ? node.parentIds : [node.parentId],
            );
            let nextDepth = null;
            if (!parentIds.length) {
                nextDepth = 0;
            } else {
                for (const parentId of parentIds) {
                    if (!nodesById.has(parentId)) {
                        continue;
                    }
                    const parentDepth = visit(nodesById.get(parentId));
                    if (parentDepth == null) {
                        continue;
                    }
                    const candidateDepth = parentDepth + 1;
                    nextDepth = nextDepth == null ? candidateDepth : Math.min(nextDepth, candidateDepth);
                }
                if (nextDepth == null) {
                    nextDepth = 0;
                }
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
            node.parentIds = inferParentIdsFromNode(node, nodesById);
            node.parentId = node.parentIds[0] || null;
            node.parentHead = node.parentHeads && node.parentHeads.length
                ? node.parentHeads[0]
                : node.parentHead;
        }

        for (const node of parsedRows) {
            for (const parentId of uniqueIntegers(node.parentIds)) {
                const parent = nodesById.get(parentId);
                if (!parent || parent.id === node.id) {
                    continue;
                }
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
            const parentIds = uniqueIntegers(
                Array.isArray(node.parentIds) ? node.parentIds : [node.parentId],
            );
            node.parentIds = parentIds;
            node.parentId = parentIds[0] || null;
            if (!parentIds.length) {
                node.parentHead = null;
            } else {
                const firstParent = nodesById.get(parentIds[0]) || null;
                node.parentHead = firstParent ? firstParent.head || null : node.parentHead || null;
            }
            for (const parentId of parentIds) {
                const parent = nodesById.get(parentId) || null;
                if (!parent || parent.id === node.id) {
                    continue;
                }
                links.push({ source: node, target: parent });
                mergeChild(parent, node);
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
            selectedRemovalParentId: state.selectedRemovalParentId,
            pendingParentMutation: state.pendingParentMutation,
            nodes: state.nodes.map((node) => ({
                id: node.id,
                parentIds: Array.isArray(node.parentIds) ? node.parentIds.slice() : [],
                parentId: node.parentId,
                parentHead: node.parentHead,
                parentHeads: Array.isArray(node.parentHeads) ? node.parentHeads.slice() : [],
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
            node.parentIds = Array.isArray(saved.parentIds) ? saved.parentIds.slice() : [];
            node.parentId = saved.parentId;
            node.parentHead = saved.parentHead;
            node.parentHeads = Array.isArray(saved.parentHeads) ? saved.parentHeads.slice() : [];
        }
        state.selectedParentId = snapshot?.selectedParentId ?? null;
        state.selectedRemovalParentId = snapshot?.selectedRemovalParentId ?? null;
        state.pendingParentMutation = Boolean(snapshot?.pendingParentMutation);
        rebuildCurrentRelations();
    }

    function applyOptimisticParentChange(recordId, parentId) {
        const node = state.nodes.find((item) => item.id === recordId) || null;
        if (!node) {
            return false;
        }
        const parent = parentId ? state.nodes.find((item) => item.id === parentId) || null : null;
        const parentIds = uniqueIntegers([
            ...(Array.isArray(node.parentIds) ? node.parentIds : []),
            parent ? parent.id : null,
        ]);
        node.parentIds = parentIds;
        node.parentId = parentIds[0] || null;
        node.parentHead = parentIds.length ? parent?.head || node.parentHead || null : null;
        rebuildCurrentRelations();
        return true;
    }

    function applyOptimisticParentRemoval(recordId, parentId) {
        const node = state.nodes.find((item) => item.id === recordId) || null;
        if (!node) {
            return false;
        }
        if (parentId) {
            node.parentIds = uniqueIntegers(
                (Array.isArray(node.parentIds) ? node.parentIds : []).filter((value) => value !== parentId),
            );
        } else {
            node.parentIds = [];
        }
        node.parentId = node.parentIds[0] || null;
        node.parentHead = node.parentIds.length ? node.parentHead || null : null;
        state.selectedRemovalParentId = null;
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
                existing.parentIds = rawNode.parentIds;
                existing.parentId = rawNode.parentId;
                existing.parentHead = rawNode.parentHead;
                existing.parentHeads = rawNode.parentHeads;
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
            state.selectedRemovalParentId = null;
            state.parentSearchQuery = "";
        }
    }

    function updateOriginFromSnapshot(snapshot) {
        const view = asObject(snapshot?.view);
        const viewId = snapshot?.view_id ?? snapshot?.viewId ?? view?.view_id ?? view?.viewId ?? state.origin.viewId;
        const name = String(
            snapshot?.viewName ||
                snapshot?.view_name ||
                view?.name ||
                state.origin.viewName ||
                snapshot?.name ||
                "Relation",
        ).trim() || "Relation";
        const query = String(snapshot?.query || view?.query || "").trim();
        state.origin.viewName = name;
        originText.textContent = [
            `serverId: ${state.origin.serverId || "local"}`,
            `viewId: ${viewId == null ? "none" : viewId}`,
            `view: ${name}`,
        ].join("\n");
        viewSqlText.textContent = query || "(none)";
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

    function nodeHitRadius(node) {
        return nodeRadius(node) + 8 / state.viewport.scale;
    }

    function isPointInsideNode(node, worldPoint) {
        if (!node || !worldPoint) {
            return false;
        }
        const dx = node.x - worldPoint.x;
        const dy = node.y - worldPoint.y;
        return Math.hypot(dx, dy) <= nodeHitRadius(node);
    }

    function findNodeAtPoint(screenX, screenY) {
        const point = screenToWorld(screenX, screenY);
        if (state.simulation && typeof state.simulation.find === "function") {
            const searchRadius = Math.max(
                ...state.nodes.map((node) => nodeHitRadius(node)),
                24 / state.viewport.scale,
            );
            const candidate = state.simulation.find(point.x, point.y, searchRadius) || null;
            if (candidate && isPointInsideNode(candidate, point)) {
                return candidate;
            }
        }
        for (const node of state.nodes) {
            if (isPointInsideNode(node, point)) {
                return node;
            }
        }
        return null;
    }

    function selectNode(node) {
        if (!node) {
            state.selectedId = null;
            state.selectedParentId = null;
            state.selectedRemovalParentId = null;
            state.parentSearchQuery = "";
            state.recordPanelOpen = false;
            renderSelection();
            requestDraw();
            return;
        }

        state.selectedId = node.id;
        state.selectedParentId = null;
        state.selectedRemovalParentId = null;
        state.parentSearchQuery = "";
        state.recordPanelOpen = true;
        renderPanel();
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
                state.selectedParentId = null;
                state.selectedRemovalParentId = null;
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
        const removalParentId = Number(state.selectedRemovalParentId || 0) || null;
        const relationSnapshot = captureParentState();
        state.pendingParentMutation = true;
        applyOptimisticParentRemoval(node.id, removalParentId);
        renderStatus(
            "Saving",
            "status",
            removalParentId ? "Parent relation removal pending." : "Removing all parent relations.",
        );
        postAction("set-parent", {
            recordId: node.id,
            parentId: null,
            removeParentId: removalParentId,
        })
            .then(() => {
                state.pendingParentMutation = false;
                state.selectedParentId = null;
                renderSelection();
                renderStatus(
                    "Saved",
                    "live",
                    removalParentId ? "Parent relation removed." : "All parent relations removed.",
                );
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
            const world = screenToWorld(screenX, screenY);
            state.pointer = {
                kind: "node",
                pointerId: event.pointerId,
                node,
                startX: screenX,
                startY: screenY,
                grabOffsetX: world.x - node.x,
                grabOffsetY: world.y - node.y,
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
            const nextX = world.x - (pointer.grabOffsetX || 0);
            const nextY = world.y - (pointer.grabOffsetY || 0);
            pointer.node.x = nextX;
            pointer.node.y = nextY;
            pointer.node.fx = nextX;
            pointer.node.fy = nextY;
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
        createHeadInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter") {
                event.preventDefault();
                submitCreateRecord();
            }
        });
        createBodyInput.addEventListener("input", () => {
            state.createDraft.body = String(createBodyInput.value || "");
        });
        createQuantityInput.addEventListener("input", () => {
            state.createDraft.quantity = String(createQuantityInput.value || "0");
        });
        createParentSearchInput.addEventListener("input", () => {
            state.createParentSearchQuery = String(createParentSearchInput.value || "").trim();
            renderCreateForm();
        });
        createParentChoiceList.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-create-parent-choice]");
            if (!button) {
                return;
            }
            state.createDraft.parentId = Number(button.getAttribute("data-create-parent-choice") || 0) || null;
            renderCreateForm();
        });
        createParentClearButton.addEventListener("click", () => {
            state.createDraft.parentId = null;
            renderCreateForm();
        });
        createClearButton.addEventListener("click", () => {
            resetCreateDraft();
            renderCreateForm();
        });
        createSubmitButton.addEventListener("click", submitCreateRecord);
        createBodyInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                event.preventDefault();
                submitCreateRecord();
            }
        });
        recordHeadInput.addEventListener("input", () => {
            state.recordDraft.head = String(recordHeadInput.value || "");
            state.recordDraftDirty = true;
            state.recordDeleteArmed = false;
            renderRecordEditor(state.nodes.find((item) => item.id === state.selectedId) || null);
        });
        recordHeadInput.addEventListener("blur", saveSelectedRecord);
        recordBodyInput.addEventListener("input", () => {
            state.recordDraft.body = String(recordBodyInput.value || "");
            state.recordDraftDirty = true;
            state.recordDeleteArmed = false;
        });
        recordBodyInput.addEventListener("blur", saveSelectedRecord);
        recordQuantityInput.addEventListener("input", () => {
            state.recordDraft.quantity = String(recordQuantityInput.value || "0");
            state.recordDraftDirty = true;
            state.recordDeleteArmed = false;
        });
        recordQuantityInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter") {
                event.preventDefault();
                saveSelectedRecord();
            }
        });
        recordQuantityInput.addEventListener("blur", saveSelectedRecord);
        recordCategoryInput.addEventListener("input", () => {
            state.recordCategoryInput = String(recordCategoryInput.value || "");
            state.recordDeleteArmed = false;
        });
        recordCategoryInput.addEventListener("keydown", (event) => {
            if (event.key === "Enter" || event.key === ",") {
                event.preventDefault();
                if (commitRecordCategoryInput()) {
                    saveSelectedRecord();
                }
            }
        });
        recordCategoryInput.addEventListener("blur", () => {
            if (commitRecordCategoryInput()) {
                saveSelectedRecord();
            }
        });
        recordCategoryList.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-record-category-remove]");
            if (!button) {
                return;
            }
            removeRecordCategory(button.getAttribute("data-record-category-remove"));
        });
        recordSaveButton.addEventListener("click", saveSelectedRecord);
        recordDeleteButton.addEventListener("click", deleteSelectedRecord);
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
            state.selectedRemovalParentId = null;
            connectParent();
        });
        currentParentList?.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-parent-remove]");
            if (!button) {
                return;
            }
            state.selectedRemovalParentId = Number(button.getAttribute("data-parent-remove") || 0) || null;
            state.selectedParentId = null;
            disconnectParent();
        });
        applyFiltersButton.addEventListener("click", applyFilters);
        clearFiltersButton.addEventListener("click", clearFilters);
        resetPhysicsButton.addEventListener("click", resetPhysics);
        controlsResizer?.addEventListener("pointerdown", (event) => {
            startPanelResize("controls", event);
        });
        recordResizer?.addEventListener("pointerdown", (event) => {
            startPanelResize("record", event);
        });
        window.addEventListener("pointermove", updatePanelResize);
        window.addEventListener("pointerup", endPanelResize);
        window.addEventListener("pointercancel", endPanelResize);
        panelToggleButton.addEventListener("click", () => {
            state.controlsPanelOpen = !state.controlsPanelOpen;
            renderPanel();
        });
        panelCloseButton.addEventListener("click", () => {
            state.controlsPanelOpen = false;
            renderPanel();
        });
        recordCloseButton.addEventListener("click", () => {
            state.recordPanelOpen = false;
            renderPanel();
        });
        createCloseButton.addEventListener("click", () => {
            state.createPanelOpen = false;
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
                persistPhysicsSoon();
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
        window.addEventListener("resize", () => {
            applyPanelWidths();
            resizeCanvas();
        });
    }

    function start() {
        renderMode();
        renderPanel();
        renderStatus("Booting", "status", "Waiting for the widget stream.");
        bindEvents();
        syncFromBridge();
        bridge?.subscribe?.((detail) => syncFromBridge(detail));
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
