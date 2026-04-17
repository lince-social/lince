Instead of cargo build use cargo check. Warnings are treated as errors.

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
Web: Web HTML version.

In root of project there is the SQLite migrations/ directory.

# Sand widgets

When constructing a sand widget that vendors code or assets with a license that requires a copy of the license, keep the required license and credit files alongside the sand package and bundle them with the widget assets. Do not ship the vendored asset alone if its license expects the notice to travel with it.
When constructing or substantially refactoring a sand widget, split large `body` and `script` implementations into their own directories with multiple focused files instead of letting a single monolithic file keep growing. Apply that structure to new sands and to sands you are already touching when the work is large enough to justify it.
