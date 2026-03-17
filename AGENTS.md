Instead of cargo build use cargo check.

If you need more information about certain topics look at the documentation in /documentation/chapters/aging. It contains information about current implementations but also ideas from other past implementations of Lince, so take it with a grain of salt. The current code is supposed to be a more up to date reference to the functioning of the app, but might not be the desired behavior, so consider it higher than the documentation for validation of business rules, but not as perfect.

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

In root of project there is the SQLite migrations/ directory.
