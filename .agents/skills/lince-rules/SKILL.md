---
name: lince-rules
description: Preserve Lince's declared behavior when implementing, fixing, refactoring, or reviewing Lince. Search Lince Records before changes and check the finished changes against their |> requirements, including related features and interface details.
---

# Lince rules

Read the [lingua skill](../lingua/SKILL.md) before reading Records.

The owner writes hard requirements as prose lines beginning with `|>` in
`institute/trails/lince/Lince.lingua` and related Lince Records. Spaces and tabs
before the marker are fine.
Read each requirement with its surrounding Record for meaning and scope.
Ordinary prose may explain the subject or discuss unrelated ideas; it need not
be a requirement. Markdown tables, code examples, and quoted sayings are not
requirements merely because they contain pipes.

Lingua also supports a `//` comment after a comma and before the next metadata
field in a Record header. A comment beginning with `// |>` declares an
agent-readable requirement without adding it to the body shown in Instinct.
The comment may occupy its own line or follow the preceding comma on the same
line, as Lingua's formatter writes it. It is still a hard requirement. This
syntax is supported in headers, not as a general body
comment; HTML comments are not a supported way to hide native Instinct text.

For example:

```text
|> Edit is always the rightmost button in the workspace controls.
|> Removing a Sand does not delete the Record it presents.
```

```text
Protein (@protein: 0,
    // |> [protein-laboratory] Exercise Protein interactions in the fast interface suite.
    #instinct, #part-of @instinct-protein: 1) {
User-facing explanation.
}
```

1. Before changing Lince, search the file for the features, concepts, and UI
   surfaces you will touch. Scan
   `rg -n '^[ \t]*\|>|//[ \t]*\|>' institute/trails/lince -g '*.lingua'`
   for applicable rules elsewhere, including shared interface guidelines and
   hidden header requirements. For body requirements alone, use
   `rg -n '^[ \t]*\|>' institute/trails/lince -g '*.lingua'`.
   The literal `>` excludes ordinary Markdown table rows and separators such
   as `|---|`, quotations beginning with `|`, and Rust `||` expressions. These
   searches find candidates: exclude fenced code examples and confirm a
   `// |>` match belongs to an actual header comment before enforcing it.
   Read the matching Records and related features: changing A may affect its
   interactions with B and C. If the file moved, locate it with `rg --files --hidden --no-ignore -g Lince.lingua`.
2. Preserve every applicable rule. Do not weaken or rewrite a rule to fit the
   implementation, and do not edit Lingua files without explicit permission
   for that edit. If requirements conflict or an ambiguity affects the work,
   explain it to the user rather than silently choosing a new meaning.
3. Before finishing, repeat the search against what actually changed, including
   affected shared components and their callers. Check the applicable rules
   against the result, using tests, visual inspection, or measurements as
   appropriate. Passing existing tests alone does not establish compliance.
4. Fix violations introduced by the work. Briefly report which relevant rules
   were checked and any existing violations or requirements you could not
   verify. If no relevant rule was found, say so; do not invent one.

The marker declares a requirement; it does not execute a test or turn arbitrary
prose into an automated assertion. Map applicable rules to deterministic checks
in the native interface/Laboratory suite, and report unmapped rules explicitly.
