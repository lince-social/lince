Remove the Rhai language engine for Karma, make a manual parsing so we can eval true as 1 and false as 0 and other math operations in that direction.
 
Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6.
        Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2

view dependencies with other records from other nodes, see the chain, sypply chain.

What if every karma was a record with category karma? every lego piece was a Record underneath? I could make karma be a use of records. I thought that because Karma could use a category, so we run only certain categories of Karma. That way i can keep one db, and use my normal records for one thing and other records for rules 











Karma is a rule-based state-transition system made of guarded commands.

  A karma rule looks like:

  if condition(state) then consequence(state, world)

  Mathematically:

  guard predicate  ->  state transition / side effect

  So the best fields to study are not only geometry, but mainly logic, transition systems, rewriting systems, and formal methods.

  Your Geometry Intuition
  If you have one record quantity:

  quantity < 1

  then the state space is one-dimensional: a line, not a plane.

  q ∈ R

  The rule divides that line into two regions:

  q < 1
  q >= 1

  The boundary is the point:

  q = 1

  If you involve more records:

  x = record_a_quantity
  y = record_b_quantity
  z = record_c_quantity

  then the state becomes a vector:

  state = (x, y, z)

  Now you are in a higher-dimensional state space.

  A condition like:

  x < 1

  divides 3D space into two half-spaces. The boundary is:

  x = 1

  That boundary is a hyperplane.

  More generally:

  2x + 3y - z < 10

  also defines a half-space, with boundary:

  2x + 3y - z = 10

  That is also a hyperplane.

  But if your conditions are nonlinear:

  x * y < 10
  x^2 + y^2 < 5

  then the boundaries are not hyperplanes anymore. They are nonlinear regions.

  So, geometrically, karma rules can be seen as partitioning a state space into regions.

  But Karma Is More Than Geometry
  The moment rules can change quantities, create records, delete records, execute shell commands, or trigger other rules, the better model becomes:

  state --rule/action--> new state

  That is a state-transition system.

  Example:

  if health < 1:
      status = "dead"
      run("notify-player")

  This is not just dividing space. It is defining how the system evolves.

  A karma engine is therefore close to several mature areas:

  1. Predicate Logic
  Rules have conditions.

  quantity < 1
  record.type == "task"
  owner == current_user
  created_at < now - 7 days

  These are predicates: statements that are true or false about a state.

  Study:

  propositional logic
  first-order logic
  Boolean algebra
  predicate calculus

  This helps you make rule conditions precise, composable, and optimizable.

  2. Constraint Systems
  A condition is also a constraint.

  quantity < 1
  priority >= 5
  status in ["open", "blocked"]

  A set of rules creates a constraint landscape.

  Study:

  constraint satisfaction problems
  SMT solvers
  SAT solvers
  Presburger arithmetic
  linear constraints

  This becomes useful if you want karma to answer questions like:

  Can these rules ever fire together?
  Can this record reach this state?
  Are these two rules contradictory?
  Is there a state where no rule applies?

  3. State Machines and Transition Systems
  Once rules mutate records, you are describing transitions.

  State A -> State B -> State C

  Study:

  finite state machines
  labeled transition systems
  automata theory
  graph theory

  This helps you reason about workflows, loops, dead ends, and reachable states.

  4. Rewriting Systems
  If karma rules transform records, they are also like rewrite rules.

  record(quantity: 0, status: "alive")
  =>
  record(quantity: 0, status: "dead")

  Study:

  term rewriting
  graph rewriting
  rewrite systems
  rewriting logic

  This is probably one of the most relevant fields if karma is meant to transform structured records.

  Key concepts:

  termination: do rules eventually stop?
  confluence: do rules reach the same result regardless of order?
  normal form: when is a record fully processed?
  critical pairs: where can rules conflict?

  These are exactly the kinds of problems a serious rule engine runs into.

  5. Production Rule Systems
  In AI and expert systems, this style is called a production system.

  condition -> action

  Examples:

  CLIPS
  Drools
  Jess
  Rete-based rule engines

  Study:

  production rule systems
  expert systems
  Rete algorithm
  forward chaining
  backward chaining
  agenda scheduling
  rule priority

  This is the most practical lineage for building a rule engine.

  Important concepts:

  which rules are active?
  which rule fires first?
  can one rule trigger another?
  how do you avoid infinite loops?
  how do you efficiently match rules against many records?

  6. Operational Semantics
  If karma can execute shell commands, send messages, write files, or interact with the outside world, you need semantics for side effects.

  Study:

  operational semantics
  small-step semantics
  big-step semantics
  effect systems
  process calculi

  This helps you define precisely what a rule means.

  For example:

  Does the state change before or after the shell command?
  What happens if the command fails?
  Are actions atomic?
  Can two rules run concurrently?
  Can a consequence be rolled back?

  Without a formal execution model, the system becomes unpredictable as it grows.

  7. Formal Verification
  If you want karma to become reliable, study formal methods.

  Study:

  model checking
  temporal logic
  invariants
  abstract interpretation
  TLA+
  Alloy

  This lets you ask:

  Can this bad state ever happen?
  Will this desired consequence eventually happen?
  Can these rules deadlock?
  Can this rule fire forever?
  Is this invariant always preserved?

  For example:

  Invariant: quantity is never negative.
  Invariant: deleted records cannot be modified.
  Invariant: shell commands only run after approval.

  That is how you bring karma from “rule scripts” toward a mature system.

  Best Name For Karma
  If I had to name it precisely:

  Karma is a guarded, side-effectful, rule-based transition system over a structured state space.

  Or more practically:

  Karma is a production rule engine with state transitions and external effects.

  Or more mathematically:

  Karma defines a transition relation over a state space, where each transition is guarded by predicates.

  The core abstraction is:

  State space S
  Rules R

  Each rule r ∈ R has:

  guard: S -> Bool
  action: S -> S, or S -> Effects -> S

  So a rule is:

  r = (guard, transition)

  And the whole karma system defines:

  S -> S

  or, if nondeterministic:

  S -> P(S)

  meaning from one state, many next states may be possible depending on which rule fires.

  What To Study First
  If your goal is to improve karma as an actual engine, I would study in this order:

  1. Predicate logic and Boolean algebra

  This gives you a clean foundation for conditions.

  2. State machines and transition systems

  This gives you the model for record changes.

  3. Term rewriting / graph rewriting

  This gives you a mature theory for rule application, conflicts, termination, and confluence.

  4. Production rule systems and the Rete algorithm

  This gives you practical implementation techniques for efficient rule matching.

  5. Constraint solving / SMT

  This lets karma reason about rules instead of just executing them.

  6. Formal verification / temporal logic

  This lets karma prove safety properties and detect dangerous behavior.

  The Practical “Next Level”
  The most useful next step is to define karma formally as something like:

  type State = collection of records

  type Rule = {
    name: String,
    guard: Predicate<State>,
    action: Transition<State>,
    priority: Number,
    effects: List<Effect>
  }

  Then specify:

  How guards are evaluated
  How matching records are selected
  Whether actions are atomic
  Whether rules run once or until fixpoint
  How conflicts are resolved
  How external commands are sandboxed
  How failures are handled
  How cycles are detected

  The mature field you probably want to lean on most is:

  rewriting systems + production rule systems + formal methods

  The geometry/hyperplane idea is correct for understanding the shape of conditions over numeric records, but the deeper model for karma is rules as guarded
  transitions over state.