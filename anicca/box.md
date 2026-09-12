# Box

Box is the workspace where Sands are placed, grouped and connected. Begin with stable placement, navigation and saving. Keep optional motion for later without deleting the existing experiments.

## Data and appearances

A Protein area selects data and repeats a chosen group once per result. The person sees the available fields and connects them to compatible Sand inputs. A group can also contain ordinary labels, buttons and decoration. One field can feed several Sands, and one Sand can accept several fields.

Keep the source area's identity, result key and child identity stable through refresh. Selecting a Record can locate several appearances; choosing one edits only that appearance. If a result disappears, remove all its owned appearances and keep settings for an explained, bounded recovery period. A broken field connection shows what changed and offers repair.

The source must supply a stable result key before its rows can become saved repeated appearances. Names and a Protein's changing content hash are not that key. Zoom changes the view, not which Record fields a template requests or displays. Only a Protein area creates result groups; other Areas act on the data those groups already carry.

Releasing a child changes how it moves, while its source, permissions and group events remain the same. Copy and release gives another view of the property. Reattach without a jump. Removing a view never deletes its Record.

Switching between a Sand and a Castle previews common, missing and extra fields, affected overrides and released children. The person can choose how to map them, cancel or undo. Added reads are explicit; hiding fields preserves the data.

Match fields by meaning and identity, not just a similar label or type. Offer a local copy before changing a Protein used elsewhere. Missing values are not zero, and a button missing a required Action input stays unavailable. Removing a property's presentation also removes its hidden interaction and accessibility target.

The “Why is it here?” view explains the source, field, group, local overrides, layout and permissions. Later motion adds forces and terrain to this same explanation.

## Placement and editing

Support workspace, group and screen anchors, layers and stable ordering. Pan, zoom, search, recenter, a minimap and bring-selection-here help recover lost content. The background can grow from dots to plus signs to a mesh as zoom changes, with configurable scale and arm length. Wallpapers have explicit assets, fit, repeat, scale and opacity.

The arm-length slider runs from dots at 0% to a connected mesh at 100%; zoom adds or removes finer repetitions. Raster wallpapers and sanitized SVG remain inert, with no scripts or remote loads. Drawing keeps compact strokes and operations rather than saving or sending a whole bitmap after every change.

Appearance, Connections and Data are separate editing choices. A Sand grows from its minimum size until an optional maximum makes its content scroll. Focus opens a temporary reading view without changing saved sizes; long content remains navigable, and returning preserves drafts, caret and workspace position.

Stationary Areas group and sort by properties. For example, assignees can run downwards and due dates across. Missing values have a place, ties stay stable and manual placement is an explicit exception with a Follow layout control. Crossing a label does not itself change data.

Group multi-value fields by their complete set initially; splitting one result among several groups is a deliberate further choice. Fixed sorting bounds scroll internally when full. A released child needs its own explicit layout rule and is never silently attached again.

## Later motion

Force Areas attract or repel selected results; sorting Areas arrange them; mutation Areas request Actions on entry or exit. Immunity protects one source's results from outside effects while allowing its internal rules. Shared visit tracking prevents copied or released views from multiplying a mutation. Overlaps, retries and cycles need clear ordering, limits and pause controls.

Mutation Areas start disarmed. Preview the proposed Action, require a saved, inspectable grant and show whether the Area is armed, paused or failed, with immediate Disarm. Mutation requires a concrete writable target; an aggregate can be arranged without being treated as a writable Record. The first matching child entering opens one visit and the last leaving closes it, so another copy entering does not repeat the Action. Deliberately leaving and returning starts a new visit.

Immunity follows the originating Protein area, not a stranger's Sand entering the same rectangle. It leaves manual editing available and grants no data permissions. An optional gentle centering force can bring unattended Sands toward a recoverable region. Matching forces combine on attached children without tearing the group apart.

Avian 3D handles contact and settling. Lince owns Area selection and effects. Terrain uses editable stamps for shapes, ridges, smoothing, flattening, height and falloff. Effects can be fixed or attached to a Sand, filtered by data and styled independently of their strength. Show which terrain applies to a selected result; a visual explanation must not apply the force a second time.

Top and perspective cameras show the same surface positions. Free-space mode allows 3D placement with a nonphysical reference plane. Converting between surface and space previews overlaps, preserves identity and can be undone. Each workspace has one active simulation. A Sand keeps its authored facing unless Face viewer is enabled; this changes the reading face without changing its collider or forces.

On the surface, height comes from the terrain that applies to the Sand. In free space there is no hidden terrain floor or force. Expanding starts from that surface height; collapsing keeps the horizontal coordinates and projects Area volumes to their footprints. Undo retains the original positions. Face viewer is calculated for each person's camera, and clicks follow the displayed face while preserving editor focus.

Protein results can arrive together or in batches, appear directly, travel from a spawn point or settle before being revealed. A complete group enters before physics acts on it. Show when settling reaches its work limit. Off-camera behavior keeps the same meaning.

## Saving and sharing

Keep live state in memory, a readable snapshot on disk and a journal of complete operations for undo and recovery. Choose a file format explicitly; using a `.lingua` extension would require the actual Lingua grammar. Interrupted writes preserve the last valid state. Expose save frequency, pending work and the last durable revision.

Provide a checker, formatter and structural comparison for the chosen snapshot format. Offline file edits are validated as a complete replacement without overwriting a rejected source. While Lince runs, other programs use its checked operations instead of editing the snapshot underneath it. Local saving, snapshot compaction, motion checkpoints and delivery to another device have separate rates; slowing delivery must not weaken local recovery.

Save definitions, placements, overrides, drawings, connections and Areas. Rebuild runtime entities, graphics buffers and Area membership. Save motion at useful checkpoints, with an explained maximum recovery gap, rather than every frame. Ordinary work reopens at rest; continued simulation is an explicit policy.

Separate shared composition from personal camera, selection and panels. Live sharing has one host that checks, orders and saves edits; guests see confirmation or refusal. Presence and movement previews are temporary. A cached workspace is read-only while its host is unavailable. Offline editing and replacing the host remain separate work.

A joining guest receives a verified snapshot and subsequent ordered changes. Stable operation identities prevent duplicate application; old revisions, missing assets, interrupted transfers and access loss have distinct recovery states. A deliberate fork creates a different workspace. Sharing a reusable definition needs its own authority because other instances may exist outside this workspace.

The proposed tasks are gathered in [the interface draft](plans/interface.md).
