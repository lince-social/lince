== Tech Stack

*Backend*: \
- Programming Language: #link("https://www.rust-lang.org")[Rust]
- Server: #link("https://github.com/tokio-rs/axum")[Axum]
- Database: #link("https://www.sqlite.org/")[Sqlite] | Easiest to replicate the concept of DNA. But way to simple...
- Database Driver: #link("https://github.com/launchbadge/sqlx")[SQLx] | Fun async, previously was Rusqlite but didn't prototype that well.

*Frontend*: \
- Web:
  - Base: HTML, CSS
  - Templating (Backend): #link("https://github.com/lambda-fairy/maud")[Maud]
  - Framework: #link("https://github.com/bigskysoftware/htmx")[HTMX] + #link("https://github.com/starfederation/datastar")[Datastar] (transitioning into)
- GPUI:
  - Experimenting

*Dirty Architecture*: \
- Application
  - Services for anything complex, business ruly, that calls repositories or manipulates data.
- Domain:
  - Entities:
    - Clean: the most accurate representation of the main drivers of the application. The source of truth of Lince's concepts.
    - Dirty: structure of data that comes in or goes out through the endpoints, or an alias for a data structure as a Type so it doesnt clutter the screen.
- Infrastructure:
  - Http:
    - Routers: the paths and arguments of endpoints.
    - Handlers: the functions that are called by endpoints, that receive the query params, path variables, header, body if wanted.
    Responsible for calling the services and/or returning presentation functions (html frontend).
  - Database:
    - Management: database connection, migrations and schema.
    - Repositories: traits and their implementations for database operations like get, update...
  - Utils: functions for quality of life, like logging.
  - Cross_cutting: dependency injection for a collection of many methods in different layers. Mostly for single database connection initiation.
- Presentation
  - Html: the templates made with maud, or sometimes just big strings. that are returned from functions.
    These HTML may contain HTMX and/or Datastar (in the process of removing HTMX).
    The directories are divided rudimentary by concepts/entities like 'operation', 'table', 'pages'.

  - GPUI: for a featureful and immersive GUI, the future for the frontend, currently in implementation.

*(Some Rust) Learning Resources*
1. #link("https://doc.rust-lang.org/stable/book")[Rust Book]: chapters 1–6 will give you a simple base if you just want to hack something together. Passing through the entire book, doing the examples and understanding them deeply, will set you up for great vibe coding.
2. #link("https://rustlings.rust-lang.org/")[Rustlings]: for when you are done with the book. You will complete exercises on a broad spectrum of the language's features.
3. #link("https://rust-for-rustaceans.com/")[Rust for Rustaceans]: for a deeper dive in the language, good practices and crate recomendations.
4. #link("https://this-week-in-rust.org/")[This Week in Rust]: a weekly newsletter about Rust projects, crates and language changes. Amazing for keeping up to date with the ecosystem, very useful.
5. Youtube Channels:
  #link("https://www.youtube.com/@jonhoo/videos")[Jon Gjengset] |
  #link("https://www.youtube.com/@NoBoilerplate")[No Boilerplate] |
  #link("https://www.youtube.com/@TheVimeagen")[The Rustagen] |
  #link("https://www.youtube.com/@TsodingDaily")[Tsoding] |
  #link("https://www.youtube.com/@codetothemoon")[Code to the Moon] |
  #link("https://www.youtube.com/@fasterthanlime")[fasterthanlime] |
  #link("https://www.youtube.com/@chrisbiscardi/featured")[chris biscardi] |
  #link("https://www.youtube.com/@mikecode-ns7tq/videos")[Mike Code] |
  #link("https://www.youtube.com/@DeveloperVoices")[Developer Voices]

6. Podcast: #link("https://rustacean-station.org/")[Rustacean Station]
7. Discord, Matrix and many more online communities are very welcoming to newbies.
