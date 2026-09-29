# IDE Castle

Build a simple, fast text editor Castle with a reusable File Explorer Castle docked on its left. Take inspiration from Zed's restrained layout and from Zed and Helix's text and file handling. The current scope is local Rust filesystem operations: list directories, read, create, write, rename, move, delete and restore files. Use Loro to retain edits while reconciling changes made by another program. This service is separate from normal Record sync and blob transfers. Opening a directory creates no File Records and copies no contents into the database.

The editor treats supported files as plain text. Highlighting, completion, language servers, linting and formatting are a later layer; the tool choices below are recommendations, not working integrations. Opening a source file does not download or execute anything. Keep the local editor useful without those tools.

## Basic controls and Castle settings

Remember each tab's selection and scroll position, including across workspace restarts. Share the buffer across Castles, with separate view positions. Tab inserts spaces to the next four-column stop; Tab and Shift+Tab indent or outdent selected lines as one undo action. The Explorer width is adjustable by dragging its divider.

Use Ctrl+O to open a file, Ctrl+S to save, Ctrl+Shift+S for Save as, Ctrl+W to close a tab, Ctrl+Tab or Ctrl+Shift+Tab to switch tabs, Ctrl+F for Find, Ctrl+H for Replace, Ctrl+G for a line number, and F3 or Shift+F3 for the next or previous match. Escape returns focus to the editor. Closing the last view of changed text offers Save and close, Discard changes or Cancel. Closing another view retains the shared buffer.

Settings belong to each IDE Castle and are saved with its workspace:

| Setting | Default and behavior |
| --- | --- |
| Find panel | Hidden until requested; literal text with next, previous, replace and replace all |
| Match case / whole word | Case matching on, whole word off; Unicode-aware matching |
| Project search | Off; when enabled, search saved files in the selected roots on request. Ctrl+Shift+F opens its controls. Unsaved text remains searchable with Find in its buffer |
| Autosave | Off; choose two or five seconds after the latest change. Use the same guarded save operation as manual Save |
| Explorer width | 270 pixels, adjustable within a bounded range |

Autosave pauses for input composition, pending file access, missing files, read-only previews, errors and unresolved conflicts. One shared file gets one write even when open in several Castles. A failed automatic save stays visibly unsaved; it must not create a repeated background write loop. The delay wakes the interface only while work is pending. Formatting on save is a separate future option, never implied by autosave.

Find scans bounded pieces of a rope snapshot on the file worker. Reject a result if the document, query or selection changed while it was running. Replace all is one undo action and respects the editing size limit. Project search streams lines without a permanent index, respects the Explorer's ignored-file setting, skips binary files, and shows when limits make results partial. Bound the search to 200 matching lines, 100,000 visited entries, 64 MiB read and ten seconds; cap each file at 16 MiB and each line at 64 KiB. Render only the visible result rows.

Files above the 16 MiB editing limit and unsupported text get a read-only view of at most the first 64 KiB, or an explanation for binary data. Saving, replacing and pasting must never write that preview over the original. CJK fonts load when needed and are shared; language-aware Chinese/Japanese word navigation still needs work in the text engine.

## First task: editor and explorer together

Allow one or more directory roots. Each root is a collapsible row with its name and a way to distinguish equal names at different paths. Expanded roots show folders and files indented below them. Nested folders expand in the same way. Keep folders first and use a stable name order. Removing a root from the view does not delete it.

Use a narrow, resizable Explorer tab on the left, a small tab strip above the editor, and a quiet status line for the path, cursor position and save state. Provide Add folder, open, new text file, Save, Save as, close, find and replace, undo and redo. Ordinary selection, clipboard shortcuts, keyboard navigation, scrolling, indentation and input methods must work. Begin with one editing pane; split views can follow once the shared buffer is reliable.

Clicking a file opens its buffer. Opening it again focuses or reuses that buffer instead of loading another copy. Keep cursor and scroll position per view, with shared content and undo. Show a small unsaved marker. Closing the last view of unsaved work offers Save, Discard or Cancel. Removing an Explorer root must not discard an open buffer.

Start with UTF-8, including files with a UTF-8 byte-order mark. Preserve line endings, mixed existing endings, final newline and file permissions. An unchanged save should not rewrite the file. Unsupported encodings and binary files get an explanatory read-only view. They remain selectable in the Explorer for other consumers, including model import.

## A reusable File Explorer Castle

Build the Explorer as its own Castle and reusable interface component from the start. The first task includes the docked and standalone tree views. Grid and input-picker views are the next task on that same component. Share directory listings, file identities and filesystem subscriptions across these views; keep selection, expansion and layout state local to each view.

| View | Behavior |
| --- | --- |
| IDE sidebar | Collapsible directory roots, indented folders and files; activating a file opens it in the editor |
| Standalone list | The same tree, with keyboard navigation, directory controls and path search |
| Icon grid | The current directory's children as folders and file-type icons, with breadcrumbs and Back/Up navigation |
| Picker | List or grid with the caller's file/folder filters; selection returns a path to the requesting input |

Use ordinary icons first. File or model thumbnails are later work and must be loaded only for visible items with a bounded cache. Switching list and grid preserves the directory, query and selection.

A button beside a path input opens the Explorer with a unique request ID, starting directory, allowed roots, file or directory mode, extension filters and single or multiple selection. In file mode, activating an allowed file returns its path and closes that picker. A folder chevron expands or collapses; entering a folder navigates. Directory selection has an explicit Select folder action so browsing does not accidentally finish it. Save as accepts a new filename and checks an existing destination before replacing it.

Return a typed path and request ID through the interface event system. Apply it only to the still-existing input that requested it. If that input changed while the picker was open, show the result for deliberate application instead of overwriting newer input. Cancel leaves the input unchanged. Multiple open pickers cannot send results to one another's fields.

The first reuse outside the IDE is the existing [model import path](../../crates/desktop/src/topology/ui.rs): put a browse button beside it, allow `.glb`, `.gltf` and `.gcloud`, and fill the path when selected. Import remains the existing explicit action. A path selection itself creates no Record, copies no asset and changes no import rules.

Project browsing respects `.gitignore` by default, with a visible Show ignored option. A general file picker must also let the user reach ignored and hidden files deliberately; project filters must not make a known asset impossible to select. Display filtering is separate from permission to access a directory.

## What to take from Zed and Helix

Zed uses ropes and summary trees to avoid repeatedly copying whole strings and to locate text efficiently. It moves costly file and search work off the UI thread and returns bounded batches. Use those principles within Lince's Rust interface. [Zed's text structures](https://zed.dev/blog/zed-decoded-rope-sumtree), [background work](https://zed.dev/blog/zed-decoded-async-rust)

Zed's project panel supplies the familiar expanding tree. Its scan settings also distinguish eager indexing from directories loaded on demand. For this first editor, go further toward low idle cost: load expanded directories and run deeper searches only when requested. Clearly label a search while more directories are still being searched. [Project panel](https://zed.dev/docs/project-panel), [directory scanning](https://zed.dev/docs/reference/all-settings#file-scan-depth)

Helix keeps rope text separate from per-view selections and applies a calculated text difference during reload. Zed checks buffer versions when applying a disk reload. Reuse the ideas of shared documents, small edits and rejecting stale background results. Helix's explicit reload can discard local changes, so it is not the policy for Lince's automatic refresh. [Helix document handling](https://github.com/helix-editor/helix/blob/master/helix-view/src/document.rs), [reload command](https://docs.helix-editor.com/commands.html), [Zed reload implementation](https://github.com/zed-industries/zed/blob/main/crates/language/src/buffer.rs)

Keep conventional text editing controls in the first Castle. Helix-style modal editing and multiple selections can be added independently later. Reuse Lince's interface and input support; embedding either complete editor would bring much more machinery than this task needs.

## Directory reading, searching and watching

Use native Rust filesystem APIs on bounded worker threads. Read directory entries directly, keep paths as `PathBuf`/`OsString`, and use buffered file reads and writes. Do not launch `ls`, `sed` or a shell for editor operations. Direct APIs avoid a process for each operation and preserve filenames containing spaces, newlines, shell characters or non-UTF-8 bytes. Display escaping must never change the path used for file operations.

Enumerate a directory when it is first expanded. Publish entries in batches and retain selection by identity while ordering settles. Read child names and types without recursively reading their contents, sizes or previews. Cache loaded listings, invalidate affected directories, and discard stale results when the user changes roots or closes a view. Draw only visible rows or grid cells plus a small margin.

Path search walks selected roots in the background for the current query, including unopened folders. Limit returned matches and cancel obsolete queries. Keep a bounded name cache, not file contents in a database. Optional project text search also runs only when requested. Directory reads publish batches of up to 256 entries; the interface retains stable paths while the sorted listing fills in.

Use OS file notifications through a Rust library such as `notify`. Watch loaded directories and the parents of open files, with shared subscriptions. Watching the parent is needed because editors often replace a file by renaming another file over it. Avoid installing a recursive watcher over every unopened directory just to display a root. Revalidate a collapsed directory when it is expanded again. [Watcher behavior and limits](https://docs.rs/notify/latest/notify/)

Start watching before completing an initial listing or file read, then reconcile any events received during that read. Treat notifications as hints to inspect current state. Combine bursts into one update per affected path, with a short bounded delay. Check fingerprints when reading changed files; timestamps alone cannot establish equality. Keep event queues bounded; overflow schedules a scoped rescan instead of silently losing updates.

Keep watches for open files even when their tree row is collapsed. Release unused subscriptions and cancel abandoned scans. On focus return or manual Refresh, reconcile open files and visible directories. When native events are unavailable, show a polling state and use a bounded interval for those paths. Avoid a continuous whole-project scan. A disconnected or unreadable root is unavailable, not evidence that every file was deleted.

## Text storage, drawing and undo

Use one authoritative Loro text document per open file. Lince already uses Loro and undo in [Record editing](../../crates/desktop/src/record_binding.rs). Reuse its integration knowledge while keeping this file editor independent of Record persistence. The current Record text path compares whole strings; extending that path to large source files would add work on every edit.

Send insertions and deletions with their ranges directly into Loro. Maintain one Ropey projection per document for line lookup, scrolling and drawing, updated from Loro's text deltas. That projection is a cache, not a second editable authority. All views read it; all edits go through Loro. Verify their equality in randomized tests, without adding a whole-text comparison to each production edit. Initial file loading and construction of these structures also run off the UI thread.

Draw and shape visible text with a small margin, cache line layout, and invalidate only affected ranges. Avoid a giant text widget that clones, measures or lays out the whole file on each keypress. Keep wrapping off initially and offer horizontal scrolling. Extremely long lines also need bounded layout work; merely drawing a few lines is insufficient if one line is several megabytes.

Keep cursor and selection anchors stable through edits. Distinguish UTF-8 offsets, character positions and user-visible grapheme boundaries. Support composed characters, emoji, bidirectional text, IME composition and clipboard operations without splitting text incorrectly. Reuse existing shaping and input facilities.

Group a typing burst into a local undo action. Import disk changes under a distinct origin or replica so Undo removes the user's own edits, not an external save. Restore selections through Loro cursor anchors. Exclude initial loading from undo. Loro provides local undo and cursor support for concurrent changes. [Loro undo](https://www.loro.dev/docs/advanced/undo)

## Bringing disk changes into Loro

A normal file stores bytes, not Loro operations. Loro can merge related operation histories, but it cannot infer every writer's intent from a replacement file. The required outcome is automatic merging of independent edits and visible preservation of competing edits. Do not promise that using a CRDT alone makes every external save conflict-free.

Keep the last observed disk bytes and fingerprint, the corresponding Loro version, and the working document with its unsaved edits. Maintain a disk-side Loro replica from the same history, with its own peer identity. It represents observed disk contents and must not include unsaved typing merely because the working document does.

When the watcher reports a change:

1. Read the changed file in a worker, checking for replacement or changes during the read. Retry unstable reads within a bound. Keep the current editor usable if reading fails.
2. Compare the new bytes with the disk-side version. Calculate changed ranges against that version, never against the current unsaved working text.
3. Calculate differences by line first, refining changed regions when useful, with a work limit. Commit complete external edits to the disk-side replica for the current disk generation. Compare their affected ranges with unsaved local edits before importing the candidate into the working document. A timeout keeps the candidate pending for review; it never causes a partial update or a full-buffer replacement.
4. With a clean buffer, refresh automatically and remain saved. With independent local edits, preserve both sets of changes and remain unsaved until the merged content is written.
5. With overlapping replacements, deletions or ambiguous insertions, retain the disk candidate and local version for a conflict view. Hold the candidate's whole operation stream rather than importing selected fragments of its history. Offer local, disk or manually combined text, and block normal Save until resolved. Resolution incorporates the disk history and the chosen result as an explicit Loro transaction, preserving newer typing and preventing rejected text from reappearing on the next disk event.

For example, editing the first paragraph locally while another program edits the last should preserve both. Replacing the same sentence in both places requires review. Repeated disk events must advance the disk replica in order without importing an edit twice. If newer typing arrives during a background calculation, transform the result through the shared history or recompute; do not apply stale offsets.

After a successful save, advance the disk replica to the exact version written. Typing that happened while saving remains unsaved. Match self-generated watcher events to the confirmed contents, not a period during which all events are ignored. Deletion or an uncertain rename keeps the buffer and shows a missing-file state; it must not silently recreate the old path.

Loro's `fork()` copies a document in linear time and space. Create and reuse disk replicas deliberately rather than forking on every keypress or watcher event. Bound history growth, retain versions needed by undo, reconciliation and in-flight saves, and release clean closed documents. Loro's whole-text diff can also be costly; use bounded line-based reconciliation and never a whole-text update of the unsaved working buffer. [Loro document API](https://docs.rs/loro/1.16.0/loro/struct.LoroDoc.html#method.fork), [Loro text API](https://docs.rs/loro/1.16.0/loro/struct.LoroText.html#method.update_by_line)

## Saving and keeping work

Save a fixed buffer version through one writer per file, shared by Lince views. Re-read and compare the destination against the known disk version before writing; reconcile an outside change before trying again. Write a temporary file in the same directory, preserve applicable metadata, flush it and replace the destination atomically where supported. Do not mark newer edits saved when an older write completes. A write error leaves the buffer intact and visibly unsaved.

Atomic replacement prevents a partially written file, but it does not prevent another program from writing between the comparison and replacement. Locks can coordinate Lince writers; unrelated programs may ignore them. Preserve observed competing versions and check the resulting file, but state the limit honestly: no editor can recover an intermediate external version it never observed. Stronger guarantees need cooperating writers or filesystem version history. Manual Save is the default; optional autosave uses the same checks.

Keep one binding for a known file across overlapping roots. Do not rely only on an inode, which can change during atomic saves. Resolve symlinks within the granted roots without replacing the link accidentally, avoid directory cycles, and reject path escapes and special files. Hard links and unsupported replacement semantics need a clear limitation instead of silently changing their meaning. Save as must also reconcile an existing destination.

Keep active Loro state and undo in memory without creating database Records. Offer a visible Restore unsaved work option backed by a bounded local recovery journal outside the project. Explain that this stores recovery copies, give it a size limit and cleanup policy, and never send it through peer sync. Without that option, closing unsaved work still asks and a crash may lose unsaved edits. Recovery must compare its saved disk fingerprint with the current file before restoring.

## Performance targets and checks

These are initial targets to measure on named reference hardware, not claims about existing performance. Record warm and cold runs, file counts, text sizes, peak memory and CPU usage. Use a static Lince scene to isolate editor overhead from unrelated animation.

| Scenario | Target or required behavior |
| --- | --- |
| Idle after loading | No periodic scans or full-buffer comparisons; less than 0.5% of one CPU core over 60 seconds with native watches and caret blinking disabled |
| Typing in a 10 MiB UTF-8 file | 95th percentile input-to-visible-update below 32 ms; no whole-document copy or layout per keypress |
| A root with 100,000 files | Root display does not wait for recursive enumeration; only visible rows are rendered |
| A directory with many immediate children | First entry batch within 150 ms on a warm local SSD; loading remains cancellable |
| Search and watcher bursts | Typing keeps its latency target; stale work is cancelled and queues stay within fixed limits |
| Large files or very long lines | Opening is cancellable; above the tested editing limit, offer a bounded read-only preview instead of allocating an unbounded CRDT |

Use Lince's existing [reactive window settings](../../crates/desktop/src/theme.rs) and [WakeSignal](../../crates/interface/src/wake.rs). Wake for input, completed work, relevant file events and the focused caret timer. An unfocused or hidden editor must not keep rendering. Background jobs must have bounded concurrency; moving unlimited work off the UI thread still wastes CPU.

Measure the full Loro history, disk replica, Ropey projection and layout caches together. Do not describe rope snapshots as proof that Loro forks are cheap. Set editing, undo and recovery limits from those measurements, and avoid compacting away history still needed for a disk merge. Test repeated opening and closing for leaked tasks, subscriptions and buffers.

The current unoptimized 10 MiB buffer probe shows fast ordinary edits but occasional large CRDT costs, including roughly 200 ms to prepare a save after many edits. Save text is now materialized on the worker and saved snapshots share their text, but CRDT export still happens before the worker. Measure and reduce that remaining cost before claiming the large-file latency target is met. The buffer probe does not measure rendered input latency or idle CPU.

Correctness tests must cover shared buffers across views, cursor and undo preservation, Unicode and IME input, unchanged-byte saves, typing during save, disjoint and overlapping disk edits, repeated events, atomic external saves, delete/recreate, rename, missed events and recovery after interruption. Verify that disk changes do not disappear when the user undoes their own typing.

Explorer tests must cover nested and overlapping roots, duplicate names, large directories, ignored files, binary selection, permissions, symlinks, unusual filenames, cancelled scans and root removal. Picker tests must show a selected model path reaching the correct input, with no import on selection and no mutation after cancellation or a stale request. Include file-boundary and save-race tests as part of the first task.

## Work after the first editor

1. Keep testing ordinary editing, filesystem operations, crash recovery, large directories and long sessions. Optimize measured costs before adding more UI.
2. Add incremental highlighting independently of external language tools. Keep parsers and queries lazy, with visible-range highlighting and bounded background work.
3. Connect installed language tools using explicit executable and environment settings. Add a small Language Tools Castle for status and configuration before considering downloads.
4. Consider split views and Record-backed file views only when wanted. The local filesystem service remains independently usable. A database/storage selector is not part of the current CRUD work.

## Earlier decisions retained for later

[Tasks.lingua](Tasks.lingua) describes General File Sync and language-server-derived links. Current [File Sync](../../crates/engine/src/file_sync.rs) supports Markdown and Lingua, and current [Record kinds](../../crates/nucleus/src/record.rs) do not include File. Do not force those schema changes into the first editor.

When connecting the editor to Records, recommend one File kind with a stable UID, root and relative path. Its title is the filename; its description is the actual contents without an added wrapper. Language is separate from kind. Plain Records can contain prose and code sections. A Command Record's entire description is Bash without a surrounding code fence. A Bash File remains a File and does not run when opened.

| Future storage choice | Contents and saving |
| --- | --- |
| Files | Contents remain in real files; Protein reads them on demand through a small local index |
| Database | Text is stored in Record bodies and binary content through blob references; no bound working directory is created |
| Files and database | Both copies are maintained, with explicit pending writes and conflicts |

Keep storage, save timing and peer sharing separate. Files mode must not quietly copy text into Record history, persistent content indexes or peer replication. Changing storage compares existing copies before importing or exporting. Turning a destination off does not delete its existing contents. Protein must carry the source and allowed operations so all Castles write through the correct backend. Areas select and display files; entering or leaving an Area does not delete or copy them.

Later language support can cover Nix, TOML, Lingua, Scheme, Bash and Rust, with a chosen Scheme dialect and Lingua's existing parser. Keep highlighting, formatting and language servers separate. Preserve nested fences and heredocs, quote and tab-stripping behavior, project context, Unicode position mapping and undo. Reject stale edits and edits outside a section; formatting remains explicit. Database-only projects may need a deliberately created working directory for full language support. Derived code links must retain provenance, show partial results and preserve manual links.

## Connecting installed language tools later

Recommend installed tools first. Store a program, argument list, working directory, environment overrides and language/project association. Resolve an explicit executable before PATH. Show the resolved path, version, running state and errors; offer Restart and Disable. Do not copy an externally installed binary into a Lince directory or overwrite it. A missing tool should offer Configure, Installation guidance and Not now. Keep refusals. The editor remains usable without it.

| Environment | Recommended connection | Alternative and tradeoff |
| --- | --- | --- |
| NixOS system | Declare tools in `environment.systemPackages`, then connect by executable name | Home Manager's `home.packages` keeps them per user. Both reuse packages managed outside Lince |
| A Nix project on any distro | Put tools in the project's development shell and launch Lince from that environment | Configure a server wrapper such as program `nix`, arguments `["develop", "/path/to/project", "--command", "rust-analyzer"]`. Starting the shell may evaluate code, fetch dependencies and run hooks, so selecting that environment must be explicit |
| Nix without changing configuration | An explicitly managed user profile exposes installed tools through PATH | `nix shell nixpkgs#nixd --command nixd` can try a server temporarily, but may download on startup. Prefer installed tools for predictable startup |
| Other Linux distributions | Install the tool with the distro package manager and connect through PATH | Use an upstream release in a user-owned directory and select its executable, or use the language's package manager. Package names and available versions depend on the distro |
| Per-project tools | Select the already installed executable inside the project's environment | For example, a local Node package's `node_modules/.bin` entry. Do not use a command that silently installs a missing package when starting the server |

NixOS declarative packages and Nix/Home Manager PATH choices are described in the [NixOS manual](https://nixos.org/manual/nixos/stable/#sec-declarative-package-mgmt) and [NixOS PATH guide](https://wiki.nixos.org/wiki/Adding_programs_to_PATH). Project wrappers use [nix develop --command](https://nix.dev/manual/nix/2.24/command-ref/new-cli/nix3-develop). On a graphical launch, use the environment actually inherited by Lince; a newly installed executable might need a new session, an explicit path or an environment override. Do not guess a user's shell configuration by sourcing arbitrary startup scripts.

For example, a NixOS or Home Manager package list can select only the languages used:

```nix
with pkgs; [
  nixd nixfmt statix deadnix
  rust-analyzer rustfmt clippy
  bash-language-server shellcheck shfmt
  taplo
]
```

Put that list in `environment.systemPackages`, `home.packages`, or a development shell's `packages`, as appropriate. A Rust project still needs a matching Rust compiler and Cargo toolchain. Prefer the project's pinned environment when tool versions matter. Nix chooses a cached build when available and may build otherwise; Lince should not maintain a second Nix installer.

| Language | Server recommendation | Formatting and linting |
| --- | --- | --- |
| Nix | `nixd` for Nixpkgs and NixOS/Home Manager option support; `nil` is an alternative. Select one | `nixfmt` for formatting; optional `statix` and `deadnix` checks on request or save, not each keystroke. [nixd](https://github.com/nix-community/nixd), [nil](https://github.com/oxalica/nil), [nixfmt](https://github.com/NixOS/nixfmt), [statix](https://github.com/oppiliappan/statix), [deadnix](https://github.com/astro/deadnix) |
| Rust | `rust-analyzer`, using the project's toolchain and root | `rustfmt` and optional Clippy through rust-analyzer or one explicit Cargo check action. With rustup, add the matching `rust-analyzer`, `rust-src`, `rustfmt` and `clippy` components. Avoid duplicate background Cargo jobs. [rust-analyzer installation](https://rust-analyzer.github.io/book/installation.html) |
| Bash | Program `bash-language-server`, arguments `["start"]` | Install `shellcheck` and `shfmt`; the server can use them for diagnostics and formatting. A distro package or an existing npm installation is suitable. [Bash Language Server](https://github.com/bash-lsp/bash-language-server) |
| TOML | Program `taplo`, arguments `["lsp", "stdio"]`, with an LSP-enabled build | Reuse Taplo's formatting and validation. Prefer a distro/Nix package or upstream binary; a Cargo installation builds from source. [Taplo server](https://taplo.tamasfe.dev/cli/usage/language-server.html), [formatting](https://taplo.tamasfe.dev/cli/usage/formatting.html) |
| Lingua | Reuse Lince's parser and validation first | Do not invent an external server requirement. Add formatting only with a defined, tested format |
| Scheme | Choose the dialect before choosing a server | Keep plain editing and highlighting available. Guile, Chicken and other dialects should not silently share an incompatible server or formatter |

Keep highlighting separate from language servers: [Tree-sitter](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html) supplies grammar-based highlighting without a running LSP. A Command Record's whole description is Bash; a normal description can contain fenced sections; a file uses its own language. Formatting and diagnostics may come from a server or a standalone tool. Configure one provider for each job instead of running duplicate processes.

Use one server per language, project root and chosen environment, shared by all views of that project. Start it when needed, debounce updates, cancel stale work and stop it when unused. Use standard input/output with direct Rust process spawning and argument arrays. Bound diagnostics and logs. Apply formatter and code-action edits only to the document version they were requested for, as one Loro undo action. Keep formatting manual initially. Never make autosave wait indefinitely for a tool or silently enable builds, package downloads, project hooks or formatter fixes.

A managed downloader can be considered later if configuring installed tools proves insufficient. Mason's registry can inform a small tested catalog, but supporting every package manager or embedding Neovim is outside this scope. Any future installation must distinguish a downloaded executable, a package-manager operation and compilation from source. [Mason](https://github.com/mason-org/mason.nvim)

Use `interface` for reusable editor and Explorer models and controls, `desktop` for Castle composition and path-input integration, and shared Rust services for filesystem work and reconciliation. Include LICENSE and credits for embedded dependencies in every Sand that uses them. Leave the disconnected web crate out of this work.
