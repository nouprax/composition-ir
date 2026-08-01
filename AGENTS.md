# Agent instructions

This file is the complete brief for agents that write code here and for agents
that review pull requests. It carries engineering philosophy, the constraints
this repository specifically imposes, and the concrete triggers a reviewer must
act on.

It does not carry product concepts, module inventories, roadmap, or operational
state: this file governs *how* code is written and reviewed, not *what* the
product is. The other two pieces divide the rest between them.

- `docs/specs/` is normative and holds the product concepts, the module
  inventories, and the contracts themselves.
- `docs/direction.md` is not normative and holds what has been decided and why,
  what is next, what is deliberately still open, and the project's operational
  state. Read it before proposing a direction, so a road already closed is not
  reopened without the argument that closed it.

## First-principles engineering

- Every code change must be designed from the full set of current requirements
  and invariants. Do not optimize for the smallest patch, the fewest edited
  lines, or the quickest local fix.
- Prefer the simplest coherent abstraction that makes semantics, ownership,
  lifecycle, failure behavior, and performance explicit. “Minimal” means the
  fewest independent concepts and mechanisms, not the smallest diff.
- One semantic operation should have one data model and one general algorithm.
  A separate path is justified only by a documented semantic, ownership, or
  lifecycle invariant—not by a convenient input shape, a current test, or a
  benchmark result.
- Treat awkward control flow, duplicated state, leaky ownership, magic values,
  mode flags, repair-up callbacks, and exceptions to an abstraction as
  evidence that the model is wrong. Fix the model instead of normalizing ugly
  code around it.
- Do not accept an implementation that creates latent complexity, divergent
  behavior, or maintenance debt for a short-term delivery or benchmark gain.
  If the durable design requires a broader refactor, perform that refactor and
  protect its invariants with tests.
- Before changing an abstraction, audit all of its callers and consumers so
  the result is repository-wide and internally consistent. Remove superseded
  paths rather than leaving parallel legacy and replacement mechanisms.
- Tests must verify semantic invariants and failure boundaries. Passing the
  current examples is necessary but is not proof that the abstraction is
  sound.
- Code review follows the same standard. Reject changes that merely mask a
  symptom, encode an input-shape exception, duplicate an existing model,
  obscure ownership or lifecycle, or buy short-term gains by planting future
  failure modes—even when the patch is small and all current tests pass.

## Performance and complexity

- Performance is a design requirement, not a later patch. Reason about
  asymptotic work, allocation behavior, data locality, and adversarial inputs
  before choosing the abstraction.
- Benchmarks are diagnostic evidence and regression gates; they are not an
  oracle for designing alternate algorithms around the measured examples.
- Do not introduce branches based on benchmark-observed cardinality, input
  size, or a convenient “common case” to recover a local number. Improve
  constant factors by improving the shared algorithm or its data structure.
- Complexity tests must verify the intended general invariant, including
  adversarial shapes that defeat the former implementation. A favorable timing
  result alone is not proof that the implementation is sound.

## Rust performance pitfalls

This codebase replaces a hand-optimized C implementation. Three Rust defaults
cost performance quietly. Each entry below says where it bites, what the fix
is, **when the fix is the wrong call**, and what evidence settles a
disagreement. None of them is a blanket prohibition: applied indiscriminately,
each fix is itself a pessimization.

### Node-graph pointer chasing

*Where it bites.* A graph of individually allocated records linked by pointers,
walked in a hot path. Every step is a dependent load the prefetcher cannot
predict. C alleviates this with an arena.

*The fix.* Hold records in a contiguous backing store addressed by integer
index, in the shape `indextree` or `id-tree` use; parent, child, and sibling
links become `usize`. Locality, safe mutation, and no borrow-checker friction
at once.

*When the fix is wrong.* Outside a hot traversal, indirection costs nothing
worth the indirection of an index. And in this repository the flat form is
ruled out for a stronger reason: `docs/specs/composition-ir.md` §4 requires
many snapshots to stay live and share unchanged records *by instance*, which a
single `Vec` cannot do because publishing would copy it. §4.1 states the
resolution — a persistent chunked store, contiguous leaves for locality and a
path-copied spine for sharing.

*Note on shared pointers.* `Arc` on published records is required here, not a
smell: snapshots are read concurrently, so records must be `Send + Sync`.
`Rc<RefCell<Node>>` is the shape to avoid — it reproduces C pointer soup and
adds runtime borrow checks — but shared ownership as such is the design.

*What settles it.* Whether the structure is on a measured hot path at all, and
then a traversal benchmark. Absent that, leave it alone.

### Bounds checking in tight loops

*Where it bites.* Byte-at-a-time inner loops — scanning syntax, comparing runs.
The check is small, but it also blocks vectorization, which is the larger cost.

*The fix.* Write the loop as an iterator: `.iter()`, `.chunks()`, `.windows()`,
`zip` over two slices. LLVM elides the check when it can prove the index safe,
so idiomatic iteration is usually the *faster* spelling rather than the
safer-but-slower one.

*When the fix is wrong.* In cold code, indexing is often the clearer expression
and the check costs nothing measurable. Rewriting readable indexing into an
iterator chain for a loop that runs once per document is churn. And the
elision is not guaranteed — an iterator rewrite that does not actually change
the generated code has bought nothing.

*What settles it.* A benchmark, or reading the generated code. `get_unchecked`
is the last resort: permitted only when the compiler demonstrably failed, with
the benchmark in the same change, and with a comment stating the invariant that
makes it sound.

### High-granularity allocation

*Where it bites.* A `String` or `Vec` allocated per token or per record during
parsing or projection. Allocator traffic dominates regardless of algorithm.

*The fix.* Prefer borrowing a slice of an existing buffer over copying into a
fresh allocation. Where an owned collection is needed and is short in the
common case, `smallvec` or `tinyvec` keeps it inline until it exceeds a
threshold.

*When the fix is wrong.* Inline capacity inflates the type, which makes every
move more expensive and every enclosing collection larger — a pessimization if
the value is stored in bulk or moved often. Picking a capacity without knowing
the length distribution is guessing. Borrowing is usually the better answer;
reach for a small-vector type when the data must be owned and the distribution
is known.

*What settles it.* Allocations per operation, together with `size_of` for the
type and the actual length distribution. An allocation-count regression
predicts the wall-time regression that appears later on larger inputs, so
measure it directly rather than inferring it from timing.

## Contracts and gates

- `docs/specs/` is normative. Every rule cites the gate that checks it, and a
  gate asserts those citations resolve.
- A rule that cannot be checked by a gate is a rule that will be violated
  silently. Add the gate in the same change as the rule, or do not add the
  rule.
- A contract may be described as frozen only against a green suite.
- Hand-written gates check the cases their author already thought of.
  Randomized equivalence against a from-scratch computation is what catches the
  case nobody thought of; both bugs found so far in this repository were found
  that way and were invisible to three hand-written gates each.
- **A gate you have not watched fail is a hypothesis.** Break the rule it
  claims to check and confirm it goes red, in the same change that adds it. A
  false PASS has turned up every time this was skipped, and it is invisible
  afterwards: a green suite looks identical whether the gate is discriminating
  or inert.
- A check that finds no files passes. Any check that walks a tree must discover
  what it walks and fail loudly on an empty result, never hardcode a path that
  a directory move silently empties.
- **A generated artifact is checked against the thing it describes, not against
  its own generator.** Regenerating and comparing proves the generator is
  deterministic and nothing else. Check against the built output — a symbol
  table, a compiler, a linker — because that is the only statement of what was
  produced that no spelling in the source can hide from. `cbindgen` not
  expanding `macro_rules!` left five exports out of the C header while every
  text comparison agreed.

## Failure modes already proven in this codebase

These are not hypothetical. Each was written, passed review-by-inspection, and
was caught by a gate. Recognizing them is part of the review.

- **A derived position carried as a record value.** An adapter stored a
  frontend's absolute source span on a record, so editing one line changed
  every later record. Coordinates are resolved by query; records carry stable
  join keys.
- **A one-directional liveness observation.** A dependency layer recorded
  "this address was absent" but not "this address was present", so a
  computation that concluded something *because a record existed* survived its
  removal and served a stale answer. Both directions are observations.
- **Consumer state modelled in the producer.** Registries, routes, interests,
  target registrations, edit programs, acknowledgements, and advance plans all
  reappear naturally in this problem domain and are all forbidden: they
  duplicate a map the consumer already has and cannot beat `O(|diffs|)`.
  `docs/specs/composition-ir.md` §5.5 states the rule; §1 of
  `docs/specs/derivation.md` states how to tell the legitimate engine-internal
  version apart.

## Execution environment boundaries

- Treat the sandbox, container, and host machine as distinct execution
  environments. A result observed inside the sandbox is evidence about the
  sandbox only unless the tool explicitly runs with host access.
- Do not infer host credential, network, keychain, GUI, daemon, device, or
  filesystem state from a sandbox failure. In particular, a sandboxed
  `gh auth status` or GitHub network error does not prove that the host GitHub
  CLI is logged out or offline.
- When a task depends on a host integration, use the available host-scoped or
  escalated mechanism to verify it before reporting a blocker or asking the
  user to reconfigure anything. State which environment produced the evidence.
- Never work around sandbox boundaries implicitly. Use the platform's explicit
  approval or connector path, keep the requested authority scoped to the task,
  and distinguish an approval denial from a real host-side failure.

## Review checklist

A principle a reviewer cannot decide is a principle that will not be enforced.
The first list is defects: reject them and say which line applies. The second
is questions: a change that trips one is not wrong, but it may not be approved
until the question is answered.

**Model**

- introduces a second data model or algorithm for one semantic operation
- adds a branch on input shape, cardinality, or a benchmark-favoured case
- leaves a superseded path alive beside its replacement
- adds a nullable or conditionally-meaningless field instead of a sum type
- adds a member the state can already answer (a parent, an ordinal, a
  lifecycle tag, a schema stamp, a count)

**Contract**

- adds or changes a rule in `docs/specs/` without a gate in the same change
- describes a contract as frozen while any suite is red
- adds a consumer registry, route, interest, target registration, edit
  program, acknowledgement, or advance plan to a published surface
- carries a coordinate, offset, or span as a record value rather than
  resolving it as a query

**Evidence**

- a performance claim with no measurement, or a measurement on one shape
  presented as a general result
- a correctness claim resting only on hand-written cases where a randomized
  equivalence check is possible
- a host-versus-sandbox conclusion drawn from the wrong environment

### Ask before approving

Performance shape is a judgement, not a violation. When a change trips one of
these, ask for the evidence the matching section names; accept "not on a hot
path" as an answer.

- shared-pointer indirection introduced into a graph that is walked hot — is
  it hot, and does a traversal benchmark move?
- a flat contiguous store proposed for the record graph — what happens to
  instance sharing across live snapshots?
- indexing in an inner loop — does an iterator change the generated code here,
  or is this cold?
- `get_unchecked` — where is the benchmark showing the compiler failed, and
  where is the soundness comment?
- a per-token owned allocation — can it borrow instead, and if it must own,
  what is the length distribution that justifies the inline capacity?
- a small-vector type applied broadly — what did it do to `size_of` and to the
  cost of moving the enclosing value?
