Needing information can be thought of as following a Trail. Some things have been said, thought, written, tested, built, and you are remembering it for the first time.

Allowing people catch up to what Lince is, on the Website, or what we did This Month In Lince is the building of trails. Calling it like that keeps the Lince animal names going, and it's not too much of a mental stretch to call it that way (hopefully).

Another kind of information trail is of DNA. In Lince DNA is the data of a particular use of the Lince tool, the database, the information saved itself. If we use the Lince tool to create trails we can help others understand some things faster than reinventing them.

Examples of the trails we can create is for knowledge of certain areas, acting as education on how something works, or how to act to create something. The act of farming is benefitted from understanding biology, but studying general biology will not give you specialized understanding of how to handle tools, what to focus on automating or how to take care of crops. With a large volume of eyes and voices we can generate blueprints of knowledge, accompanied with tips for action/habits in the implementation of some areas. The whole of our wisdom throught generations can be saved in a way that gives us the what, and the how to do anything. A wikihow that integrates the steps into the daily routine of your life system, so you can start today, learning and experimenting what you want to explore in life.

- [x] **Relations** (ships as the group with Record) — d3 force graph with
  physics sliders, golden-angle layout, zoom/pan/fit, directed arrows,
  edge-kind labels; Shift+drag adds a link (optimistic), edge-click selects,
  the header chip's ✕ removes; **Trail mode** lays a root's forward
  link-tree out topologically with a Done/Undo promotion cascade over
  shared status presets (quantity or concept buckets, e.g.
  `@todo/@next/@wip/@done` — the SAME vocabulary a kanban column can use);
  resizable controls panel, link keybinds (Delete unlinks, Ctrl+Z undoes a
  session-local stack); link-kind inputs autocomplete from concepts.
- [x] **Relations node gravity (tree weight refactor)** — one extra physics
  variable on top of the existing forces: each node gets a *weight* from its
  depth in the selected link tree — the root is heaviest (or lightest, when
  inverted) and each hop toward the leaves gets lighter; nodes not connected
  to the root weigh the same as leaf nodes. A vertical gravity bias then
  pulls heavy nodes down and lets light ones float up, so the tree settles
  into root-down/leaves-up (or inverted, root-up/leaves-down) while charge,
  link, collision, and center forces keep working unchanged. Configurable
  per sand: gravity direction toggle (root sinks / root floats), strength
  slider, and which link kind/root defines the tree (reuse the Trail mode
  root picker). Applies in both graph mode and Trail mode — in Trail mode it
  replaces/augments the fixed topological ranks with the same simulated
  gravity so the done/next/ahead coloring stays readable on a physically
  settled tree. (Shipped: Graph controls → Node gravity section. Weight maps
  to BUOYANCY — a constant per-node vertical acceleration, not a target
  line, so nodes keep falling until their link tethers them and the tree
  hangs like a mobile; the center forces stay on for cohesion while
  charge/link soften. Graph mode simulates all nodes this way; Trail mode
  unpins the rows and simulates only tree nodes, x anchored to the topo
  layer.)
