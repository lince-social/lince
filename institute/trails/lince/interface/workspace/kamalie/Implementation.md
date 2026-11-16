Kamalie's initial Castle is implemented in source under [castles/sand/kamalie](../../../../../../crates/interface/src/native/castles/sand/kamalie/mod.rs). No build, execution, tests, Lingua checker, deployment or installation was performed: the owner explicitly requested coding only. Checked tasks in Standalone-castle.lingua denote source implementation; they do not assert compilation or playable acceptance.

The original Record was split before game code was written. Its durable UID remains on Kamalie.lingua. The architecture, wider capability map and later acceptance plan are preserved separately. The owner's later change from square to circle is reflected in the active game Records and the player mesh.

Implemented source:

- Native store registration, Castle creation, saved placement restoration and a static store thumbnail. The store entry is available without the optional Freedoom feature.
- A small white circle rendered with a Bevy mesh, flat moss-green walls/floor and a dark exit; no HUD. The camera renders native sprites into a nearest-sampled 408 × 296 GPU texture. The viewport preserves aspect ratio when resized.
- One run and input context per Castle, independent random streams, reserved camera layers, focus release, pause and restart. Hidden and unfocused simulations suspend. Visible scenes can still render on host frames so asynchronous GPU pipeline preparation can finish even before the first focus. A shared wake timer runs during active play and initial presentation, and is released when idle. Removal releases the scene hierarchy; asset handles follow its lifetime.
- Connected randomized corridors, broader passages, rooms, branches and loops. The exit is chosen from the farther reachable cells. Entering it generates a new level, discards the old dungeon and retains held objects with their existing parts and integrity. Restart creates an empty fresh build. Reopening the saved Castle starts a new run; run checkpoint persistence is deferred.
- Directional Z pickup and X release, multiple attachments, continuous mouse-directed stick swing and thrust while holding left click. Bushes are passive attachments. Both materials participate in local contact damage and fracture.
- Per-part integrity and reciprocal impact wear. Disconnected groups become fragments; a held fragment retains the grip only while connected. Central bush cuts scatter more weakly than edge cuts. Edge hits sever a small surviving chip, and fragments rotate about their own centers. The bounded debris fallback is described in Materials-and-fracture.lingua.
- A typed embedded JSON catalog parsed once into immutable shared memory, with stronger stick parts than bush parts. Matching custom Record extensions are authored in Stick.lingua and Bush.lingua. They are not yet imported into the runtime; the checked-in JSON snapshot is the current game input.

Source responsibilities:

| File | Responsibility |
| --- | --- |
| controls.rs | Executable keybind documentation; WASD/arrows, Z, X, F6 pause, F7 restart, Tab release, left-click activation |
| host.rs, input.rs, wake.rs | Castle integration, focus/input lifecycle, aspect ratio and session scheduling |
| render.rs | Bevy camera, GPU target, circle, sprite geometry and scene teardown ownership |
| dungeon.rs | Connected floor generation, reachability, collision and exit selection |
| run.rs | Current run, movement, attachments, procedural item placement and level transitions |
| objects.rs, fracture.rs | Object parts, local integrity, swept contact, wear and connected fragments |
| data.rs, materials.json | Definitions, constants, bounded generation values and random stream |

The gameplay modules depend on Bevy types and their own typed data, not Lince Records. They are grouped with the Castle for this first slice. Extracting a separate gameplay crate, enemy/death systems, broader generated weapon behavior, Record-extension import, run saves and area integration remain later work.

Source review included the host's local Bevy 0.19.1 APIs, reserved render layers, default Park shortcuts and ordinary Sand store/persistence paths. The applicable shared declarations found were edit-panel geometry and Protein readability/Laboratory rules; this change adds a store entry without changing their geometry or text behavior. No game-specific |> requirement existed. Runtime rule compliance, compiler/type checking, multi-instance isolation and performance measurements remain unverified. KAM.S11 and KAM.S38–S43 intentionally remain unchecked.
