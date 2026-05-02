globalThis.KarmaOrchestraLogic = {
    uniqueBy(items, keyFn) {
        const out = [];
        const seen = new Set();
        for (const item of items || []) {
            const key = keyFn(item);
            if (seen.has(key)) continue;
            seen.add(key);
            out.push(item);
        }
        return out;
    },
};
