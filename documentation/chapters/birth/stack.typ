== Tech Stack

*Backend*: \
- Programming Language: #link("https://www.rust-lang.org")[Rust]
- Database: #link("https://www.sqlite.org/")[Sqlite] | Easiest to replicate the concept of DNA. Way too simple, way too reproduceable.
- Database Driver: #link("https://github.com/launchbadge/sqlx")[SQLx] | Used for raw and typesetted queries

*Architecture*: \
- Application
  - Services for anything complex, business ruly, that calls repositories or manipulates data.
- Domain:
  - Entities:
    - Clean: the most accurate representation of the main drivers of the application. The source of truth of Lince's concepts.
    - Dirty: structure of data that comes in or goes out through the endpoints, or an alias for a data structure as a Type so it doesnt clutter the screen.
- Infrastructure:
  - Database:
    - Management: database connection, migrations and schema.
    - Repositories: traits and their implementations for database operations like get, update...
  - Utils: functions for quality of life, like logging.
  - Cross_cutting: dependency injection for a collection of many methods in different layers. Mostly for single database connection initiation.
- Presentation
  - GPUI: GPU accelerated Rust dependency used by Zed.

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
