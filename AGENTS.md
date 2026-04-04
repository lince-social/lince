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

# Documentation

In /documentation/technical/chapters/ look at aging/ for theory on Lince, look at sickness for Web focused implementation subjects. When you are asked to create a document its always a .typ file that go in one of those two (probably sickness/); Dont forget to reference them inside the chapter.
The documentation contains information about current implementations but also ideas from other past implementations of Lince, so take it with a grain of salt. The current code is supposed to be a more up to date reference to the functioning of the app, but might not be the desired behavior, so consider it higher than the documentation for validation of business rules, but not as perfect.
