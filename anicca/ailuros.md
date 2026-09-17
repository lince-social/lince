# CAD for Ailuros — research

Researched 2026-09-16. Answers the `- [ ] Research` box in `Ailuros` (`@ailuros`).
Question asked: how does CAD work in the core, can we embed an open source one in
Lince — possibly as an external castle that still behaves natively — or do we
build our own.

Lince is MIT. The interface is Bevy 0.19 + avian3d, native, no embedded browser
(CEF rejected, Servo unresolved). Those three facts eliminate most of the field
before any geometry question is asked.

## 1. What a CAD program actually is

Every parametric CAD, from CATIA to Dune3D, is the same five layers stacked.
Nothing about the stack is secret; each layer is decades of edge cases.

**Layer 1 — the geometry kernel.** Curves and surfaces, almost always NURBS
(non-uniform rational B-splines): control points, knot vector, weights. A plane,
a cylinder and a torus are usually kept as analytic special cases because exact
forms intersect far better than sampled ones. This layer answers "where is this
surface at parameter (u,v)" and "where do these two surfaces cross".

**Layer 2 — topology (B-rep).** Boundary representation: a solid is a shell, a
shell is faces, a face is a trimmed surface bounded by loops of edges, an edge is
a curve between two vertices, and every one of them carries a tolerance. The
topology is what makes a bag of surfaces a watertight object. Booleans
(union/subtract/intersect) are the hard part: intersect every surface pair, build
the intersection curves, split faces along them, classify each piece as inside or
outside the other solid, stitch the survivors. Tangency, near-tangency and
coincident faces are where kernels die. This is why there are perhaps six
production-grade B-rep kernels on Earth — Parasolid, ACIS, CGM, OCCT, Solid
Modeling Solutions, and whatever Autodesk uses internally.

**Layer 3 — the constraint solver.** A sketch is points and lines plus
constraints (coincident, parallel, tangent, distance, angle, equal). The solver
turns those into a system of nonlinear equations and runs Newton-Raphson (usually
with a rank/DoF analysis first, to report under- and over-constrained sketches
instead of diverging). Solvespace's solver and Zoo's `ezpz` are both this. It is
a smaller, much more tractable problem than the kernel.

**Layer 4 — the feature tree (history).** A part is not geometry, it is a recipe:
sketch → extrude → fillet → pattern, replayed on every edit. This is what makes
CAD parametric, and it produces the single worst problem in the field:

> **Topological naming.** "Fillet the edge between face 3 and face 7" has to
> survive the earlier sketch changing so that the old face 7 no longer exists.
> Kernels name topology by generation order, so an upstream edit silently
> repoints the fillet at the wrong edge and the model explodes. FreeCAD lived
> with this for fifteen years and only shipped a real fix in 1.0 (the Ondsel
> naming algorithm). Any "build our own" plan pays this cost, and it is not a
> geometry cost — it is a persistent-identity cost, which is Lince's home turf.

**Layer 5 — tessellation and exchange.** The screen never sees B-rep; faces get
meshed into triangles per frame budget, with a separate high-quality mesh for
export. Interchange is STEP (ISO 10303, AP203/214/242 — the only format that
carries real B-rep between systems), IGES (dead but everywhere), STL/3MF for
printing, glTF for display.

### The alternative stacks

- **Mesh CSG.** Skip exact surfaces; keep triangle meshes and do robust boolean
  ops on them. Manifold's algorithm guarantees manifold-in → manifold-out. This
  is what OpenSCAD uses now. Cheap, robust, but you lose exact circles, exact
  fillets, and STEP export fidelity.
- **Implicit / f-rep.** A shape is a function f(x,y,z) < 0. Union is min,
  intersection is max, blends and lattices and offsets are trivial, booleans can
  never fail because there is no topology to corrupt. Mesh only at the end (dual
  contouring). This is libfive, Curv, Fidget, and commercially nTop. The trade:
  no exact edges to dimension, no B-rep to export, and mating/tolerancing —
  "this piece connects to that piece" — is genuinely awkward.
- **Voxel.** Only relevant if Ailuros' "tiny, tiny voxels" line from
  `Interfaceless` is taken literally. Not manufacturable-precision.

## 2. The decision that decides everything else

Ailuros wants "the smallest piece of it, that could connect with others" — parts
that mate, with tolerances, with build instructions as a Trail. That is a
B-rep/STEP problem, not an implicit problem. Implicit is the only option that
drops cleanly into a Bevy canvas at interactive rates with no C++ in the build,
and it is the wrong representation for the stated use.

So: exactness is needed, and exactness is not obtainable in pure Rust today.

## 3. The field itself

Before asking what we can embed, what exists and how each one is built.

### The commercial tier — kernels you rent

Almost no CAD vendor writes its own kernel. There are four that matter and they
are licensed, not sold, at five to six figures plus royalties.

- **Parasolid** (Siemens, from the 1988 Romulus lineage) — the industry default.
  NX, Solid Edge, SolidWorks (until its own fork), **Onshape**, **Shapr3D**,
  **Plasticity**. If a young commercial CAD feels unreasonably solid at
  booleans and fillets, it is Parasolid underneath and the company's real work
  is the UI.
- **ACIS** (Spatial/Dassault) — the other old one. DraftSight, many CAM tools.
- **CGM** (Dassault) — CATIA's kernel, now sold separately.
- **D-Cubed DCM** (Siemens) — not a kernel, the *constraint solver* half,
  licensed separately and used by most of the above. That it is a separate
  product is the evidence that layer 3 is genuinely separable.

What renting buys: thirty years of boolean edge cases and someone else's
liability. What it costs: money, a closed binary, and no chance of it living
inside an MIT program. Out of reach for us, and worth knowing precisely *what*
is out of reach — the kernel, not the application.

### The free field

- **FreeCAD** (LGPL) — OCCT + Coin3D + Qt + Python. A workbench shell rather
  than a program; every discipline is a plugin. Its Python layer means a feature
  is a script, which is the pattern worth stealing. 1.0 finally shipped the
  topological-naming fix. Huge, C++, a whole desktop app — a teacher, not a
  dependency.
- **Dune3D** (GTK4, v1.4.0 "Einstein", Jan 2026) — Solvespace's solver + OCCT +
  a modern UI, 33 MB, one person, 2.5 years. The proof that the stack is
  assemblable at small scale by one determined human.
- **SolveSpace** (GPLv3) — tiny, its own kernel and its own NURBS, and the
  solver everyone else extracts. Famous for the solver, limited at booleans.
- **OpenSCAD** (GPL) — script only, no GUI model, **no constraint solver, no
  feature tree**, mesh CSG (now on Manifold). The existence proof that layers 3
  and 4 are optional if your input language is good enough. Its descendants:
  **ImplicitCAD**, **Curv**, **Antimony**.
- **CadQuery** and **build123d** (Apache-2.0, Python over OCCT) — the closest
  prior art to what Lince should do. A part is a program; selectors pick faces
  and edges by *query* ("the topmost circular face") rather than by index, which
  is an answer to topological naming that sidesteps persistent ids entirely.
  Read build123d's selector design before designing ours.
- **BRL-CAD** (BSD/LGPL, in continuous development since **1979**, ~1M lines) —
  the great unknown major. Its architecture is a third option nobody else
  offers: CSG-primary, and geometry is evaluated by **ray-tracing** (`librt`)
  rather than tessellated. Instead of computing a boundary, you shoot rays and
  classify intervals per ray, which makes booleans trivially robust and gives
  you analysis (mass, shielding, line-of-sight) as a by-product. It is slow to
  edit, alien to modern CAD UI, and unmatched at "is this assembly actually
  solid". Worth knowing about if Ailuros ever needs to *analyse* a device rather
  than draw one.
- **openNURBS** (Rhino, freely licensed C++ with source) — not a kernel, a
  read/write library for the `.3dm` format plus NURBS evaluation. The way into
  the Rhino/Grasshopper world if that ever matters.
- **Blender / Gmsh / Salome** — mesh modelling, meshing and FEA respectively.
  Adjacent, not CAD.
- **Zoo Design Studio** — the app is open source, the geometry engine is a
  hosted GPU service; the "3D view" is a video stream over WebSocket. Their
  language KCL is Rust and their solver `ezpz` is MIT. Take those two, leave the
  architecture.
- **nTop** — commercial implicit modelling, originally built on Matt Keeter's
  libfive. The proof that f-rep is a real engineering stack and not a demo, for
  lattices, blends and generative shapes specifically.

## 4. Candidates, filtered

Filter order: license compatible with MIT → no browser, no Qt/GTK app → buildable
in our tree → geometry depth.

### Usable

| Thing | License | What it is | Verdict |
|---|---|---|---|
| **OCCT** (via `opencascade-rs` / `occt-rs`) | LGPL-2.1 **with a narrow exception** — see below | The only free production B-rep kernel. Booleans, fillets, STEP r/w, healing. Powers FreeCAD, Dune3D, CadQuery, KiCad's 3D. | Usable, with a licensing catch that bites our single-binary shape. Cost: a C++ build (CMake, minutes to tens of minutes), and the Rust bindings are partial. `bschwind/opencascade-rs` is the established one but thin; `occt-rs` is 2026, early, partly AI-written, AGPL-with-CLA — avoid that one. |
| **`ezpz`** (Zoo/KittyCAD) | MIT | 2D geometric constraint solver in Rust, ships in Zoo Design Studio, works native or wasm. Built specifically because slvs is GPL and D-Cubed is expensive. | Best-in-class fit. Layer 3 is solved for us, for free, in-language. |
| **Manifold** — `meshbool` (pure Rust port, Apache-2.0, tracks upstream v3.5.1, passes 245 of the C++ tests), or `manifold-csg` (bindings) | Apache-2.0 | Guaranteed-manifold mesh booleans. | The pragmatic middle. No C++ needed with `meshbool`. `meshbool` is small and young (7 stars) but the boolean core is the part that is done. |
| **Fidget** (Matt Keeter) | MPL-2.0 | Implicit-surface evaluation with a hand-written aarch64/x86_64 JIT, 31× over its own interpreter; manifold dual contouring meshing; 2D/3D bitmap rendering. | The best Rust engine in the whole survey, for the representation we probably don't want. Keep for organic/lattice/blend work, not for mating parts. |
| **truck** / **monstertruck** | Apache-2.0 | Pure-Rust B-rep with NURBS, `truck-shapeops` booleans, `truck-stepio` STEP. `monstertruck` is virtualritz's fork (upstream PRs were stalling) adding a rewritten fillet engine with per-edge and variable radii, chamfers, `Result`-returning booleans, shape healing, T-splines, STEP assembly output. | The only pure-Rust path to exact B-rep. Booleans exist but are not battle-hardened — the docs themselves ship a "robust version of splitting closed edges and faces" as a separate opt-in, which tells you what the default is. |
| **`csgrs`** | MIT | OpenSCAD-shaped CSG over BSP trees, wired into the dimforge ecosystem (nalgebra/parry/rapier) we already use via avian3d. 32/64-bit, wasm. | Easiest thing to get a shape on screen with. Ceiling is low. |

### Read the OCCT exception before counting on it

I read the actual text (`OCCT_LGPL_EXCEPTION.txt`). It is much narrower than its
reputation:

> "The object code form of a 'work that uses the Library' can incorporate
> material from a header file that is part of the Library. As a special
> exception to the GNU Lesser General Public License version 2.1, you may
> distribute such object code incorporating material from header files provided
> with the Open CASCADE Technology libraries (including code of CDL generic
> classes) under terms of your choice, provided that you give prominent notice
> in supporting documentation to this code that it makes use of or is based on
> facilities provided by the Open CASCADE Technology software."

That only lifts the header/inline/template-instantiation clause of LGPL 2.1. It
does **not** grant general static-linking freedom. So the ordinary LGPL rule
still applies: dynamic linking is fine and our own code stays MIT, but the user
must be able to swap in their own build of the library. Lince ships as **one
binary** (`lince`, Cell plus optional UI) — statically linking OCCT into it would
put us in the "provide object files or otherwise allow relinking" obligation,
which is a distribution and CI problem, not a code problem. Dynamic linking a
system OCCT is clean but makes the single binary depend on a large C++ library
being present, which contradicts the whole point of shipping one file. Decide
this before writing any binding code; it is the actual blocker on OCCT, not the
API.

### Rejected, and why

- **FreeCAD / Dune3D / SolveSpace as embedded apps** — all are Qt/GTK
  applications with their own event loop and document model. Embedding means
  either a second window we do not control or a browser we already rejected.
  Dune3D (v1.4.0 "Einstein", Jan 2026, GTK4) is worth studying as a design: it
  is precisely Solvespace's solver + OCCT + a modern UI, in 33 MB. That is the
  architecture, proven at small scale, by one person.
- **Solvespace's `slvs` / the `slvs` Rust crate** — GPLv3. Incompatible with MIT
  Lince. `ezpz` exists to replace exactly this.
- **Zoo Design Studio** — the app is open, the geometry engine is not: it is a
  hosted GPU service and the 3D view is a **video stream over WebSocket**. Nothing
  to embed, and it inverts our model of where data lives. Take `ezpz` and leave
  the rest.
- **Fornjot** — the obvious answer two years ago, an all-Rust b-rep kernel. Hanno
  Braun ended it; the repository was archived 2026-06-19 with the goals
  explicitly not reached. Read its post-mortems before starting a kernel.
- **`zenith-cad-kernel`** — from-scratch Rust B-rep, exact booleans, STEP, ~132k
  lines, 8 crates, MPL-2.0. Written in **29 days** (2026-08-19 → 09-16) by one
  person with an AI, zero stars, cannot read all real STEP files. Genuinely
  interesting and completely unproven. Watch; do not depend.
- **CADmium** — browser CAD over `truck`, prototype, quiet since 2024.
- **libfive / Curv** — C++ ancestors of Fidget. Use Fidget instead.

## 5. What "external castle that behaves natively" costs

Worth pricing honestly, because it sounds free and is not.

A separate process holding the model means the geometry state lives **outside the
Box**. Protein supplies authorized reads and Actions own writes; a CAD process
with its own document is a second source of truth with no Protein binding, so no
Area of Influence can filter parts, no Protein Action can change a dimension, and
nothing about the model syncs to another Organ. We would have to invent a bridge
whose only job is re-teaching the external tool what a Record is — the same
mistake the web crate made with its own state, one process further away.

It also breaks rendering: to "behave natively" the castle must draw inside our
canvas, and an external GTK/Qt app cannot. The only ways are a video stream
(Zoo's answer — and it forces their hosted-engine architecture on us) or shared
GPU buffers (fragile, platform-specific).

And determinism. The interface already runs avian3d with `enhanced-determinism`,
and the DST plan wants a seed to reproduce a run exactly. If part geometry ends
up in Records that sync, a nondeterministic kernel — floating-point tolerance
decisions, thread-order-dependent boolean results — is a **sync divergence**, not
just a rendering wobble. An in-process Rust library we can pin and seed is
checkable; an external binary is not.

Conclusion: an external process is fine as a **converter** (feed it a file, get a
file back, e.g. headless FreeCAD for a STEP import we cannot parse) and wrong as
a castle.

## 6. What this points at

Not "embed a CAD". Not "write a kernel". The third thing:

**Lince owns layers 3 and 4, and borrows layers 1, 2 and 5.**

Layer 4 — the feature tree with stable names — is the part Lince is already
unusually well equipped for. A feature tree is an ordered DAG of parameterised
operations with durable identity, which is a Record tree with `@part-of` and
uids. The topological naming problem is a persistent-identity problem, and
Lince's whole thesis is that identity is a uid, never a position. A sketch
dimension that is a Record quantity is dimension-driven design for free, and
makes a part a thing you can Need, contribute to, transfer and sync — which no
CAD file can be. That is the part worth building, and it is the part nobody else
has.

Layers 1–2 stay borrowed: `monstertruck`/`truck` while it holds, OCCT when
exactness wins over the single-binary constraint, `meshbool` for display-time and
for anything that only has to be printable. Layer 3 is `ezpz`. Layer 5 is
`truck-stepio` plus our own tessellation into Bevy meshes. Fidget lives beside
it all as a second representation for the organic parts, never as the mating
geometry.

**The one architectural decision, and it has to be made on day one.** Layers 3
and 4 — sketch points as Records, constraints as assertions, the feature DAG —
are portable across every kernel in this document. Layers 1 and 2 are not: a mesh
fillet is not a B-rep fillet, and a feature tree written directly against mesh
CSG does not carry forward, it gets rewritten. So the kernel must sit behind one
narrow internal trait from the very first commit — roughly `solid from profile +
extrusion`, `boolean`, `fillet edges`, `tessellate`, `export` — with mesh CSG as
its first implementation and B-rep as a later one. Put that seam in on day one
and the prototype is a prototype; leave it out and the prototype is a throwaway.
That is the only decision here that is expensive to reverse.

Concretely the first honest step is small: one castle that holds a 2D sketch
whose points are Records and whose constraints are assertions, solved by `ezpz`,
extruded into a Bevy mesh through that trait, implemented by `csgrs` or
`meshbool`. No kernel, no C++, no external process, MIT throughout. If
sketch-plus-extrude-plus-boolean survives contact with a real part, the B-rep
question becomes answerable with evidence rather than by survey.

## 7. Numbers worth keeping

- Production free B-rep kernels: **one** (OCCT). Rust ones: **zero proven**.
- Fornjot: **archived 2026-06-19**, goals not met, after ~4 years.
- Dune3D: one person, 2.5 years, 33 MB, solver + OCCT + GTK4 — the whole
  architecture, at the smallest size anyone has managed.
- FreeCAD: **15 years** to fix topological naming.
- Fidget JIT: 1024² brute-force, 5.8 s interpreted → **182 ms** JIT.

## Sources

- Fornjot archived: https://github.com/hannobraun/fornjot · https://www.fornjot.app/
- truck: https://github.com/ricosjp/truck · https://docs.rs/truck-shapeops
- monstertruck: https://github.com/virtualritz/monstertruck
- zenith: https://github.com/hinatahugu29/zenith-cad-kernel
- opencascade-rs: https://github.com/bschwind/opencascade-rs · occt-rs: https://github.com/occt-rs/occt-rs
- Fidget: https://github.com/mkeeter/fidget · https://www.mattkeeter.com/projects/fidget/ · meshing idea: https://www.mattkeeter.com/blog/2026-07-03-meshing/
- Manifold: https://github.com/elalish/manifold · meshbool: https://github.com/BorgerLand/meshbool · manifold-csg: https://github.com/zmerlynn/manifold-csg
- csgrs: https://github.com/timschmidt/csgrs
- ezpz: https://github.com/KittyCAD/ezpz · https://zoo.dev/blog/announcing-solver
- Zoo engine as video stream: https://github.com/KittyCAD/modeling-app
- slvs GPL: https://solvespace.com/library.pl · https://github.com/thekakkun/rust_slvs
- Dune3D: https://github.com/dune3d/dune3d · https://docs.dune3d.org/en/latest/why-another-3d-cad.html · FOSDEM 2026: https://fosdem.org/2026/schedule/event/UUVWPM-dune_3d_-_2_12_years_in_the_3rd_dimension/
- Topological naming: https://github.com/FreeCAD/FreeCAD-documentation/blob/main/wiki/Topological_naming_problem.md
- B-rep vs implicit: https://www.ntop.com/resources/blog/understanding-the-basics-of-b-reps-and-implicits/
- libfive: https://github.com/libfive/libfive
- CADmium: https://mattferraro.dev/posts/cadmium
- OCCT exception text: https://raw.githubusercontent.com/Open-Cascade-SAS/OCCT/master/OCCT_LGPL_EXCEPTION.txt
- BRL-CAD: https://brl-cad.github.io/docs/index.html · https://brlcad.org/~maths22/books/en/HACKING_BRL-CAD.html
- openNURBS: https://www.rhino3d.com/features/developer/opennurbs/
- Parasolid / the rented-kernel tier: https://en.wikipedia.org/wiki/Parasolid
