Instead of cargo build use cargo check. Warnings are treated as errors.

If you need more information about certain topics look at the documentation in /documentation/technical/chapters/aging. It contains information about current implementations but also ideas from other past implementations of Lince, so take it with a grain of salt. The current code is supposed to be a more up to date reference to the functioning of the app, but might not be the desired behavior, so consider it higher than the documentation for validation of business rules, but not as perfect.

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
Web: Web HTML version. This uses widgets that have their documentation for creation in /documentation/AI/. It contains alplhabetically ordered markdowns to be inserted into the preprompt context of AIs that are used to create the widget and for models when changing the app's code to understand the process of changing the Web version.

In root of project there is the SQLite migrations/ directory.
