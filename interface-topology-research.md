Advanced Topology helper, refined September 13, 2026 from the current Advanced Topology tasks in Lince.lingua and the owner's follow-up decisions. This is architecture discussion, not an implementation report or a new task plan. Examples of particular objects do not define the feature's scope.

The owner has decided: Sands have three dimensions, with depth equal to their smaller planar side; top view is a view of the same 3D space; imports start with simple glTF; unpinning enables physics; normal spatial navigation supports flying with collision; groups preserve exact relative placement; Areas have adjustable depth. Blender conversion and undo are deferred. Undo belongs to the future history shared with workspace collaboration and Sync.

The Record now places .blend conversion under Future. Its broader future physics entry still exists; the owner's explicit instruction brings basic forces and collisions after unpinning into the present scope. That does not require the remaining future physics features. No .lingua edits were made.

1. Give Sands real volume in one coordinate system.

A rectangle with planar dimensions 300 × 100 has depth 100. Depth is derived from the smaller local side whenever layout dimensions change. Apply any overall scale once to the resulting volume. Imported glTF keeps its own geometry; do not replace an imported mesh with this rectangular Sand rule.

Recommend keeping the existing canvas size units, mapping the canvas plane to X/Z and using Y for depth. Use one fixed import conversion, initially 100 canvas units per glTF metre, consistent with the current physics length unit. Keep the conversion out of normal UI; expose ordinary asset resizing. Preserve double-precision saved positions and use the same origin conversion for rendering, picking and physics. This avoids an unnecessary rewrite of all canvas measurements. glTF's unit convention is documented in the [glTF specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html); the canvas mapping is a recommendation.

Recommend anchoring the content face at the Sand's local depth zero and extending its solid body behind that face. Resizing then changes thickness without moving the content face. The body's centre is offset by half the depth. Keep the same placement and dimensions when changing camera views. A Sand's local shape remains rectangular after rotation; do not derive depth from its rotated screen bounds.

2. Present existing content on the solid Sand.

Recommend drawing text, images and controls on the content face, with plain side and back faces. Side clicks select the Sand; clicks on the content face reach the existing controls. Keep content attached to the Sand's orientation. This retains a clear top view and makes the volume visible when flying around it.

The current [CanvasItem](crates/interface/src/canvas.rs) requires a UI Node and stores planar positions; changing the [camera](crates/interface/src/app.rs) alone cannot produce this behavior. Rendering the existing widget content onto a solid's face is the main remaining implementation uncertainty. Reuse layout, text editing, scrolling and focus through a presentation adapter. Avoid giving every glyph, border or decorative UI node a separate physics body. Attached Sands retain their own dimensions but share their group's motion. Measure the simplest working content renderer before choosing any large texture or rendering-cache system.

3. Import simple glTF through the existing loader.

Recommend core static meshes, node transforms, ordinary materials and textures. Use the default scene, falling back to the first scene, and reject an empty result. No character-pose, animation or special-extension work is required. Unsupported content should produce a concise import result. Enable the required Bevy import and material features rather than writing another loader. The pinned loader's capabilities are described in the [Bevy 0.19.1 source](https://raw.githubusercontent.com/bevyengine/bevy/v0.19.1/crates/bevy_gltf/src/lib.rs).

Each import instance is one logical Sand with its own saved identity, workspace and placement. Picking an internal mesh selects its owning Sand. Copies share loaded geometry and textures, while moving or pinning one copy affects only that instance.

Copy the source and required local resources into a managed asset directory and save a reference in workspace storage. Use an ordinary generated asset ID; content-addressed packages and automatic deduplication are not prerequisites. Bound resource loading and restrict dependency paths to the supplied import resources. Failed or cancelled imports must not leave a partially usable instance. Retain supplied credits and notices. This is local asset storage and Interface work; it does not require creating Records or adding a workspace Sync protocol.

4. Make world pinning control physical movement.

Imported assets start world-pinned. All spatial Sands can use the same control. A world-pinned object remains in the scene and participates in collisions, but cannot be displaced by forces or bumps. Direct editing still adjusts it. Unpinning creates or enables dynamic movement, including applicable Area forces and collisions with other objects.

Recommend zero gravity, existing damping, and locked automatic rotation initially. Objects can move along all three axes; manual rotation remains available. This provides spatial physics without adding falling, a ground plane or tumbling. Use explicit nonzero mass, initially the existing ordinary-Sand mass per independent Sand or asset. Sum member masses for attached groups; Areas themselves contribute no solid mass. Pinning clears velocity; unpinning starts at rest, with current forces applied normally. Preserve the existing hold while a Sand is being dragged or edited, including its attached group.

World pinning is separate from existing [screen pinning](crates/interface/src/sand_placement.rs) and from Area immunity. A spatial group cannot mix viewport anchoring with world-relative placement without an explicit conversion. Keep existing screen-pinned UI separate from spatial grouping initially. Any world-pinned member holds its whole attached group against automatic movement; unpinning one member does not release the group while another remains pinned.

5. Keep collision and flying navigation simple.

Use box colliders matching ordinary rectangular Sands' real dimensions. Use imported triangle geometry for glTF collision, preserving concavity. Selection bounds can be simpler than collision geometry. Keep a single instance body even when its imported scene has many mesh colliders.

Recommend beginning with prepared triangle meshes for both pinned and unpinned simple imports, with explicit mass and locked automatic rotation. Avian's example uses dynamic triangle-mesh bodies and notes that convex decomposition can improve robustness and speed at a preparation cost. Test this baseline before adding decomposition. If needed for moving imports, use several convex pieces that preserve the relevant empty space; never silently substitute one enclosing hull for the entire asset. The renderer and collider must follow the same transforms. [Avian mesh example](https://docs.rs/crate/avian3d/latest/source/examples/trimesh_shapes_3d.rs), [collider API and triangle-mesh caveats](https://docs.rs/avian3d/0.7.0/avian3d/collision/collider/struct.Collider.html).

Normal spatial mode supports free flight with a small collision shape and sliding; edit mode supplies noclip. Keep ordinary top-view panning and zooming. Separate flight input from text entry so typing does not move the viewpoint. On leaving edit mode, resolve overlap or restore a revalidated clear position; if neither succeeds, keep noclip active with a concise explanation. Walking, gravity and path finding are not required. Avian supplies [movement sweeps and sliding](https://docs.rs/avian3d/0.7.0/avian3d/character_controller/move_and_slide/struct.MoveAndSlide.html).

6. Give each attached group one transform and one physics body.

Save a group root and each member's local position and rotation. Rendering, picking, collision and influence derive world placement from those same transforms. Grouping and ungrouping preserve current world placement. Keep group membership separate from Protein ownership and the internal widget hierarchy. Group scaling is not required for this work.

Recommend one rigid body per attached group, carrying the members' colliders at fixed local offsets. Areas follow that body as influence volumes, without becoming solid obstacles. Forces on members contribute to the group body's movement. This preserves the exact relative arrangement and removes the need for separate bodies connected by solver joints. The current [physics integration](crates/interface/src/physics.rs) uses FixedJoint for grouped Sands; the current [selection code](crates/interface/src/canvas_selection.rs) excludes Areas. Both need to change. Avian supports [multiple colliders attached to one body](https://docs.rs/avian3d/0.7.0/avian3d/collision/collider/struct.Collider.html).

7. Extend Areas to volumes in that same space.

Give Areas an explicit positive depth. Recommend initializing it from the smaller planar side for consistent creation, then allowing independent adjustment. Existing square, circle and polygon footprints become boxes, cylinders and extruded polygons. Evaluate containment in local coordinates and transform force directions into world coordinates. Spawn positions, sorting axes and reach follow the group transform too. Unlimited reach remains workspace-wide.

Recommend using the Sand's placement point for Area membership initially, extending the existing point-based behavior into three dimensions. Full collider overlap would cause large Sands to enter several Areas earlier and adds complexity. The Area-depth control changes the actual influence volume, including effects and mutation entry/exit tracking.

Recommend that an Area does not drive the motion of its own attached group. Otherwise a force applied to a member also carries the source along, potentially creating endless self-driven motion. It may still affect other groups, and its nonmovement effects follow their existing rules. Preserve the current disarm-and-preview behavior when directly editing a mutation Area or its group's placement. Merely grouping without changing world placement must not cause false boundary crossings. See [Areas](crates/interface/src/area.rs), [effects](crates/interface/src/area_effects.rs) and [Record mutations](crates/interface/src/area_mutation.rs).

8. Keep placement, selection and persistence bounded.

Recommend creation and ordinary dragging on the current editing plane, preserving depth during a planar drag. Expose explicit movement along the third axis. For group selection, use a rectangular volume with visible adjustable depth, borrowing Area volume controls. Select member placement points inside that volume and include existing attached groups as units. A normal click selects the nearest hit. These rules avoid adding several selection modes before basic spatial grouping works.

Save instance IDs, asset references, transforms, groups, pin state and Area depth through the current workspace storage. Save the settled arrangement rather than treating transient physics velocity as authored state. Keep normal saving and reopening; defer interactive undo and edit history to the shared Sync history feature. Do not add a topology-only undo stack or a speculative history framework now.

The correctness checks should cover an extruded rectangle and its resize, unchanged geometry across camera views, editable content on its face, two independent glTF instances, pinned and unpinned forces and collisions, exact group-relative movement, adjustable Area depth and self-force exclusion, selection by volume, and save/reopen. Measure import preparation, moving simple meshes, repeated-instance memory and idle wakeups. Run appropriate tests and cargo check with warnings treated as errors when implementation is authorized. This refinement changes only this helper; no code was changed or build checks run.

Future retains .blend conversion, common Sync history and undo, workspace collaboration and review, automatic paths, additional physics behavior, source editing and replacement, Fiote proposals, Gaussian splatting and camera feeds. None is a prerequisite for the simple topology described here.
