Lince could help people share ways of meeting Needs, with the software to put those ways into practice.

These are exploratory proposals from 2026-09-05, written at the owner's invitation to rethink the prototype. They are not accepted decisions or a replacement for [Lince.lingua](Lince.lingua). No implementation is proposed as already complete.

I read the philosophy, Records, Assertions, Trail, Transfer, Interface, Karma, Fiote and Resenha in that Record; the relevant Ontology, Karma and Interface working notes; and representative Record, Assertion, Promise, Transfer, Fact, Protein and Fiote code. The working notes contain historical and conflicting directions. In particular, the Fiote build notes' own-harness direction differs from the Fiote Record's “Pi for now”; the Record takes precedence. This essay does not resolve that implementation choice or audit every completion claim.

What feels most distinctive about Lince is the continuity it wants between understanding a situation, deciding what would help, finding people and resources, doing something, and learning whether it helped. Personal organization becomes collective organization without requiring someone to abandon their own tools, meanings or boundaries. “The death of Lince” gives this an unusually useful product criterion: the system succeeds when the person needs less management to live well.

I would preserve Records, attributed Assertions, Cells and Organs, explicit authority, inspectable Actions, and the separation between a proposal and something that happened. I would reconsider making one signed quantity carry almost every meaning, and making people configure each pillar separately to build one useful workflow.

The most promising connection is this:

> A Need describes a desired situation. A Trail describes a way to reach it. A plan applies that Trail here. Promises establish who has committed. Actions and observations supply evidence. Fulfillment says whether the Need was met.

Those are different roles and stages in one connected model. They need not become six new object systems or six screens someone must fill out. A small task can remain one Record and a completion control.

The first change I would make is to separate an object, its state, and someone's relationship to it.

“My bicycle exists,” “I can lend it Tuesday,” and “I need transport Wednesday” describe three different things. A positive bicycle quantity should not itself mean permission to use the bicycle. A negative temperature should not imply a Need. A Need for companionship should not require inventing a unit of companionship.

The current Assertion model already carries an author, an optional object, a quantity and a unit. That is an excellent starting point: quantities can belong to meaningful relationships as well as a Record's convenient primary counter. The extension would be to describe relevant time, context, evidence and value types consistently, making those descriptions available to queries, actions and interfaces.

Keep “-3 apples” as a very good shortcut. It can create a Need for three apples with sensible defaults. As the situation becomes richer, distinguish the apples observed in stock, the apples offered, those reserved, expected deliveries, and what the owner wants to retain. The visible number can be a useful projection of these descriptions. People should not maintain all those numbers by hand.

Also distinguish how resources participate. Food is consumed; a drill occupies a time slot and returns; a document can be copied; a machine transforms material; a lesson can change what someone knows. These differences unlock more applications than giving everything additional generic tags. Valueflows provides useful precedent in its distinctions between consume, use, cite, produce and transfer. I would borrow the distinctions that Lince needs, without making adoption of that entire ontology a prerequisite. [Valueflows actions](https://www.valueflo.ws/concepts/actions/).

A Need should be able to describe an outcome and acceptable alternatives.

“Have a working bicycle by Friday” allows repair, borrowing or acquisition. “Own this particular bicycle” does not allow all those substitutions. Lince must preserve that choice. It should never silently generalize someone's request into a different goal because an alternative is cheaper.

Some Needs describe reaching a state; others describe maintaining it, preventing something, or having an experience. “Finish this chapter,” “keep enough supplies for next week,” “avoid overlapping bookings,” and “spend an afternoon making music together” all belong. Numeric targets are useful when they express what the person actually wants. Personal confirmation can be the entire completion criterion for a qualitative Need.

Completion and fulfillment deserve separate meanings. Finishing a repair checklist is evidence toward a working bicycle. It is not proof that the bicycle works. A delivery receipt can establish that an item arrived without establishing that it satisfied the recipient. Lince should let the relevant person accept, partially accept, dispute, or reopen fulfillment, with simple defaults appropriate to the activity. A recurring Need should create distinct occasions when that matters, so meeting last Monday's Need does not accidentally meet next Monday's.

This also clarifies Facts. A signed Fact can establish that an Actor reported something or that Lince performed a particular data change. It does not establish that the reported physical event is true. Observations can disagree. Inferred, reported, measured and accepted information should remain distinguishable, without requiring a cumbersome evidence form for an ordinary checkbox.

For ontology, I would favor small descriptions of what a workflow needs over a mandatory classification of everything.

An object might be schedulable, lendable, inspectable, measurable and repairable at the same time. A room, bicycle and projector can share booking behavior because each has availability and reservable capacity. They do not need a common grandparent in a universal taxonomy before their calendars work.

These descriptions can be reusable contracts over Records and Assertions: a booking needs a resource, interval, beneficiary and capacity rule. A measurement needs a value, unit, observed time and source. A review needs an artifact, reviewer and decision. The contract supplies validation, default controls and available actions. It need not introduce a dedicated Record kind for every combination.

The important principle is to require information at the moment it becomes necessary. A note saying “borrow a projector” is valid immediately. Confirming a booking requires a particular resource and time. If two installed descriptions impose incompatible constraints, Lince should explain the conflict at that boundary; an installation order must not silently choose which one wins.

There is a useful precedent in SHACL's separation between a data graph and shapes that validate it. My proposal extends that idea into Lince's authoring and action surfaces; SHACL itself does not supply those product behaviors. [W3C SHACL](https://www.w3.org/TR/shacl/).

Vocabulary bridges would make this work across communities. Two groups may describe compatible resources differently. A bridge can say that one concept is narrower than another, that a unit conversion is exact, or that one thing is an acceptable substitute for this particular Need. Those are different claims. “Can substitute here” must not become “is identical everywhere.”

Fiote can propose a bridge from examples, but the bridge should become an inspectable, attributed mapping with a scope. Names changing should not break it, and two similarly named concepts should not be merged automatically. A shared Vocabulary establishes intended meaning; an actual resource may still require a grade, condition, time or other qualification before a Transfer is appropriate.

The second major change is to make a Trail carry a way of acting, as well as knowledge about acting.

Your Trail Record already reaches toward this: knowledge, procedures, recurrent work, Transfers and interfaces arriving together. I would make that the primary way a person acquires an application in Lince.

A “lend equipment” Trail could carry the booking contract, checkout and return actions, overdue behavior, resource views, examples and explanations. Its roles are generic: equipment, borrower and custodian. Installing it means choosing which existing Records fill those roles. It should use the existing equipment inventory rather than creating a separate equipment database.

A “repair a bicycle” Trail could describe the outcome it helps achieve, required parts and skills, alternative steps, checks, and a useful work surface. Applying it to a particular bicycle creates only the missing plan information. A completed repair can later inform a revised Trail, while private names, schedules and prices remain outside the reusable example unless deliberately shared.

The abstraction is powerful because a procedure can cross domains. Booking serves rooms, tools, lessons and performances. Review serves code, grant applications and research claims. Transformation serves cooking, fabrication and document conversion. Domain expertise still matters: the bicycle repair procedure must come from someone who understands bicycle repair. Lince supplies the reusable coordination machinery.

The hardest part of reducing configuration is binding these reusable pieces to the person's world. A Trail should declare the roles it needs, suggest existing Records that fit, explain uncertain matches, and ask only for the remaining meaningful choices. Repeating a launch should reopen the same setup when intended. Installing a related Trail should reuse its shared data and identity. Private live data and reusable definitions must remain separable.

An executable Trail also needs a clear revision and declared effects. A future update can improve instructions without silently increasing authority or rewriting an agreement people already accepted. This is about the integrity of present commitments, not compatibility with old Lince versions.

For Karma, I would keep exact rules and give people a higher level at which to describe useful behavior.

“When this happens, do that” remains valuable. But “keep the pantry stocked for seven days” is a clearer expression of intent than manually creating separate rules for forecasting, thresholds, reservations, ordering, incoming deliveries and retries. A Trail can compose those pieces into one behavior with a few human parameters.

Underneath, separate reading a derived value, proposing a plan, and performing an action. Displaying predicted stock must not place an order. A plan to obtain food must not count as food arriving. Pure calculations should be reusable by Protein, Karma and simulation without making one effectful rule run another just to obtain its result. This can respect the Record's rejection of recursive `value(@rule)` reads: reuse named calculations or views, not hidden execution chains.

The most valuable shared definition may be the action itself. Describe its typed inputs, preconditions, effects, required authority, and what confirms completion. That description can support an ordinary button, a Karma consequence, a Fiote tool, a preview and a simulation adapter. Special rendering or domain code can still exist behind it. The person should not reconstruct the same business operation independently in five places.

For example, “reserve this equipment for these hours” should have one meaning whether a person clicks a calendar, a rule requests it, or Fiote prepares it. Each caller may have different authority, and the action enforces that authority. Assigning a lendable description to a Record must not itself grant someone permission to lend it.

Automation should also handle changed plans. If the repair part is delayed, the useful response is to show which promises are affected and offer a borrowing alternative. Blindly repeating the original order misses the purpose of the automation. A running plan therefore benefits from explicit waiting, partial completion, cancellation and recovery. These can be reused workflow behaviors rather than hand-built boolean conventions in every app.

A person deliberately creating an automatic rule can authorize its stated effects at creation, within selected limits. Repeated confirmation need not be the default. When an action falls outside those limits, the proposed change should explain exactly what new choice is needed. Authority belongs to the workflow definition and Actor context, not to how persuasive a model's text is.

Transfers could become the place where several contributions form one workable plan.

The Transfer Record already imagines multiple parties, alternative solutions and dependencies. I would push this further: a Transfer can coordinate conditional commitments around a shared outcome, including use, work, delivery, return and verification. A sale remains an easy preset. A private task should not require a negotiation merely because it shares the underlying action machinery.

For a community dinner, one person offers a kitchen if someone cleans it afterward, another offers ingredients if transport is arranged, and someone offers transport if pickup finishes before six. Lince can show what is missing before anyone treats the dinner as confirmed. The accepted plan links the commitments, rather than scattering them across unrelated chats and calendars.

This makes “What could I contribute?” a much more interesting query. Transporting one ingredient might unblock the whole dinner. Repairing one shared tool might help several projects. Lince could show the chain of known dependencies and let a person choose where to help. Its claim should be modest and concrete: “This is the remaining dependency in these three plans,” not “This action objectively helps humanity most.”

Reservations are crucial here. “Available” depends on time, existing commitments, the owner's retained needs and the authority to allocate the resource. Future deliveries should be shown separately from stock physically present. An offer is weaker than a reservation; a reservation is weaker than evidence of delivery.

Offline use needs an explicit allocation policy. Two disconnected Cells should not each promise the same last bicycle. They can save tentative requests, use one responsible booking authority, or operate within previously allocated rights where the resource model permits it. Bounded-counter research is a useful precedent for allocating numeric spending rights; applying that idea to exclusive equipment and intervals requires additional design. [Balegas, bounded-counter design](https://run.unl.pt/bitstream/10362/27864/1/Sousa_2017.pdf).

Nor can a distributed agreement make physical actions atomic. Once someone has cooked or traveled, cancelling another commitment does not undo that work. Plans need expiry, partial fulfillment, amendment and agreed responses to failure. Those responses are coordination features that many applications can share.

Valueflows' distinction between recipes, intentions, commitments and observed events is especially relevant to this progression. It also describes reusing information between these stages to reduce data entry. The Lince-specific opportunity is connecting that continuity to personal Needs, executable Trails and composed interfaces. [Valueflows flows](https://www.valueflo.ws/concepts/flows/).

Resenha could become a way to rehearse a decision in the ordinary interface.

“What if we hold the event on Saturday?” should use the same booking and resource semantics as the eventual plan. “What if delivery is two days late?” should reveal the promises at risk. A scenario should store assumptions and proposed changes against a known state; accepting it requests changes against the current state, where reservations and permissions are checked again.

The same declared actions should participate in rehearsal and real execution, with separate adapters for external effects. A simulation must not send real invitations or pretend it knows an external system's response. Where an effect has no useful model, show that uncertainty. Start with bounded time windows, known recipes and explicit delays. A deterministic replay establishes what the model does for given inputs, not what people or the weather will actually do.

This is why I would make a calendar, a dependency view and a simple comparison of alternatives work before requiring a detailed world simulation. Your imagined farm can eventually inhabit the globe. The useful question—what would it take to build this here?—can already be answered by a plan attached to a place and a few sketches.

Fiote's most valuable job could be turning an unclear situation into a durable, usable arrangement.

Someone says, “The bicycle is broken and I need to get to work Friday.” Fiote finds the bicycle, the travel Need and relevant Trails. It offers repair and borrowing plans with visible assumptions. After the person chooses, the resulting Records, commitments, calendar and controls keep working without the conversation. A corrected assumption becomes data the next action can use, rather than a sentence everyone hopes the model remembers.

Fiote could also recognize that someone has performed the same procedure several times and offer to save it as a Trail. That is a useful route from personal habit to reusable application: observe a pattern, explain it, let the person refine it, rehearse it, and then authorize its repeatable parts. Suggestions from private history should remain private unless shared.

A complementary role is asking the one question that would change a plan. “Do you need your own bicycle, or would borrowing work?” removes a real uncertainty. Asking the person to choose Record kinds and wire ports exposes the implementation. Free-form conversation remains available, but neither a model nor conversation should be required to operate the resulting workflow.

For the Interface, compose around the current activity as well as around data types.

A lendable resource could offer a booking calendar and availability control. A plan with several Actors could offer a coordination view. A disputed observation could offer an evidence comparison. Sands declare what inputs and actions they support; a Trail supplies a curated arrangement, with Box available for changing it.

Generated controls provide a useful baseline, but good interfaces still need designed layouts. A booking form and a resource timeline should share one booking contract without being forced into the same appearance. Spatial movement can request a domain change when a view explicitly defines that behavior; moving a card for presentation should not accidentally create a commitment.

The lower-configuration test is that a person can open “lend equipment,” choose their equipment and policy, and start lending. An author can later open the composition and understand how it works. Requiring every person to build a Castle before borrowing a drill would miss the goal even if every component were reusable.

I would test the proposed foundation against a few deliberately different applications:

| Application | What it forces Lince to express | Reused elsewhere |
| --- | --- | --- |
| Tool library | Availability, exclusive reservations, custody, return and condition | Rooms, vehicles, rehearsal spaces |
| Repair cooperative | Diagnosis, alternative methods, parts, labor and acceptance | Maintenance, production, service work |
| Learning group | Prerequisites, practice, mentoring and evidence of understanding | Training, onboarding, research |
| Community dinner | Transformation, multiple contributors and conditional commitments | Events, workshops, collective purchasing |
| Research notebook | Competing claims, provenance, revisions and reusable methods | Quality review, incident analysis |
| Creative project | Artifacts, collaboration and completion without a compulsory score | Music, writing, games, exhibitions |

If every application requires another backend kind, the model is still too rigid. If each needs a large bag of unexplained assertions and custom scripts, it is still too empty. Some specialist code will always be appropriate: geometry, audio processing and game simulation need their own engines and artifacts. Lince can coordinate their inputs, outputs, Actors and Needs without putting every vertex, sample or frame in its ledger.

For collective decision making, I would avoid one universal score for people, Needs or Contributions. Trust can be evidence relevant to a particular action and relationship. Fairness can be an Organ's explicit, revisable policy. Privacy matters even for useful recommendations: an offer of delivery should not reveal the private reason someone needs help. “Helping everyone” requires room for different values and for people whose Contributions do not produce tidy metrics.

A further possibility is to make long-term reduction of recurring Needs visible. Delivering water today and helping establish a maintained water supply can serve related Needs on different timescales. A Trail could show both immediate relief and the resources, training and ongoing care needed for a more lasting solution. This would give the philosophy of making Lince unnecessary a practical expression, while keeping the choice with the people involved.

The experiments I would consider first are these. They are suggestions for human refinement, not a new implementation schedule:

- [ ] Describe one booking contract and use it for a tool and a room, with complete usable interfaces and no new backend kind for either.
- [ ] Apply a repair Trail to one bicycle; compare repair and borrowing against the same transport Need, then record whether the chosen contribution helped.
- [ ] Author one reservation action once and expose it through a Sand, Karma, Fiote and a bounded rehearsal, with the same validation and distinct caller authority.
- [ ] Install two Trails against the same resources, verifying that shared availability and commitments are reused rather than copied.
- [ ] Run a small collective event through a late delivery, a withdrawal and partial fulfillment; make the consequences understandable on screen.
- [ ] Reuse those foundations for a learning or creative activity whose fulfillment is qualitative, to find assumptions that only work for inventory.

I would judge these experiments by how many meaningful choices the person makes before receiving help, how many declarations an author repeats, and how well the plan remains understandable when something goes wrong. The strongest breakthrough would be a Trail that someone can understand, adapt to their own circumstances, and put to work immediately—and whose useful pieces can meet the next person's Need too.
