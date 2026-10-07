---
name: componentree
description: Find related features and propose shared families before coding. Use when the user invokes componentree or asks to organize a feature around shared behavior.
---

# Componentree

1. Take the requested feature and search the codebase for related features, components, traits, systems, actions, and utilities. Trace how they share behavior today.
2. Propose joining one or more existing families. If none fit, propose a small new family with concrete members and shared responsibilities. Treat a family named by the user as the starting proposal.
3. Show the proposed members, shared behavior, relevant code, and changes needed. Ask the human whether these are the families and behaviors they want. Wait for approval before editing project code. Reuse explicit approval already given for the same proposal; do not ask again.
4. After approval, implement the requested feature through the shared family implementation. Add specialized behavior only where needed, keep changes within the approved scope, and verify shared behavior for existing and new members.
