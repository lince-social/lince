# File Sync, and code as links

Recorded as an idea and deliberately not scheduled.

A File Sync mode that takes **every** file in a directory, respects `.gitignore` and makes each one a Record, optionally with an assertion marking it, and then a visual text editor over those Records. Then a developer flag on that folder that turns the code's own structure into links: in Rust, a module declaration becomes an import link, and a call to a function or a use of a struct or trait becomes a link between the Records. Links a person adds by hand stay hand-made and never rewrite the file.

Why it is worth more than it looks. If code files are Records with import and call links, then a traversal policy over the assertion graph walks *code* — the same primitive doing double duty. "Read this function's Record, glance at everything it calls, summarise everything that calls it" becomes one policy over a graph instead of a bespoke code-intelligence feature, and it arrives as configuration rather than as a subsystem. That is a large part of what makes an agent good at a codebase.

The links come from a language server, not from a model and not from our own parsers. A language server already answers exactly these questions — where a symbol is defined, who uses it, what a file contains, and who calls whom — so one integration buys every language that has a server, with no per-language parser and no tokens spent. It is also more correct than parsing, because it resolves through imports, generics and re-exports, which a regex or a tree-sitter query does not. The cost is that a server has to be installed and running per language, wants a real project rather than a loose folder, and is slow to start on a large repository.

What it would cost, from what is already known: a sync folder is flat today, with one `head.md` per Record and a `FileFormat` of only Markdown or Lingua.

- [ ] Support nested paths, arbitrary extensions and a file's own name as identity.
- [ ] Send binary files in such a folder to the blob store, since Record bodies are text.
- [ ] Keep derived links distinguishable from hand-made ones. A refresh must never wipe a person's manual link and must never add a second copy of one it made before; `asserted_by` plus a provenance marker is the shape of the answer. Getting this wrong is worse than not having the feature, because it silently eats work — and it gets more load-bearing under a language server, not less, because links then refresh on every edit rather than once.
- [ ] Make the refresh a background job with a visible state, never something that happens while a person waits.
