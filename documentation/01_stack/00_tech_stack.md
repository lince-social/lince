# Tech Stack

Backend:

- Programming Language: [Rust](https://www.rust-lang.org)
- Server: [Axum](https://github.com/tokio-rs/axum)
- Database: [Sqlite](https://www.sqlite.org/) | Easiest to replicate the concept of DNA.
- Database Driver: [SQLx](https://github.com/launchbadge/sqlx) | Fun async, previously was Rusqlite but didn't prototype that well.

Frontend:

- Base: HTML, CSS
- Templating (Backend): [Maud](https://github.com/lambda-fairy/maud)
- Framework: [HTMX](https://github.com/bigskysoftware/htmx) + [Datastar](https://github.com/starfederation/datastar) (transitioning into)

Dirty Architecture:

- Application
  - Providers: simple CRUD operations.
  - Use_cases: anything complex, business ruly, that calls providers or manipulates data.
  - Schema: structure of data that comes in or goes out through the endpoints, or an alias for a data structure as a Type so it doesnt clutter the screen.
- Domain:
  - Entities: the most accurate representation of the main drivers of the application. The source of truth of Lince's concepts.
- Infrastructure:
  - Http:
    - Routers: the paths and arguments of endpoints.
    - Handlers: the functions that are called by endpoints, that receive the query params, path variables, header, body if wanted. Responsible for calling providers, use_cases and/or returning presentation functions (frontend).
- Presentation
  - Web: the templates made with maud, or sometimes just big strings. that are returned from functions. These HTML may contain HTMX and/or Datastar. The directories are divided rudimentary by concepts/entities like 'operation', 'table', 'pages'.
  - Tui: not implemented yet, but mainly for a non gui, terminal based version, with hyper keyboard centric workflows, designed for portability, Actions Per Minute (APM) and performance.
