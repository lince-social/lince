// Shared pure trail progression logic.
// Keep this file free of DOM and Datastar access so it can be tested directly.
globalThis.TrailRelationLogic = (() => {
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

    function nodeIdFromRow(row) {
        return Number(valueOf(row, "id") || 0);
    }

    function rowParentIds(row) {
        return parseJsonArray(valueOf(row, "parentIdsJson", "parent_ids_json"))
            .map((value) => Number(value))
            .filter(Number.isFinite);
    }

    function normalizedQuantity(value) {
        const parsed = Number(value);
        if (!Number.isFinite(parsed)) {
            return 0;
        }
        if (Math.abs(parsed - 1) < 0.000001) {
            return 1;
        }
        if (Math.abs(parsed + 1) < 0.000001) {
            return -1;
        }
        if (Math.abs(parsed) < 0.000001) {
            return 0;
        }
        return parsed;
    }

    function trailMaps(rows) {
        const recordIds = new Set((Array.isArray(rows) ? rows : []).map((row) => nodeIdFromRow(row)));
        const parentMap = new Map();
        const childMap = new Map();
        const quantities = new Map();

        (Array.isArray(rows) ? rows : []).forEach((row) => {
            const recordId = nodeIdFromRow(row);
            const parents = rowParentIds(row).filter((parentId) => recordIds.has(parentId));
            parentMap.set(recordId, parents);
            quantities.set(recordId, normalizedQuantity(valueOf(row, "quantity")));
            parents.forEach((parentId) => {
                if (!childMap.has(parentId)) {
                    childMap.set(parentId, []);
                }
                childMap.get(parentId).push(recordId);
            });
        });

        return { parentMap, childMap, quantities };
    }

    function parentsComplete(recordId, parentMap, quantities) {
        const parentIds = parentMap.get(recordId) || [];
        if (!parentIds.length) {
            return true;
        }
        return parentIds.every((parentId) => normalizedQuantity(quantities.get(parentId)) === 1);
    }

    function visibleTrailRows(rows, boundRootId) {
        const normalizedBoundRootId = Number(boundRootId || 0);
        return (Array.isArray(rows) ? rows : []).filter((row) => {
            const recordId = nodeIdFromRow(row);
            if (normalizedBoundRootId > 0 && recordId === normalizedBoundRootId) {
                return true;
            }
            return normalizedQuantity(valueOf(row, "quantity")) !== 0;
        });
    }

    function computeTrailQuantityChanges(rows, trailRootRecordId, recordId, quantity) {
        if (!Array.isArray(rows) || !trailRootRecordId) {
            return { changes: [], error: "Trail snapshot is not loaded yet." };
        }

        const nextQuantity = normalizedQuantity(quantity);
        const { parentMap, childMap, quantities } = trailMaps(rows);
        const nextQuantities = new Map(quantities);
        const currentQuantity = normalizedQuantity(quantities.get(recordId));

        const resolvedTrailRootRecordId = Number(trailRootRecordId);
        if (recordId === resolvedTrailRootRecordId) {
            nextQuantities.set(recordId, nextQuantity);
        } else if (nextQuantity === 1) {
            if (!parentsComplete(recordId, parentMap, nextQuantities)) {
                return { changes: [], error: "All parents must be 1 before this record can be completed." };
            }
            nextQuantities.set(recordId, 1);
        } else if (nextQuantity === -1) {
            nextQuantities.set(recordId, parentsComplete(recordId, parentMap, nextQuantities) ? -1 : 0);
        } else if (!parentsComplete(recordId, parentMap, nextQuantities)) {
            nextQuantities.set(recordId, 0);
        } else {
            nextQuantities.set(recordId, currentQuantity);
        }

        const queue = [recordId];
        const visited = new Set();
        while (queue.length) {
            const current = queue.shift();
            if (visited.has(current)) {
                continue;
            }
            visited.add(current);
            const childIds = childMap.get(current) || [];
            childIds.forEach((childId) => {
                const existing = normalizedQuantity(nextQuantities.get(childId));
                if (parentsComplete(childId, parentMap, nextQuantities)) {
                    if (existing === 0) {
                        nextQuantities.set(childId, -1);
                    }
                } else if (existing !== 1) {
                    nextQuantities.set(childId, 0);
                }
                queue.push(childId);
            });
        }

        const changes = [];
        nextQuantities.forEach((resolvedQuantity, changedRecordId) => {
            const currentQuantity = normalizedQuantity(quantities.get(changedRecordId));
            if (currentQuantity !== resolvedQuantity) {
                changes.push({
                    recordId: changedRecordId,
                    quantity: resolvedQuantity,
                });
            }
        });
        changes.sort((left, right) => left.recordId - right.recordId);
        return { changes, error: null };
    }

    return {
        computeTrailQuantityChanges,
        normalizedQuantity,
        parentsComplete,
        trailMaps,
        visibleTrailRows,
    };
})();
