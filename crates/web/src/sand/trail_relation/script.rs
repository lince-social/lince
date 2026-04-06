pub(crate) fn script() -> String {
    r####"
    (() => {
        const d3 = window.d3;

        const DEFAULT_PHYSICS = {
            charge: -200,
            distance: 110,
            collision: 24,
        };

        const state = {
            binding: null,
            snapshot: null,
            stream: null,
            discoverResults: [],
            selectedOriginal: null,
            selectedNodeId: null,
            discoverRequestSeq: 0,
            discoverRefreshTimer: null,
            assigneeRequestSeq: {
                discover: 0,
                create: 0,
            },
            assigneeSuggestions: {
                discover: [],
                create: [],
            },
            physics: { ...DEFAULT_PHYSICS },
            graph: {
                svg: null,
                viewport: null,
                linkLayer: null,
                nodeLayer: null,
                labelLayer: null,
                zoom: null,
                zoomScale: 1,
                simulation: null,
                resizeObserver: null,
                width: 0,
                height: 0,
                nodes: [],
                links: [],
                needsFit: true,
            },
        };

        const elements = {
            statusPill: document.getElementById("trail-status-pill"),
            boundPill: document.getElementById("trail-bound-pill"),
            rowPill: document.getElementById("trail-row-pill"),
            linkPill: document.getElementById("trail-link-pill"),
            syncPill: document.getElementById("trail-sync-pill"),
            zoomPill: document.getElementById("trail-zoom-pill"),
            graphStage: document.getElementById("trail-graph-stage"),
            graph: document.getElementById("trail-graph"),
            graphEmpty: document.getElementById("trail-empty"),
            zoomOutButton: document.getElementById("trail-zoom-out"),
            zoomInButton: document.getElementById("trail-zoom-in"),
            fitButton: document.getElementById("trail-fit"),
            centerButton: document.getElementById("trail-center"),
            searchHead: document.getElementById("trail-search-head"),
            searchCategory: document.getElementById("trail-search-category"),
            searchAssignee: document.getElementById("trail-search-assignee"),
            searchAssigneeSuggestions: document.getElementById("trail-search-assignee-suggestions"),
            discoverSummary: document.getElementById("trail-discover-summary"),
            discoverResults: document.getElementById("trail-discover-results"),
            selectedOriginalTitle: document.getElementById("trail-selected-original"),
            selectedOriginalCopy: document.getElementById("trail-selected-original-copy"),
            createAssignee: document.getElementById("trail-create-assignee"),
            createAssigneeSuggestions: document.getElementById("trail-create-assignee-suggestions"),
            syncScope: document.getElementById("trail-sync-scope"),
            syncScopeCopy: document.getElementById("trail-sync-scope-copy"),
            syncFieldQ: document.getElementById("trail-sync-field-q"),
            syncFieldH: document.getElementById("trail-sync-field-h"),
            syncFieldB: document.getElementById("trail-sync-field-b"),
            createSubmit: document.getElementById("trail-create-submit"),
            bindingTitle: document.getElementById("trail-binding-title"),
            bindingCopy: document.getElementById("trail-binding-copy"),
            overwriteCopy: document.getElementById("trail-overwrite-copy"),
            initializeTrail: document.getElementById("trail-initialize"),
            runSync: document.getElementById("trail-run-sync"),
            bindSelected: document.getElementById("trail-bind-selected"),
            physicsCharge: document.getElementById("trail-physics-charge"),
            physicsDistance: document.getElementById("trail-physics-distance"),
            physicsCollision: document.getElementById("trail-physics-collision"),
            selectionTitle: document.getElementById("trail-selection-title"),
            selectionCopy: document.getElementById("trail-selection-copy"),
            setLocked: document.getElementById("trail-set-locked"),
            setReady: document.getElementById("trail-set-ready"),
            setDone: document.getElementById("trail-set-done"),
        };

        function instanceId() {
            return window.frameElement?.dataset?.packageInstanceId || "preview";
        }

        function contractUrl() {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/contract";
        }

        function streamUrl() {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/stream";
        }

        function actionUrl(action) {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/actions/" + encodeURIComponent(action);
        }

        function boundTrailStorageKey() {
            return "lince.widget.trail_relation." + instanceId() + ".boundTrailRoot";
        }

        function readStoredTrailRoot() {
            try {
                const value = Number(window.localStorage?.getItem?.(boundTrailStorageKey()) || 0);
                return Number.isFinite(value) && value > 0 ? value : null;
            } catch (_error) {
                return null;
            }
        }

        function writeStoredTrailRoot(recordId) {
            try {
                if (Number(recordId) > 0) {
                    window.localStorage?.setItem?.(boundTrailStorageKey(), String(recordId));
                }
            } catch (_error) {
            }
        }

        function setStatus(text) {
            elements.statusPill.textContent = text;
        }

        function escapeHtml(value) {
            return String(value ?? "")
                .replaceAll("&", "&amp;")
                .replaceAll("<", "&lt;")
                .replaceAll(">", "&gt;")
                .replaceAll("\"", "&quot;")
                .replaceAll("'", "&#39;");
        }

        function parseJsonArray(value) {
            try {
                const parsed = JSON.parse(value || "[]");
                return Array.isArray(parsed) ? parsed : [];
            } catch (_error) {
                return [];
            }
        }

        function valueOf(object, ...keys) {
            for (const key of keys) {
                if (object && object[key] != null) {
                    return object[key];
                }
            }
            return null;
        }

        function normalizeText(value) {
            return String(value ?? "").trim().toLowerCase();
        }

        function nodeIdFromRow(row) {
            return Number(valueOf(row, "id") || 0);
        }

        function rowHead(row) {
            return String(valueOf(row, "head") || "");
        }

        function rowBody(row) {
            return String(valueOf(row, "body") || "");
        }

        function rowPrimaryCategory(row) {
            return String(valueOf(row, "primaryCategory", "primary_category") || "").trim();
        }

        function rowCategories(row) {
            const categories = parseJsonArray(valueOf(row, "categoriesJson", "categories_json"));
            if (categories.length) {
                return categories;
            }
            const primary = rowPrimaryCategory(row);
            return primary ? [primary] : [];
        }

        function rowAssigneeNames(row) {
            return parseJsonArray(valueOf(row, "assigneeNamesJson", "assignee_names_json"));
        }

        function rowAssigneeUsernames(row) {
            return parseJsonArray(valueOf(row, "assigneeUsernamesJson", "assignee_usernames_json"));
        }

        function rowParentIds(row) {
            return parseJsonArray(valueOf(row, "parentIdsJson", "parent_ids_json")).map((value) => Number(value)).filter(Number.isFinite);
        }

        function selectedNode() {
            return state.graph.nodes.find((node) => node.id === Number(state.selectedNodeId)) || null;
        }

        function normalizeFields(fields) {
            const text = String(fields || "");
            let normalized = "";
            if (text.includes("q")) normalized += "q";
            if (text.includes("h")) normalized += "h";
            if (text.includes("b")) normalized += "b";
            return normalized || "hb";
        }

        function currentFields() {
            let fields = "";
            if (elements.syncFieldQ.checked) fields += "q";
            if (elements.syncFieldH.checked) fields += "h";
            if (elements.syncFieldB.checked) fields += "b";
            return fields || "hb";
        }

        function quantityLabel(quantity) {
            if (quantity === 1) return "Done";
            if (quantity === -1) return "Ready";
            return "Locked";
        }

        function scopeCopy(scope) {
            if (scope === "n") return "Node syncs a single record.";
            if (scope === "nt") return "Both syncs the record and its children.";
            return "Tree syncs children only.";
        }

        function renderZoomPill(scale) {
            const resolved = Number.isFinite(scale) ? scale : state.graph.zoomScale || 1;
            state.graph.zoomScale = resolved;
            elements.zoomPill.textContent = Math.round(resolved * 100) + "%";
        }

        function truncateLabel(value, limit = 18) {
            const text = String(value || "").trim();
            if (text.length <= limit) {
                return text || "(untitled)";
            }
            return text.slice(0, limit - 1).trimEnd() + "…";
        }

        function nodeRadius(node) {
            return 18 + Math.min(Number(node.childrenCount || 0), 6);
        }

        function nodeFill(quantity) {
            if (quantity === 1) return "#7ef0c6";
            if (quantity === -1) return "#f2bb78";
            return "#64748b";
        }

        function postJson(action, payload) {
            return fetch(actionUrl(action), {
                method: "POST",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload || {}),
            }).then(async (response) => {
                const data = await response.json().catch(() => ({}));
                if (!response.ok) {
                    throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
                }
                return data;
            });
        }

        function renderSelectedOriginal() {
            if (!state.selectedOriginal) {
                elements.selectedOriginalTitle.textContent = "No original selected";
                elements.selectedOriginalCopy.textContent = "Choose an original record from Discover.";
                elements.createSubmit.disabled = true;
                return;
            }

            const categories = rowCategories(state.selectedOriginal);
            elements.selectedOriginalTitle.textContent =
                "#" + nodeIdFromRow(state.selectedOriginal) + " " + (rowHead(state.selectedOriginal) || "(untitled)");
            elements.selectedOriginalCopy.textContent = categories.length
                ? "Categories: " + categories.join(", ")
                : "No categories on the selected original.";
            elements.createSubmit.disabled = !elements.createAssignee.value.trim();
        }

        function renderBinding() {
            if (!state.binding?.trailRootRecordId) {
                elements.boundPill.textContent = "No trail bound";
                elements.bindingTitle.textContent = "No copied trail root bound";
                elements.bindingCopy.textContent = "Use Open trail in Discover, or select a graph node and bind it as the trail root.";
                elements.syncPill.textContent = "No sync";
                elements.overwriteCopy.textContent = "";
                elements.initializeTrail.disabled = true;
                elements.runSync.disabled = true;
                return;
            }

            writeStoredTrailRoot(state.binding.trailRootRecordId);
            elements.boundPill.textContent = "Trail root #" + state.binding.trailRootRecordId;
            elements.bindingTitle.textContent =
                "Bound copied trail root #" + state.binding.trailRootRecordId;
            elements.bindingCopy.textContent =
                "Streaming view " + (state.binding.viewId ?? "?") + ". Open trail on a copied root rebinds the SSE view to that record tree.";
            elements.initializeTrail.disabled = false;
            elements.runSync.disabled = false;

            if (state.binding.sync) {
                const fields = normalizeFields(state.binding.sync.fields || "hb");
                const overwritten = [];
                const preserved = [];
                if (fields.includes("q")) overwritten.push("quantity"); else preserved.push("quantity");
                if (fields.includes("h")) overwritten.push("head"); else preserved.push("head");
                if (fields.includes("b")) overwritten.push("body"); else preserved.push("body");
                elements.syncPill.textContent =
                    "Sync from #" + state.binding.sync.syncSourceRecordId +
                    " · " + (state.binding.sync.scope || "t") +
                    " · " + fields;
                elements.overwriteCopy.textContent =
                    "Overwrite: " + overwritten.join(", ") + ". Preserve: " + preserved.join(", ") + ".";
                elements.syncScope.value = state.binding.sync.scope || "t";
                applyFields(fields);
            } else {
                elements.syncPill.textContent = "No sync";
                elements.overwriteCopy.textContent = "";
            }
        }

        function renderSyncInputs() {
            updateScopeCopy();
            if (!state.binding?.sync?.syncSourceRecordId) {
                return;
            }
            const fields = currentFields();
            const overwritten = [];
            const preserved = [];
            if (fields.includes("q")) overwritten.push("quantity"); else preserved.push("quantity");
            if (fields.includes("h")) overwritten.push("head"); else preserved.push("head");
            if (fields.includes("b")) overwritten.push("body"); else preserved.push("body");
            elements.overwriteCopy.textContent =
                "Sync source #" + state.binding.sync.syncSourceRecordId +
                " would overwrite " + overwritten.join(", ") +
                " and preserve " + preserved.join(", ") + ".";
        }

        function renderSelection() {
            const node = selectedNode();
            const canAct = Boolean(node && state.binding?.trailRootRecordId);
            elements.bindSelected.disabled = !node;
            elements.setLocked.disabled = !canAct;
            elements.setReady.disabled = !canAct;
            elements.setDone.disabled = !canAct;

            if (!node) {
                elements.selectionTitle.textContent = "No node selected";
                elements.selectionCopy.textContent = "Click a node in the graph to inspect it and update its trail quantity.";
                return;
            }

            const categoryCopy = node.categories.length
                ? "Categories: " + node.categories.join(", ")
                : "No categories";
            const assigneeCopy = node.assigneeNames.length
                ? " · Assignees: " + node.assigneeNames.join(", ")
                : "";
            elements.selectionTitle.textContent =
                "#" + node.id + " " + (node.head || "(untitled)") + " · " + quantityLabel(node.quantity);
            elements.selectionCopy.textContent = categoryCopy + assigneeCopy;
        }

        function renderDiscoverResults() {
            if (!state.discoverResults.length) {
                elements.discoverResults.innerHTML =
                    "<p class=\"copy\">No records match the current filters.</p>";
                return;
            }

            elements.discoverResults.innerHTML = state.discoverResults.map((row) => {
                const id = nodeIdFromRow(row);
                const categories = rowCategories(row);
                const assigneeNames = rowAssigneeNames(row);
                const assigneeUsernames = rowAssigneeUsernames(row);
                const excerpt = rowBody(row).trim();
                const isSelected = state.selectedOriginal && nodeIdFromRow(state.selectedOriginal) === id;
                return `
                    <article class="resultCard${isSelected ? " isSelected" : ""}" data-original-id="${id}">
                        <div class="sectionHead">
                            <div>
                                <div class="selectionTitle">#${id} ${escapeHtml(rowHead(row) || "(untitled)")}</div>
                            </div>
                            <div class="actionRow">
                                <button class="button buttonGhost" type="button" data-open-root="${id}">Open trail</button>
                                <button class="button buttonPrimary" type="button" data-use-original="${id}">Use as original</button>
                            </div>
                        </div>
                        ${excerpt ? `<p class="resultExcerpt">${escapeHtml(excerpt)}</p>` : ""}
                        <div class="resultMeta">
                            ${categories.map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                            ${assigneeNames.map((value, index) => {
                                const username = assigneeUsernames[index];
                                const label = username ? `${value} (@${username})` : value;
                                return `<span class="pill">${escapeHtml(label)}</span>`;
                            }).join(" ")}
                        </div>
                    </article>
                `;
            }).join("");
        }

        function renderDiscoverSummary() {
            elements.discoverSummary.textContent =
                "Showing " + state.discoverResults.length + " matching records. Open trail binds the selected copied root as the live SSE source.";
        }

        function suggestionElements(kind) {
            if (kind === "discover") {
                return {
                    input: elements.searchAssignee,
                    panel: elements.searchAssigneeSuggestions,
                };
            }
            return {
                input: elements.createAssignee,
                panel: elements.createAssigneeSuggestions,
            };
        }

        function hideAssigneeSuggestions(kind) {
            const { panel } = suggestionElements(kind);
            panel.hidden = true;
            panel.innerHTML = "";
            state.assigneeSuggestions[kind] = [];
        }

        function renderAssigneeSuggestions(kind) {
            const rows = state.assigneeSuggestions[kind] || [];
            const { panel } = suggestionElements(kind);
            if (!rows.length) {
                hideAssigneeSuggestions(kind);
                return;
            }
            panel.innerHTML = rows.map((row) => `
                <button
                    class="suggestionButton"
                    type="button"
                    data-assignee-kind="${kind}"
                    data-assignee-value="${escapeHtml(row.username || String(row.id))}"
                >
                    <strong>#${escapeHtml(row.id)} ${escapeHtml(row.name)}</strong>
                    <span class="suggestionMeta">@${escapeHtml(row.username)}</span>
                </button>
            `).join("");
            panel.hidden = false;
        }

        function requestAssigneeSuggestions(kind) {
            const { input } = suggestionElements(kind);
            const query = input.value.trim();
            if (!query) {
                hideAssigneeSuggestions(kind);
                return;
            }
            const requestSeq = ++state.assigneeRequestSeq[kind];
            postJson("search-assignees", { query })
                .then((result) => {
                    if (requestSeq !== state.assigneeRequestSeq[kind]) {
                        return;
                    }
                    state.assigneeSuggestions[kind] = Array.isArray(result.results) ? result.results : [];
                    renderAssigneeSuggestions(kind);
                })
                .catch((error) => {
                    console.error(error);
                });
        }

        function applyAssigneeSuggestion(kind, value) {
            const { input } = suggestionElements(kind);
            input.value = value;
            hideAssigneeSuggestions(kind);
            if (kind === "discover") {
                refreshDiscoverResults().catch((error) => setStatus(error.message));
            } else {
                renderSelectedOriginal();
            }
        }

        function scheduleDiscoverRefresh(delayMs) {
            clearTimeout(state.discoverRefreshTimer);
            state.discoverRefreshTimer = window.setTimeout(() => {
                refreshDiscoverResults().catch((error) => setStatus(error.message));
            }, delayMs);
        }

        function discoverPayload() {
            return {
                headContains: elements.searchHead.value.trim() || null,
                category: elements.searchCategory.value.trim() || null,
                assignee: elements.searchAssignee.value.trim() || null,
            };
        }

        function refreshDiscoverResults() {
            const requestSeq = ++state.discoverRequestSeq;
            setStatus("Searching");
            return postJson("search-trails", discoverPayload()).then((result) => {
                if (requestSeq !== state.discoverRequestSeq) {
                    return;
                }
                state.discoverResults = Array.isArray(result.results) ? result.results : [];
                if (state.selectedOriginal) {
                    const replacement = state.discoverResults.find((row) => nodeIdFromRow(row) === nodeIdFromRow(state.selectedOriginal));
                    if (replacement) {
                        state.selectedOriginal = replacement;
                    }
                }
                renderSelectedOriginal();
                renderDiscoverSummary();
                renderDiscoverResults();
                setStatus(state.binding?.trailRootRecordId ? "Live" : "Ready");
            });
        }

        function initializeGraph() {
            if (!d3) {
                setStatus("Missing d3");
                return;
            }

            const svg = d3.select(elements.graph);
            svg.selectAll("*").remove();

            const defs = svg.append("defs");
            defs.append("marker")
                .attr("id", "trail-arrow")
                .attr("viewBox", "0 -5 10 10")
                .attr("refX", 20)
                .attr("refY", 0)
                .attr("markerWidth", 7)
                .attr("markerHeight", 7)
                .attr("orient", "auto")
                .append("path")
                .attr("fill", "rgba(120, 215, 255, 0.78)")
                .attr("d", "M0,-5L10,0L0,5");

            const viewport = svg.append("g").attr("class", "viewport");
            const linkLayer = viewport.append("g");
            const nodeLayer = viewport.append("g");
            const labelLayer = viewport.append("g");

            const zoom = d3.zoom()
                .scaleExtent([0.25, 3.5])
                .on("zoom", (event) => {
                    viewport.attr("transform", event.transform);
                    renderZoomPill(event.transform.k);
                });

            svg.call(zoom).on("dblclick.zoom", null);
            svg.on("click", () => {
                state.selectedNodeId = null;
                renderSelection();
                updateGraphSelection();
            });

            state.graph.svg = svg;
            state.graph.viewport = viewport;
            state.graph.linkLayer = linkLayer;
            state.graph.nodeLayer = nodeLayer;
            state.graph.labelLayer = labelLayer;
            state.graph.zoom = zoom;
            renderZoomPill(1);

            state.graph.resizeObserver = new ResizeObserver(() => {
                resizeGraph();
            });
            state.graph.resizeObserver.observe(elements.graphStage);
            resizeGraph();
        }

        function resizeGraph() {
            if (!state.graph.svg) {
                return;
            }
            const width = Math.max(elements.graphStage.clientWidth, 240);
            const height = Math.max(elements.graphStage.clientHeight, 240);
            state.graph.width = width;
            state.graph.height = height;
            state.graph.svg.attr("viewBox", `0 0 ${width} ${height}`);
            if (state.graph.simulation) {
                state.graph.simulation
                    .force("x", d3.forceX(width / 2).strength(0.08))
                    .force("y", d3.forceY(height / 2).strength(0.08))
                    .alpha(0.25)
                    .restart();
            }
        }

        function buildGraphData(rows) {
            const previous = new Map(state.graph.nodes.map((node) => [node.id, node]));
            const nodes = rows.map((row) => {
                const id = nodeIdFromRow(row);
                const previousNode = previous.get(id);
                return {
                    id,
                    head: rowHead(row),
                    body: rowBody(row),
                    quantity: Number(valueOf(row, "quantity") || 0),
                    categories: rowCategories(row),
                    assigneeNames: rowAssigneeNames(row),
                    assigneeUsernames: rowAssigneeUsernames(row),
                    childrenCount: Number(valueOf(row, "childrenCount", "children_count") || 0),
                    depth: Number(valueOf(row, "depth") || 0),
                    x: previousNode?.x ?? (state.graph.width / 2) + ((Math.random() - 0.5) * 40),
                    y: previousNode?.y ?? (state.graph.height / 2) + ((Math.random() - 0.5) * 40),
                    vx: previousNode?.vx ?? 0,
                    vy: previousNode?.vy ?? 0,
                };
            });

            const nodeIds = new Set(nodes.map((node) => node.id));
            const seenLinks = new Set();
            const links = [];

            rows.forEach((row) => {
                const childId = nodeIdFromRow(row);
                rowParentIds(row).forEach((parentId) => {
                    if (!nodeIds.has(parentId)) {
                        return;
                    }
                    const key = parentId + "->" + childId;
                    if (seenLinks.has(key)) {
                        return;
                    }
                    seenLinks.add(key);
                    links.push({
                        id: key,
                        source: parentId,
                        target: childId,
                    });
                });
            });

            return { nodes, links };
        }

        function dragBehaviour(simulation) {
            return d3.drag()
                .on("start", (event, node) => {
                    event.sourceEvent.stopPropagation();
                    if (!event.active) {
                        simulation.alphaTarget(0.25).restart();
                    }
                    node.fx = node.x;
                    node.fy = node.y;
                })
                .on("drag", (event, node) => {
                    node.fx = event.x;
                    node.fy = event.y;
                })
                .on("end", (event, node) => {
                    if (!event.active) {
                        simulation.alphaTarget(0);
                    }
                    node.fx = null;
                    node.fy = null;
                });
        }

        function updateGraphSelection() {
            if (!state.graph.nodeLayer) {
                return;
            }
            state.graph.nodeLayer.selectAll("circle")
                .classed("is-selected", (node) => node.id === Number(state.selectedNodeId));
        }

        function fitGraph() {
            if (!state.graph.svg || !state.graph.nodes.length) {
                return;
            }

            const xs = state.graph.nodes.map((node) => node.x).filter(Number.isFinite);
            const ys = state.graph.nodes.map((node) => node.y).filter(Number.isFinite);
            if (!xs.length || !ys.length) {
                return;
            }

            const minX = Math.min(...xs);
            const maxX = Math.max(...xs);
            const minY = Math.min(...ys);
            const maxY = Math.max(...ys);
            const width = Math.max(maxX - minX, 80);
            const height = Math.max(maxY - minY, 80);
            const scale = Math.max(0.35, Math.min(2.2, 0.86 / Math.max(width / state.graph.width, height / state.graph.height)));
            const tx = (state.graph.width - (minX + maxX) * scale) / 2;
            const ty = (state.graph.height - (minY + maxY) * scale) / 2;
            const transform = d3.zoomIdentity.translate(tx, ty).scale(scale);
            state.graph.svg.transition().duration(220).call(state.graph.zoom.transform, transform);
            state.graph.needsFit = false;
        }

        function centerGraph() {
            if (!state.graph.svg || !state.graph.nodes.length) {
                return;
            }
            const avgX = state.graph.nodes.reduce((sum, node) => sum + node.x, 0) / state.graph.nodes.length;
            const avgY = state.graph.nodes.reduce((sum, node) => sum + node.y, 0) / state.graph.nodes.length;
            const transform = d3.zoomIdentity
                .translate((state.graph.width / 2) - avgX, (state.graph.height / 2) - avgY)
                .scale(1);
            state.graph.svg.transition().duration(220).call(state.graph.zoom.transform, transform);
        }

        function zoomBy(multiplier) {
            if (!state.graph.svg || !state.graph.zoom) {
                return;
            }
            const current = d3.zoomTransform(elements.graph);
            const nextScale = Math.max(0.25, Math.min(3.5, current.k * multiplier));
            const centerX = state.graph.width / 2;
            const centerY = state.graph.height / 2;
            const transform = d3.zoomIdentity
                .translate(centerX - ((centerX - current.x) / current.k) * nextScale, centerY - ((centerY - current.y) / current.k) * nextScale)
                .scale(nextScale);
            state.graph.svg.transition().duration(180).call(state.graph.zoom.transform, transform);
        }

        function trimmedLinkCoordinates(link) {
            const source = link.source;
            const target = link.target;
            const dx = target.x - source.x;
            const dy = target.y - source.y;
            const distance = Math.hypot(dx, dy) || 1;
            const ux = dx / distance;
            const uy = dy / distance;
            const sourceRadius = nodeRadius(source) + 2;
            const targetRadius = nodeRadius(target) + 8;
            return {
                x1: source.x + ux * sourceRadius,
                y1: source.y + uy * sourceRadius,
                x2: target.x - ux * targetRadius,
                y2: target.y - uy * targetRadius,
            };
        }

        function applyPhysics() {
            if (!state.graph.simulation) {
                return;
            }
            state.graph.simulation
                .force("charge", d3.forceManyBody().strength(state.physics.charge))
                .force("link", d3.forceLink(state.graph.links).id((linkNode) => linkNode.id).distance(state.physics.distance).strength(0.9))
                .force("collision", d3.forceCollide().radius((node) => nodeRadius(node) + state.physics.collision))
                .force("x", d3.forceX(state.graph.width / 2).strength(0.08))
                .force("y", d3.forceY(state.graph.height / 2).strength(0.08))
                .alpha(0.7)
                .restart();
        }

        function renderGraph() {
            if (!state.graph.svg) {
                return;
            }
            const rows = Array.isArray(state.snapshot?.rows) ? state.snapshot.rows : [];
            const { nodes, links } = buildGraphData(rows);

            state.graph.nodes = nodes;
            state.graph.links = links;

            elements.rowPill.textContent = nodes.length + " nodes";
            elements.linkPill.textContent = links.length + " links";
            elements.graphEmpty.hidden = nodes.length > 0;

            if (!nodes.length) {
                state.selectedNodeId = null;
                renderSelection();
                state.graph.linkLayer.selectAll("*").remove();
                state.graph.nodeLayer.selectAll("*").remove();
                state.graph.labelLayer.selectAll("*").remove();
                if (state.graph.simulation) {
                    state.graph.simulation.stop();
                }
                return;
            }

            if (state.selectedNodeId && !nodes.some((node) => node.id === Number(state.selectedNodeId))) {
                state.selectedNodeId = null;
            }

            const simulation = d3.forceSimulation(nodes)
                .force("link", d3.forceLink(links).id((node) => node.id).distance(state.physics.distance).strength(0.9))
                .force("charge", d3.forceManyBody().strength(state.physics.charge))
                .force("collision", d3.forceCollide().radius((node) => nodeRadius(node) + state.physics.collision))
                .force("x", d3.forceX(state.graph.width / 2).strength(0.08))
                .force("y", d3.forceY(state.graph.height / 2).strength(0.08));

            const linkSelection = state.graph.linkLayer
                .selectAll("line")
                .data(links, (link) => link.id)
                .join("line")
                .attr("class", "node-link")
                .attr("marker-end", "url(#trail-arrow)");

            const nodeSelection = state.graph.nodeLayer
                .selectAll("g")
                .data(nodes, (node) => node.id)
                .join((enter) => {
                    const group = enter.append("g");
                    group.append("circle").attr("class", "node-circle");
                    group.on("click", (event, node) => {
                        event.stopPropagation();
                        state.selectedNodeId = node.id;
                        renderSelection();
                        updateGraphSelection();
                    });
                    return group;
                })
                .call(dragBehaviour(simulation));

            nodeSelection.select("circle")
                .attr("r", (node) => nodeRadius(node))
                .attr("fill", (node) => nodeFill(node.quantity));

            const labelSelection = state.graph.labelLayer
                .selectAll("text")
                .data(nodes, (node) => node.id)
                .join("text")
                .attr("class", "node-label")
                .text((node) => truncateLabel(node.head));

            simulation.on("tick", () => {
                linkSelection
                    .attr("x1", (link) => trimmedLinkCoordinates(link).x1)
                    .attr("y1", (link) => trimmedLinkCoordinates(link).y1)
                    .attr("x2", (link) => trimmedLinkCoordinates(link).x2)
                    .attr("y2", (link) => trimmedLinkCoordinates(link).y2);

                nodeSelection.attr("transform", (node) => `translate(${node.x},${node.y})`);
                labelSelection
                    .attr("x", (node) => node.x)
                    .attr("y", (node) => node.y + nodeRadius(node) + 14);
            });

            state.graph.simulation?.stop();
            state.graph.simulation = simulation;
            renderSelection();
            updateGraphSelection();
            if (state.graph.needsFit) {
                window.setTimeout(fitGraph, 80);
            }
        }

        function connectStream() {
            if (state.stream) {
                state.stream.close();
            }
            const source = new EventSource(streamUrl(), { withCredentials: true });
            state.stream = source;
            setStatus("Connecting");
            source.addEventListener("trail-sync", (event) => {
                const payload = JSON.parse(event.data);
                state.binding = payload.binding || state.binding;
                state.snapshot = payload.snapshot || null;
                writeStoredTrailRoot(state.binding?.trailRootRecordId);
                renderBinding();
                renderGraph();
                setStatus("Live");
            });
            source.addEventListener("trail-error", (event) => {
                const payload = JSON.parse(event.data);
                setStatus(payload.message || "Stream error");
            });
            source.onerror = () => {
                setStatus("Stream disconnected");
            };
        }

        function bindTrailRoot(recordId) {
            state.graph.needsFit = true;
            setStatus("Binding trail");
            return postJson("bind-trail", {
                trailRootRecordId: Number(recordId),
            }).then((result) => {
                state.binding = result.detail || null;
                writeStoredTrailRoot(state.binding?.trailRootRecordId);
                renderBinding();
                connectStream();
            }).catch((error) => {
                setStatus(error.message);
            });
        }

        function createTrail() {
            if (!state.selectedOriginal) {
                setStatus("Select an original");
                return;
            }
            setStatus("Creating");
            state.graph.needsFit = true;
            postJson("create-trail", {
                sourceRecordId: nodeIdFromRow(state.selectedOriginal),
                assignee: elements.createAssignee.value.trim(),
                scope: elements.syncScope.value || "t",
                fields: currentFields(),
            }).then((result) => {
                state.binding = result.detail || null;
                writeStoredTrailRoot(state.binding?.trailRootRecordId);
                renderBinding();
                connectStream();
                setStatus("Trail created");
            }).catch((error) => {
                setStatus(error.message);
            });
        }

        function applyQuantityChanges(changes) {
            if (!Array.isArray(state.snapshot?.rows)) {
                return;
            }
            const byId = new Map(
                (Array.isArray(changes) ? changes : [])
                    .map((entry) => [Number(entry.recordId), Number(entry.quantity)]),
            );
            if (!byId.size) {
                return;
            }
            state.snapshot.rows = state.snapshot.rows.map((row) => {
                const recordId = nodeIdFromRow(row);
                if (!byId.has(recordId)) {
                    return row;
                }
                return {
                    ...row,
                    quantity: byId.get(recordId),
                };
            });
        }

        function initializeTrail() {
            if (!state.binding?.trailRootRecordId || !Array.isArray(state.snapshot?.rows)) {
                return;
            }
            const previousRows = state.snapshot.rows.map((row) => ({ ...row }));
            const optimisticChanges = state.snapshot.rows.map((row) => ({
                recordId: nodeIdFromRow(row),
                quantity: nodeIdFromRow(row) === Number(state.binding.trailRootRecordId) ? -1 : 0,
            }));
            applyQuantityChanges(optimisticChanges);
            renderGraph();
            setStatus("Resetting trail");
            postJson("initialize-trail", {
                trailRootRecordId: state.binding.trailRootRecordId,
            }).then((result) => {
                applyQuantityChanges(result?.detail?.changed || []);
                renderGraph();
                setStatus("Trail reset");
            }).catch((error) => {
                state.snapshot.rows = previousRows;
                renderGraph();
                setStatus(error.message);
            });
        }

        function runSync() {
            if (!state.binding?.trailRootRecordId) {
                return;
            }
            setStatus("Syncing");
            postJson("run-trail-sync", {
                trailRootRecordId: state.binding.trailRootRecordId,
                scope: elements.syncScope.value || "t",
                fields: currentFields(),
            }).then((result) => {
                state.binding = {
                    ...(state.binding || {}),
                    ...(result.detail || {}),
                };
                renderBinding();
                setStatus("Sync requested");
            }).catch((error) => {
                setStatus(error.message);
            });
        }

        function setSelectedNodeQuantity(quantity) {
            const node = selectedNode();
            if (!node || !state.binding?.trailRootRecordId) {
                return;
            }
            setStatus("Updating node");
            postJson("set-trail-quantity", {
                trailRootRecordId: state.binding.trailRootRecordId,
                recordId: node.id,
                quantity,
            }).then(() => {
                setStatus("Node updated");
            }).catch((error) => {
                setStatus(error.message);
            });
        }

        function loadContract() {
            setStatus("Loading");
            return fetch(contractUrl(), { credentials: "same-origin" })
                .then((response) => response.json().then((data) => ({ ok: response.ok, data })))
                .then(({ ok, data }) => {
                    if (!ok) {
                        throw new Error(data?.message || data?.error || "Failed to load contract.");
                    }
                    state.binding = data?.binding || null;
                    if (state.binding?.sync) {
                        elements.syncScope.value = state.binding.sync.scope || "t";
                        applyFields(state.binding.sync.fields || "hb");
                    } else {
                        elements.syncScope.value = "t";
                        applyFields("hb");
                    }
                    updateScopeCopy();
                    renderBinding();
                    renderSelectedOriginal();
                })
                .then(() => {
                    if (state.binding?.trailRootRecordId) {
                        writeStoredTrailRoot(state.binding.trailRootRecordId);
                        connectStream();
                    } else {
                        const storedTrailRoot = readStoredTrailRoot();
                        if (storedTrailRoot) {
                            return bindTrailRoot(storedTrailRoot);
                        }
                        setStatus("Ready");
                    }
                    return refreshDiscoverResults().catch((error) => {
                        console.error(error);
                    });
                });
        }

        function applyFields(fields) {
            const normalized = normalizeFields(fields);
            elements.syncFieldQ.checked = normalized.includes("q");
            elements.syncFieldH.checked = normalized.includes("h");
            elements.syncFieldB.checked = normalized.includes("b");
        }

        initializeGraph();
        renderSelection();

        elements.searchHead.addEventListener("input", () => scheduleDiscoverRefresh(200));
        elements.searchCategory.addEventListener("input", () => scheduleDiscoverRefresh(200));
        elements.searchAssignee.addEventListener("input", () => {
            requestAssigneeSuggestions("discover");
            refreshDiscoverResults().catch((error) => setStatus(error.message));
        });

        elements.discoverResults.addEventListener("click", (event) => {
            const openButton = event.target.closest("[data-open-root]");
            if (openButton) {
                bindTrailRoot(openButton.getAttribute("data-open-root"));
                return;
            }
            const useOriginalButton = event.target.closest("[data-use-original]");
            if (useOriginalButton) {
                const recordId = Number(useOriginalButton.getAttribute("data-use-original"));
                state.selectedOriginal = state.discoverResults.find((row) => nodeIdFromRow(row) === recordId) || null;
                renderSelectedOriginal();
                renderDiscoverResults();
                return;
            }
            const card = event.target.closest("[data-original-id]");
            if (!card) {
                return;
            }
            const recordId = Number(card.getAttribute("data-original-id"));
            state.selectedOriginal = state.discoverResults.find((row) => nodeIdFromRow(row) === recordId) || null;
            renderSelectedOriginal();
            renderDiscoverResults();
        });

        elements.createAssignee.addEventListener("input", () => {
            renderSelectedOriginal();
            requestAssigneeSuggestions("create");
        });

        elements.searchAssigneeSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-assignee-kind]");
            if (!button) {
                return;
            }
            applyAssigneeSuggestion(
                button.getAttribute("data-assignee-kind"),
                button.getAttribute("data-assignee-value") || "",
            );
        });

        elements.createAssigneeSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-assignee-kind]");
            if (!button) {
                return;
            }
            applyAssigneeSuggestion(
                button.getAttribute("data-assignee-kind"),
                button.getAttribute("data-assignee-value") || "",
            );
        });

        elements.createSubmit.addEventListener("click", createTrail);
        elements.initializeTrail.addEventListener("click", initializeTrail);
        elements.runSync.addEventListener("click", runSync);
        elements.bindSelected.addEventListener("click", () => {
            const node = selectedNode();
            if (!node) {
                return;
            }
            bindTrailRoot(node.id);
        });

        elements.setLocked.addEventListener("click", () => setSelectedNodeQuantity(0));
        elements.setReady.addEventListener("click", () => setSelectedNodeQuantity(-1));
        elements.setDone.addEventListener("click", () => setSelectedNodeQuantity(1));

        elements.physicsCharge.addEventListener("input", () => {
            state.physics.charge = Number(elements.physicsCharge.value);
            applyPhysics();
        });
        elements.physicsDistance.addEventListener("input", () => {
            state.physics.distance = Number(elements.physicsDistance.value);
            applyPhysics();
        });
        elements.physicsCollision.addEventListener("input", () => {
            state.physics.collision = Number(elements.physicsCollision.value);
            applyPhysics();
        });

        elements.fitButton.addEventListener("click", fitGraph);
        elements.centerButton.addEventListener("click", centerGraph);
        elements.zoomOutButton.addEventListener("click", () => zoomBy(1 / 1.18));
        elements.zoomInButton.addEventListener("click", () => zoomBy(1.18));

        elements.syncScope.addEventListener("change", updateScopeCopy);
        [elements.syncFieldQ, elements.syncFieldH, elements.syncFieldB].forEach((input) => {
            input.addEventListener("change", renderSyncInputs);
        });

        document.addEventListener("click", (event) => {
            if (!event.target.closest(".autocompleteHost")) {
                hideAssigneeSuggestions("discover");
                hideAssigneeSuggestions("create");
            }
        });

        loadContract().catch((error) => {
            setStatus(error.message || "Failed to load Trail Relation");
        });
    })();
    "####.to_string()
}
