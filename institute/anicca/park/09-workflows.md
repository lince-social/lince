# 9. Daily workflows, feature inventory and combinations

[Organization](Park.md), [targets](08-benchmarks.md), [inventory task P124](05-feature-costs-and-scaling.md), [coverage task P134](11-acceptance-and-experiments.md). This seed inventory maps the active app's feature families to directly experienced work. P124 must reconcile every reachable registration/control with this inventory and existing selectors before claiming source-complete coverage. No finite plan covers every possible document, timing and feature permutation.

## Stable workflow IDs

Each row supplies an action, the actual outcome to observe and performance dimensions. Reuse existing practice/Laboratory tests and normal shared controls; one row is not permission to create a duplicate suite. Automated synthetic input proves the exercised path only. Physical device/scanout, hardware layout and actual-session visibility remain separately identified.

| ID | Workflow | Outcome that must remain correct | Main budgets |
| --- | --- | --- | --- |
| W01 | Focus a native editor; type, move/select text, delete, undo/redo and paste synthetic Unicode | Text/preedit/selection follows the ordered input without loss, wrong focus, duplicate commits or private clipboard capture. Pending paste and IME retain their lifetime. | B02/B03/B08 |
| W02 | Edit a local Record title; change focus, inspect another view and reopen | Immediate draft, same UID/source in all views, exact durable final text, honest status and no remote wait. Title edits do not rebuild an unrelated body. | B02/B05/B07 |
| W03 | Edit a local description in Raw, Pretty and Split, including a paragraph in a large document | Editor stays responsive; appropriate preview, links/assets/shaders and multiple views converge without selection/scroll loss or whole-document work per key. | B02/B05/B07/B12 |
| W04 | Make an edit while private storage is held or fails; continue typing and then retry/reopen | Echo and other controls work; draft remains recoverable; errors are visible; acknowledgement matches a durable revision and replay does not duplicate edits. | B02/B05, held-write exception labeled |
| W05 | Show the same Record in several views, including a query row, full Record and another workspace | Local/remote changes reach actual dependents, sort/filter/count membership stays correct, and one edit does not trigger unnecessary repeated saves. | B05/B07/B12 |
| W06 | Open/close an existing local panel, native Sand, full Record or external application | Useful prompt feedback, correct current output/camera placement, usable focus, complete content and cancellation; no freeze of an already-open editor. | B02/B04 |
| W07 | Scroll a panel/list/editor with wheel and touchpad, including nested scroll roots | Correct units/phase/ordering, clipping, hit coordinates, scroll bounds, selected/focused rows and new/replaced children; no crash or queue exhaustion failure. | B02/B03/B12 |
| W08 | Click/select/activate a control while content updates or output geometry changes | The action still belongs to the current owner/target/version; no stale queued press invokes a replacement action or wrong recipient. | B02/B03 |
| W09 | Pan/zoom the infinite canvas, including large coordinates, pinned UI and off-screen content | Retained content remains sharp at current density; visibility, input, capture and origin shifts match the current output. No global reflow for pure translation. | B01/B02/B08/B12 |
| W10 | Drag/resize/group an ordinary item across Areas and between outputs | Correct placement, pointer capture, inherited appearance, Fit/Fill, nesting and physics effects; unchanged text is retained where valid. | B01/B02/B08 |
| W11 | Open/focus/move windows with tiling and maximize/fullscreen, using navigation shortcuts | Windows remain reachable in the intended canvas grid/row; focus/geometry follow current output, gap and constraints; no special application exemption. | B01/B02/B04 |
| W12 | Change global/per-kind/workspace/individual tokens, reset/import/export appearance and drag a style slider | Correct inheritance and explicit colors/zeros, one coherent theme revision, no border-only/blinking content, bounded write cost and cheap subsequent reads. Export changes appearance, not Records. | B02/B08/B13 |
| W13 | Edit a settings field, expand accordions, search/select a dropdown and drag a slider | Focus/draft/validation survives refresh; no panel rebuild on every input and no stale action after source/tab replacement. | B02/B04/B08 |
| W14 | Filter/sort/group a Protein result, insert/remove a matching Record and scroll to a focused/dirty row | Correct membership/order/aggregates and stable UID identity; virtualized rows preserve editing, selection and reveal/navigation. | B02/B04/B07/B12 |
| W15 | Create a Record; change quantity/unit, assertions/references or identity; run a harmless local action | Correct canonical mutation, facts/history, access and live views; no repeated side effect from batching/retry or lost meaningful state transition. | B02/B05/B07/B14 |
| W16 | Browse threads/messages/Fiote output while editing elsewhere, then cancel a held job | Incremental output, correct message identity/order and cancellation; no long message/model/queue phase blocks local input. Use prepared/held local tools, not a real paid/network job. | B02/B04/B12 |
| W17 | Run the harmless saved Command and interact with native Terminal while output streams | Correct output bytes/history/status, responsive stdin and other editors, bounded ordered publication and cleanup at exit. | B02/B03/B12 |
| W18 | Keep clocks/calendar/timer/kanban/todo/frequency and a prepared Karma rule active | Required deadlines and semantic transitions continue; unchanged unrelated views do not poll/rebuild. No missed timer/automation event while hidden. | B02/B08/B10/B12 |
| W19 | Open a local file/folder/document/source/IDE tab; edit and explicitly save a private sample | Prompt usable shell, correct final data, cancellation/errors/disk-conflict handling and unrelated input responsiveness. Preserve actual existing autosave/durability semantics. | B02/B04/B05/B12 |
| W20 | Play prepared media/video/audio/shader, then hide/reveal and open a second view | Correct playback and content, required audio/recording continues, uploads/paint follow genuine demand, compatible immutable results shared. | B01/B02/B10/B12 |
| W21 | Move items through Areas or interact with drawing/map/2D–3D/topology views | Correct transforms/reach/effects, useful hit testing, current live content and no all-canvas scan when a cheaper exact dependency path wins. | B01/B02/B12 |
| W22 | Inspect native capture/recorder/privacy or copy/paste synthetic local test data | Explicit owner/UID/placement policy, required semantics and correct resulting data/pixels; no private content leaks during retained/stale-frame reuse. Real clipboard contents are untouched by automated cases. | B01/B02/B11/B12 |
| W23 | Use keyboard/accessible navigation, then attach/detach the existing accessibility consumer in isolation | Correct focus, labels/state/selection/reveal and password privacy; incremental tree updates, no visual-only shortcut breaks semantics. | B02/B03/B08 |
| W24 | Keep Firefox at 100% zoom active with a 720p60 video while typing or dragging native content on the other monitor | Both outputs meet their own targets; browser decode/callbacks distinguished from real presentation; no starvation of native or client path. | B01/B02/B06/B11 |
| W25 | Close/reopen several instances; switch inactive workspaces; reveal previously off-screen edited content | Complete current content on reveal; needed background work survives; no stale cache, leaked subscription/worker/asset or lost dirty draft. | B04/B08/B10/B12 |
| W26 | Change a source/owner/Record identity or revoke access while a job/frame/input is queued | Old results and actions cannot apply to the replacement; required cancellation/retirement/privacy epoch respected without waiting for arbitrary polling. | B02/B03/B12 plus correctness |
| W27 | Output layout changes while content is busy or the session is locked | Current geometry/focus/input mapping and capture/privacy remain correct; no exposed stale frame, swapped saved workspace or lost reachable window. Reuse existing regression fixtures; physical actions only if separately needed/approved. | B01/B02/B11 plus correctness |
| W28 | Open Laboratory, run a prepared case, Stop/Close and return to normal workspace | Temporary data/isolation, exact case counts, bounded metrics and truthful pending results; previous workspace/input/drafts recover. | B04/B10/B12 |

## Source feature-family inventory to complete

This is a map for P124, not an assertion that every listed feature is exercised by an existing automated case. Names come from current native/app.rs and related registrations. Add exact source/selector/cost ownership in the manifest. Optional external hardware/network paths are represented with private prepared sources or marked unobserved; this does not expand physical/provider acceptance beyond the existing scope.

| Shared family | Features/registrations to reconcile | Workflows and directly experienced costs |
| --- | --- | --- |
| Text, icons and controls | Sand, SandText, editable text/input, icons, themes, tokens, sliders, dropdowns, tooltips/toasts, menus | W01/W07/W08/W12/W13; input, resolver reads, text, layout and complete pixels. |
| Workspace/canvas/editing | Canvas, controls, workspaces, edit mode/store, item properties, layout, composition, panels | W06–W13/W25; demand, focus, panel churn, transform versus layout. |
| Record and query presentation | Record views/binding, full Record, Protein Castle/Area, descriptions, record extensions, castle feed | W02–W05/W14/W15; recovery, mutation, fanout, preview and row identity. |
| Assertions/vocabulary/access | Assertion Castle/editor, Ontology, trust, visibility, access control, Organ/context | W14/W15/W26; lookup/validation, membership, access correctness and publication. |
| Time and planning | Time Castle, work timer, calendar, kanban, todo, frequency, schedule controls | W18/W25; source deadlines, hidden semantic work and unchanged UI. |
| Automation and simulation | Karma, Demeter, simulation, transfer, rules/actions | W15/W18; useful background evaluation versus local transaction contention and presentation. |
| Threads and assistants | Thread Castle, Fiote session/character, communication/speech/listening/call paths | W16/W25/W26; streaming output, workers, persistence/cancellation and view lifecycle. |
| Local programs | Terminal, Command Castle, saved commands/history | W17; bounded output service and producer notifications. |
| Local files and developer views | File explorer, document viewer, source view, IDE, external drop/openers | W06/W19; read/parse/decode, eager resources, saving and conflict handling. |
| Images/media/capture | Media Sand, video, sound, sound Areas, camera, recorder, capture controls | W20/W22/W24; producer demand, shared decode/upload, clocks and ownership. |
| Shader/spatial/graphics | Shader Castle/effects, drawing, map/location, topology/splats/rooms/terrain, physics/Areas | W09/W10/W20/W21; changed participants, pipeline readiness, geometry and GPU work. |
| Sync and maintenance | Sync Castle, information, configuration, backup, notifications, component pushes | W06/W13/W25/W26; subscriptions, polling, status refresh, background I/O and initialization. |
| Learning/testing/navigation | Instinct/practice/tutorial, Laboratory, shortcuts, accessibility, inspection/credits | W01/W13/W23/W28; isolated state, result wiring, focus, export and lazy resources. |
| Park/native/client integration | External windows, output/layer-shell controls, launcher, clipboard/drop, capture/lock | W06/W08/W11/W22/W24/W27; independent routing/presentation, lifetime and security epochs. |
| Feature-gated compiled additions | Every extra native/app registration enabled by the actual shipping features, including games where present | Map to existing interaction/media/worker families and a representative start/focus/exit workflow; do not enable optional features solely to inflate the test set. |

Registration alone does not imply a system runs, allocates eagerly or wastes time. For every family, distinguish no instances, visible instances, off-camera instances, inactive workspace, no consumers, active semantic jobs, pending dirty edits and multiple consumers. Record which conditions are applicable rather than pretending every feature supports all of them.

## Finite combination factors

Use deterministic covering sets over valid pairs of these factors. A factor value does not multiply every underlying feature/Record by every other value. Match each factor pair to a workflow with an actual oracle. The manifest stores included/excluded pairs, scene identity and the reason a combination is inapplicable.

| Factor | Values to represent |
| --- | --- |
| Output activity | One output; two with one quiet; two both active; current mixed output density/scale only where configured. |
| Presentation population | Small/normal/dense; visible/off-camera/inactive; one versus multiple views of same data. |
| Focus/input | Native/client; mouse/touchpad/keyboard/IME; idle/focused editor/composition/pending paste. |
| Navigation/layout | Stationary/pan/zoom/drag/scroll; free/row-column/grid/tiling; nested clip and Fit/Fill; current 2D/3D paths where supported. |
| Data activity | Quiet; local head edit; local body edit; query membership change; remote deterministic update; source/owner replacement. |
| Style activity | Unchanged; local override/reset; inherited/relation change; global switch; continuous slider preview. |
| Background activity | None; required timer/rule; ready query/file task; terminal output; media/shader/video; prepared sync/assistant stream. |
| Storage/worker state | Ready; held/busy; failure then retry; completion after close/replacement; bounded queue pressure. |
| Lifecycle/privacy | First open/reopen; hide/reveal; dirty close/recovery; access revoked; lock/capture policy/output epoch change in existing isolated fixtures. |

Start with the cheap headless/work-count pairs, then run timing for representative normal/dense subsets and hardware-only rendering checks where needed. No compilation/private GPU renderer overlaps a measured physical run. Do not contact real peers, trigger a costly model job, alter real Records or fill the user's canvas to generate this set.

## Mandatory combinations beyond pairwise coverage

These are selected high-risk triples/transitions because they cross the exact proposed refactor boundaries. They are shared cases to reuse, not new task definitions or a request for repeated physical stress cycles.

| Combination | Why it is required and outcome |
| --- | --- |
| Native typing + held local write + Firefox on the other output | Separates echo/dispatch/render from storage and per-output work; no growing input age, lost draft or false saved status. |
| Local body edit + Pretty/Split preview + several views/query rows | Exposes full snapshot/markup/fanout amplification; only dependent views change and text remains exact. |
| Local edit + query sort/filter membership change + row virtualization | Exposes stale row identity or hidden dirty editor loss; correct membership/reveal and durable data. |
| Drag/scroll + inherited theme/Area change + current hit testing | Retained transforms cannot bypass real style/layout dependencies or queue a click to old geometry. |
| Theme update + first-open asynchronous content + the next complete frame | Detects missing text/icons/border-only rendering and mixed style epochs. |
| Typing + terminal/assistant output + required timer/rule | Tests queue fairness, semantic deadlines and bounded publication without losing output. |
| Off-camera data/style edit + workspace reveal + immediate input | Retained/hidden optimization must initialize current pixels/geometry before accepting the action. |
| Queued worker/GPU result + source/output replacement + capture/privacy change | Old generations cannot apply or leak pixels; submitted consumers retire safely. |
| Edit + explicit save/close + crash/reopen in a private fixture | Confirms batching/journal/compaction preserve recoverability and honest durable acknowledgement. |
| One-output video + other-output drag/type + dense native scene | Measures actual resource sharing and independent output responsiveness rather than empty-workspace FPS. |

P134 records source-complete family assignment, selected pairs, these cross-boundary cases and genuine gaps. P135 evaluates useful optimization combinations against them. New bug reports add a concrete reproducer to the appropriate shared workflow and an atomic repair task if needed; they do not justify redesigning each Castle separately.
