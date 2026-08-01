
> Extracted from `docs/Central: Karma.md` on 2026-07-29. These are the standing
> laws of the whole system, not of any one pillar — which is why they no longer
> live inside one pillar's file.

## [x] Cross-cutting maneirisms cheat-sheet

- [x] Everything is a record; activation is quantity; delete is deactivate
  (and hard-delete is a separate, further step).
- [x] The fact is the truth, quantity is the cache; undo is compensation.
- [x] Metadata/state changes announce themselves as zero-delta annotation
  facts.
- [x] Warnings are advice (cycles, Proof loops), never rejections.
- [x] Current heartbeat order: promise expiry → decision expiry → timers →
  signal sampling → effects (budgeted notify) → senses pass → crossings pass.
  Karma phase K3 replaces the polling heartbeat as timer owner with the
  tickless deadline director/sequencer while preserving explicit stable
  priority.
- [x] One situation, one open decision (dedup by subject+kind).
- [x] Uids are identity everywhere, across Cells; slugs are local sugar and
  get dropped on collision at import.
- [x] Visibility is default-hidden, whole-row, enforced in exactly one
  place — and applied before aggregation.
- [x] Blocked organs are rejected at every door (import, discovery,
  outbox).
- [x] Conventions: uids are prefixed ULIDs (`r_/f_/p_/l_/c_/t_` = record,
  fact, promise, link, concept, transfer); slugs are `dot.case`; timestamps
  RFC3339; durations `90s`/`2h`/`30d`; `@slug` in conditions is sugar for
  `quantity(@slug)`.
- [x] Time is deliberately NOT a record column: automated timing = Karma
  schedules/Frequencies; declarative time (what strangers match on) lives on promise
  windows.
- [x] Board chrome is frontend state; sand data is Protein/Actions.

## The theory (from the retired blueprint)

**The refounding, three sentences:**

> **Everything is a Record. Every change is a Fact. Every intended change is a Promise.**

**The pillar map:** Record (state) · Memory/Ledger (facts) · Lingua (shared
concepts; Instinct tier = concepts with engine functions) · Karma
(Signals → Context → Senses/Rules/Imagination → Recommendation/Attention →
Policy → Effects) · Transfer (promise bundles under agreement + visibility) ·
Trust (verifiable signed deltas) · Protein (declarative reads) & Actions (typed
writes). Humans and software agents manage the same Karma knobs through
those shared read/write contracts.

**The placement rule (the Window):** the core owns what must be computed,
verified, or agreed across Organs; interfaces own what is seen; embed
honestly what the world already built well. Altitude ladder: Primitive →
Pillar engine → Instinct → Lingua concept/unit → fds sidecar → Sand →
Embedded foreign app.

**Non-negotiables:** quantity stays central (negative = Need, positive =
Contribution, zero = peace); quantity-as-activation on everything; full math
in typed Karma computation graphs; compatibility fully ignored —
greenfield build, old data ported by hand; local-first; no global reputation
score, ever.

**Storage:** SQL is SQLite dialect; Protein is the abstraction that later
permits AniccaDB — replacing `store` must not change one character of the
Protein/Action contract.

**The Window's standing law:** apps are projections of one organism. A sand is a
bundle of HTML and JavaScript that reads through Protein and writes through
Actions — never a layer, never a phase, and never a name the backend knows.
The Karma sand is one such sand: full CRUD over the rules that change Records,
assembled from units, exact Facts, classification and schedules. **Economy is
not a sand.** It is what the Karma sand looks like when the rules in it are about
a balance, exactly as a pantry is what it looks like when they are about flour —
a preset and a set of Records, shipped as data. Inventory and pantry reuse the
same primitives directly, and none of them may push a domain name downward:
`crates/web/tests/sand_boundary.rs` fails the build if `economy`, `money`,
`currency` or `finance` appears as an identifier in any backend crate.
Correspondingly there is no money type in the kernel — a currency is a unit like
any other, `Quantity { amount, unit }` says it, and converting between two units
is a rule someone writes, not a feature. Chat = comments =
negotiation (messages on a shared object); profiles = catalogs = libraries
(published records behind visibility). When a new workflow
arrives, triage it against the primitives; if it doesn't decompose, the
missing piece is named by what resists — that is how the next abstraction
gets deduced instead of appended.

**The north star:** Ana wakes. No dashboard. The kitchen scale posts a fact;
beans cross their threshold; a promise to the roaster activates under a rule
she approved months ago; two Cells settle Saturday pickup. One whisper on
the walk to work — a nod, two promises change state. Work is an Organ; the
standup is a view nobody fills in. A second whisper near the market — her
mother's pantry Need, published to family only, met on the way home. In the
evening she scrubs the timeline out of curiosity: rent fine, a bar's event
Organ bit on her guitar Need, the tomatoes surplus in nine days and the
donation rule is staged. The Lincegoshi grows fat and luminous and
dissipates. Under four minutes of managing life, all of it decisions only a
human could make.

That is the Death of Lince: management time asymptotically approaching the
irreducible minimum — the moments of actual human choice. More needs met,
more transactions peer-to-peer, more donations, more efficiency: the dance
of the world, made executable. Everything is a Record. Every change is a
Fact. Every intended change is a Promise. The rest is choreography — and
Protein is how the dance is seen.


The Lince Way:

We model all aspects of the world with Records. It has 'head' and 'body' for title/description information and a 'quantity' for number. The central point of the Lince workflows is the changing of the 'quantity'.

Karma is an automation system that can take information such as the quantity of Records, calling (shell or SQL) Commands and Frequencies and evaluating (any Rust code that fits) it in a math equation called a Karma Condition. If it passes a threshold ('=') of being unequal to zero (or doesn't need to '=\*') it will carry the value onto the Karma Consequence. The Consequence can be the changing of the Record's quantity, the calling of (shell or SQL) Commands, Transfers being active, and more (syncing tree of Records between Organs).

The Frequency part of Karma is a built-in cron-job, example:

Condition (math): frequency-1 \* record-quantity-3
Threshold: =
Consequence: record-quantity-4

If this frequency is set to one month plus one day, when evaluated frequently (default Karma cycle happens every 60 seconds) it will return most of the time the number 0, only once per month plus one day it will return the number 1, so the math of almost always '0 \* something = 0) will make it not pass through the Threshold that stops zero amounts and not bring the Consequence. In this case the consequence is the changing of the quantity of the fourth Record (with id 4). So this Karma row is for the setting of record 4 to have the same quantity as Record 3 once every one month and one day.

The usage of Lince revolving around the Record's quantity means users can create rules that edits a collection of numbers as state, being more generic than fixed text options, Karma and Transfer following this creates pressure towards The Lince Way. It ss usefull to join several pieces of information with +-\*/() equations and creating business rules as much as possible with data (says Lince).

A Lince node is called an Organ, it can have many users inside or not. People can have users in various Organs, a personal one, family, work, project... And access them all through any one of them with login.

Two different Organs may want to use Records as the concept of Needs and Contributions to Needs. For executing interactions between users of Organs we have the Transfer feature, it can carry out donations, putting only the one way Contribution, or an economic trade by having a Need being met with an item/service Contribution one way and a Contribution for a money Need on the other way. That allows from buying items in digital market platforms to planning a party.



| "Death is only the beginning" - The Mummy.

The Lince tool is allows for registry, interconnection and automation of Needs and Contributions with open scope.
The phrase above is the densest way to explain Lince. Let's explain it well, starting with the registry part:

### Registry
We all have different Needs. To satiate them we do personal/professional tasks, acquire personal items, perform economic trades, socialize with others, study... Lince helps you organize the meeting of those Needs. You put your Needs in Lince, the frequent or the one-time ones. You create Records of them, give titles (head) and descriptions (body) and assign a quantity, with that you created data on your Need, now you can play with it, put your finger on it, connect with others, see the bigger picture, automate them, and help others do the same.

### Interconnection
With your Needs modeled you can connect with others, they might Contribute to yours or the other way around. You can promise to give something to someone, and expect something in return, or not. After that it is your responsability to complete the Contributions on your side. This process is called a Transfer Proposal.

### Automation
Now that we know what our Needs are, and how we can receive Contributions to them, and do the same for others, we might want to satiate the need of managing our interactions with Lince and other systems. For automation and more, Lince has Karma, a simple system of Conditions that bring Consequences that can build complex workflows.

Many great ideas are lost with time. After doing the registry, interconnection and automation we might create a pattern for efficient Contributions. If that pattern is for basic needs, we might feel more inclined to make it public, turning our system into a blueprint for others to evolve their own workflows.

With this philosophy we can automate the remembering part of habits, creating a need for them everyday, we can assign work tasks, buy, borrow and donate items, create roadmaps for others to follow, in the areas of production and education, and do much more.

## The Death of Lince

When you hear Lince, most of the time, unless explicitly stated otherwise, you are hearing about the Lince tool, not the philosophy. While it is the philosophy that drives some to built the tool and apply it, others may use the Lince tool for other purposes. Different uses of Lince are accepted, since there's no way to avoid it, and expected, since better ideas are always possible.

Most software is repetitive Create, Read, Update, Delete (CRUD) operations. We can create a good general base for basic CRUD software, and continue to expand beyond that. The diversity in what can be created with it is the real power.

People will have access to a completely free and open source todo app, event planner, financial analysis platform, industrial production planning and more; all with a change of data (DNA) and maybe some extensions.

Once the operational part of life is modeled, automated and interconnected, the effects of having built a robust and generic base for interaction with systems will start to show. If there is no incentive to rebuild a management system/platform (because there is a clearly G.O.A.T.ed one), more efforts will have to be focused on new highly specific innovative and optimizing ideas. People will enjoy having previously centralized, private and paywalled capabilities now in close reach for free.

Think of platforms for purchasing items or car rides, they are companies that get a cut, to orchestrate something that computationally a global peer-to-peer network personal devices could easily manage. Think of how much your much time your devices spend processing your online purchases and how much it could be plugged into the LNHM (Lince-Net-Hive-Mind).

Item purchase platforms in Lince are for Records of item Contributions, your purchases are Transfers. The car rides you request on are Transfers of money for 'Transportation from point A to B'. Any automation they make inside their platforms is limited to the data they have on you, which is a miniscule amount of your whole life (at least I hope it is). If you where to set up hacky ways to use those platforms automatically to fit your needs, you would be doing it to every app you have, connecting them in ways they where not meant to, possibly programming something to organize all those changing external apps. It doesn't need to be that inefficient, you can have one central program that you can trust to work offline for you, acting just the way you want, featureful, built by many people, for it is the main one.

The model of Lince can be a superset of many apps, and can spark many different markets and support people with it's usage. Those that occupy their time building the basic operations of life now with a good generic base can focus fully on the differential part of their Contributions to the world: the steps, the insights, the algorithms, a higher quality.

This is just an abstraction: When you install Lince (the tool) you have a Lince Cell, with it's personal data/behavior (DNA). When a cell is used by several people we call that an Organ. When you connect with other Lince nodes, you assume they are possibly Organs, you dont know how they function internally. Lince, the philosophy, is when all Cells and Organs work together to satiate everyone's Needs.

The assumption is that an app that is useful for a lot of cases and incentivizes people to help each other, with no extra friction, is the best use of technology we can have. It will not solve anything by itself, but is is the best tool in the software part of the world.

Like every good Contribution, the goal is to not have to Contribute anymore; because it was a one time Need, or the Frequent Need has been solved.

Lince is a Contribution to the Need of having Lince, of having all production possibly connected, so there is predictability in resource usage and logistics coordination.

It's a Contribution to the disorganized construction of solutions glued by the internet, centralized in platforms that worsen by the day.

It's a Contribution to understanding the whole of possibilities of how you can connect with other people, after seeing Needs you didnt know others had, sparking ideas of Contributions you didn't know you could make.

The death of Lince is the death of the Need of Lince. If there is ever any configuration of resources that makes Lince useless, then it's job is fullfilled, there is nothing else to be done, no commit, no push, just vibes. Untill then, a system for self organization that can turn into a dance of the world is, to some, something useful and exciting to build.

A team is being assembled, to fight the battle of killing Lince. Will you join the party?
