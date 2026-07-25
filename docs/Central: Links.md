## [x] Links — the graph

- [x] `add-link`, `remove-link`, `relink-order` (drag-reorder sugar);
  identity is the triple (from, kind, to) — the same two records carry many
  link kinds.
- [x] Adding an order-like link (`precedes`/`before`/`order`/descendants)
  that closes a loop succeeds but warns; non-order kinds never warn (mutual
  recipes are legal).
- [x] Protein `include: { links: { kinds, direction, depth } }` BFS-expands
  and stamps each link with its `hop`; `order: [{ topo: "before" }]` gives
  the focus queue.
- [x] Tags/clusters are links (`linked_to: { kind: "tag", to }`), composing
  with `any`/`not`/`all` for include+exclude filtering.
