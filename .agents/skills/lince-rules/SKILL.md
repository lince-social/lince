---
name: lince-rules
description: Preserve Lince's declared behavior when implementing, fixing, refactoring, or reviewing Lince. Search Lince.lingua before changes and check the finished changes against its pipe-prefixed requirements, including related features and interface details.
---

# Lince rules

Read the [lingua skill](../lingua/SKILL.md) before reading Records.

The owner writes hard requirements as prose lines beginning with `|` in
`institute/trails/lince/Lince.lingua`. Whitespace before the marker is fine.
Read each requirement with its surrounding Record for meaning and scope.
Ordinary prose may explain the subject or discuss unrelated ideas; it need not
be a requirement. Markdown tables, code examples, and quoted sayings are not
requirements merely because they contain pipes.

For example:

```text
| Edit is always the rightmost button in the workspace controls.
| Removing a Sand does not delete the Record it presents.
```

1. Before changing Lince, search the file for the features, concepts, and UI
   surfaces you will touch. Scan `rg -n '^\s*\|' institute/trails/lince/Lince.lingua`
   for applicable rules elsewhere, including shared interface guidelines.
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
