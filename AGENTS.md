Instead of cargo build use cargo check. Warnings are treated as errors. Do not use worktrees.

# Architecture

The crates/ directory contains the following crates:
Lince: binary, the rest are libraries.
Domain: Structs and Traits.
Persistence: Database connection and Impl of Domain Repository Traits.
GUI: GPUI code.
TUI: Ratatui.
Utils: System-wide utilities.
Injection: Dependency Injection.
Application: Main application logic.
Web: Web HTML version, pure JS, homebrew solutions as much as possible.

In this repository the schema is owned by Rust structs in persistence, and the repo-root `migrations/` directory is embedded into the binary at compile time via `sqlx::migrate!`. DO NOT ALTER A PAST MIGRATION UNLESS ASKED TO.

# Sand widgets

When constructing a sand widget that vendors code or assets with a license that requires a copy of the license, keep the required license and credit files alongside the sand package and bundle them with the widget assets. Do not ship the vendored asset alone if its license expects the notice to travel with it.
When constructing or substantially refactoring a sand widget, split large `body` and `script` implementations into their own directories with multiple focused files instead of letting a single monolithic file keep growing. Apply that structure to new sands and to sands you are already touching when the work is large enough to justify it.

# Business Rules

Due to the Organ Sync feature of making two Lince nodes being synced and the file sync feature of making records being synced with disk we are able to have the Lince Institute (the tasks and business rules) in this repo as markdown on `/notes/institute`. Do not edit it, use it for consultation only. The user may tell you to edit markdown files in `docs/`, that is free read/write on your part so make sure that when planning a feature, before implementing you check for a file related to that feature, add the implementation architecture, details, code examples or checkboxes in the file so if you dont implement the entire plan in one go we can pick up later.
