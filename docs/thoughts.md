# Interface

Dont ever do: 3. Do these points in this order: 2 (if we can say fuck compatibility and attend to 4 in this porting to new then do it, if you can do 1 while you do these points we save the trip so you start doing 2 after all the tokens for all the different parts of ui have been laid out before we can enforce them and migrate existing ui (base+sands)) then 7 (make 7 be what you said, including making sure the system follows the load order and everything we can make be tokens are tokens, for color and spacing and thickness...)

# 1. 674 — Style load order isn't nailed down: default tokens → LynxUI → Sand structural CSS → configured global style → per-Sand style.
# 2. 686 — Migrate every official Sand to LynxUI for ordinary elements, drop duplicated component CSS.
# 3. 687 — Formalize "official-first, unversioned" stance (compat not promised to user Sands yet).
# 4. 688 — Keep LynxDS-components.js compatible with persisted older shell HTML while new code uses window.LynxUI.
5. 689 — Keyboard nav, focus management, ARIA state, help/error association, non-color status meaning, no transitions — across all components.
6. 690 — Treat specialized-Sand exceptions (graphs/terminals/games) as a code-review convention, no manifest/exemption attributes.
# 7. 693 — Sand-author docs, component behavior tests, iframe tests, theme-override tests, hardcoded-color/shadow/transition checks.

Most of these (1, 5, 6, 7) are cross-cutting checks/tests rather than net-new components — the component set itself (668-685) is done. 2 (migrate official Sands) is the big remaining implementation task; the rest are guardrails/docs.

Base UI features (unchecked, above LynxUI in "Customization and architecture" / "Space, thickness..." / etc., lines 593-666) — this is the semantic-token system LynxUI sits on: per-Sand/global style selection+persistence, the Lynx colorscheme (48 vars, light/dark), spacing/border/radius tokens, badge/status non-color rules, transparency/elevation rules, typography, motion (none), and a design-system lint check rejecting hardcoded colors. None of it is ticked — it's the substrate LynxUI is meant to consume.

Given item 2 (migrate official Sands) depends on the base-UI token system existing and being consistent, want me to check current state of the codebase — is the token/colorscheme system actually implemented (styles/ dir, semantic CSS vars, config table wiring) — before we pick a starting task?

---

 Ok, i want to completely remake the way we do sands, so we appeal to the part in docs that says about sand composition and the Sandbox sand: we take the individual parts of sands and separate them as building blocks, so when someone wants to create another sand or even edit the existing complete sand they have they can do it. I started playing with that concept in a different working way with events, when kanban card is clicked event of record clicked to the Record sand, or making the sand group way of before of putting kanban as a group with the kanban sand and the record sand as a group, but this idea would be on another level. it is about editing the sand while they exist and of creating another sand in a sand editor, the sandbox sand. But thinking about it: if i can edit a sand while it exists, then we need to be able to take the sandbox sand and putting it's capabilites of CRUDing a sand into every sand, so i feel like it will become a capability of the edit mode of the board. We should merge the sand store and sandbox sand and sand editing into one thing, when we want to add a new sand we go into edit mode and add another, just like we can have groups of groups of sand we should then think of imported sands (depending on the way it is constructed, one piece or a group) as groups of components, if it is a "complete" sand or a "component" that is an abstraction on the human, to the board it is either a lone sand, or a sand in a group.

-------------------------------------------------------------------------------------------------------------------------------------------------------


# Lince - Philosophy

#Person-level key loss — a suggestion, not written to docs yet

This one I'd want your sign-off on before committing, since it's genuinely new machinery. My instinct: don't give every Person their own offline root — that's the right cost for an Organ's identity, wrong cost for every individual human in it. Instead, reuse what already exists at two different scales depending on context:

- Inside a multi-Person Organ (family, project, community), the Organ's own already-established internal trust is the recovery mechanism: the same identity_succession/successions table already built for Organ-level key rotation gets generalized to cover a Person's signing key too, attested by the Organ's operational authority (or a small quorum of other Persons in it) rather than a separate ceremony. You already trust the people in your family Organ enough to share a roster; that's the same trust a re-key needs.
- Inside a solo Organ (you are both the Organ and the only Person), a lost Person key is really just another key in a roster you already control — the Organ's own existing operational key re-issues it, no new mechanism at all.

For absorbing a dormant Person's things — different problem, more sensitive, and I'd keep it strictly separate from key recovery: never automatic, even after a long configurable silence. Route it through Karma's Authority/Decision Queue (§5) the same way we just decided custody revocation can only be a proposal, never a command — a long-dormant Person's open Needs/custody surfaces as a Decision for another trusted Person (or a time-boxed steward, the Moss-inspired role from earlier) to act on by hand. Consistent with everything else we've settled this session: Lince proposes, a human decides, nothing forces itself on someone who can't consent anymore.

Want that written into Ontology.md's identity section, or does it need more thinking first?
