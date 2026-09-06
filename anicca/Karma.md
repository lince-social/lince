# Karma

Karma is Lince's rule layer: a rule watches for something, decides, and proposes a consequence. Most of it is built. The last link is not, and that one gap decides what every feature built on top of Karma is allowed to promise.

## Karma cannot dispatch anything

Its intents are inert. Nothing takes an accepted candidate and turns it into an op. A rule can be written, it can fire, it can produce a candidate, and then nothing happens. Anything designed against Karma today must therefore be designed against its vocabulary and implemented on something else, or it will look built and do nothing. Nothing anywhere should read as though writing a Condition makes something happen.

`effect_queue` at `0001_init.sql:170` is what to implement on in the meantime. It is a durable queue with `kind` of command or notify, `status` of queued, running, done or failed, plus `attempts` and `origin_uid`, with claim and finish helpers in `store/src/misc.rs`, and it survives a restart. When dispatch lands, Karma becomes the configurable front over the same queue and nothing designed this way has to change.

## The vocabulary that already exists

The parts of a rule are built and frozen, so a design that uses these words is using real ones.

An input can be a saved Protein view: `InputSource::SavedProtein { view }` at `nucleus/src/karma/ast.rs:178`. That is deliberate and it is where selection belongs. Karma's own `Condition` is numeric on purpose — condition, then gate, then carry, on exact decimals — and forcing set-matching into it would wreck the one thing it does well. So a rule that needs to select a set of Records names a Protein view; Karma never grows a second query language.

A trigger can be a Fact: `TriggerSource::Fact { record, concept }` at `nucleus/src/karma/ast.rs:152` fires on a Fact about a Record or about a Concept, which is precisely "an assertion I care about was made". `record_assertion` rows carry `asserted_by`, so "who asserted this" is already answerable — which is what makes it possible to treat an assertion that arrived over sync differently from one a person wrote here.

A route says what a candidate is for: `CandidateRoute` at `nucleus/src/karma/value.rs:15` is `Observe`, `Recommend`, `Draft`, `Ask` and `Act`, already frozen in the revision hash. `Ask` is "put this in front of somebody".

`InputSource::SecretMetadata { secret }` at `nucleus/src/karma/ast.rs:180` exists with no secret store anywhere behind it. That gap belongs to `anicca/Secrets.md`.

## Runaway rules

A rule that fires on assertions and writes assertions can wake itself forever. The shape Karma already chose for runaway cycles is the right one everywhere: a ceiling per rule per window, and when it is hit the rule pauses and says which rule and which cycle. Not silently killed, and not silently infinite.

Two more bounds belong with it, because both are ways somebody else spends our machine. A rule must not fire on an assertion that arrived from a contact's sync — a peer must not be able to spend our resources, and the assertion carries who asserted it and every Record carries the Organ it came from. And a rule must not fire on writes made by our own automation unless it says so explicitly.

- [ ] Build dispatch: take an accepted candidate and turn it into an op, so a rule can finally cause something. Until this lands every consequence is written against `effect_queue` by hand.
- [ ] Enforce a ceiling per rule per window, and pause the rule naming itself and its cycle when it is reached.
- [ ] Never fire a rule on an assertion that arrived over sync from another Organ.
- [ ] Show the queue a rule lands in: what is waiting, who claimed it, what failed and why. A rule system nobody can inspect cannot be told apart from a broken one.
