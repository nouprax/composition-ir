# Composition IR contract

Status: **frozen**, 2026-07-31, across the frontend, derivation, ABI, and
output boundaries — the last of which reopened it once already. Building three
backends found four things they needed that were not here, all now closed and
recorded in [`backend-contract.md`](backend-contract.md) §5. Freezing before a
consumer existed was the error, and the rule it produced is that a claim about
a consumer nothing has exercised is a hypothesis.

Every normative rule below is checked by a gate in
`packages/composition-ir/tests/gates.rs`,
`packages/composition-ir-ffi/tests/gates.rs`, or
`packages/composition-ir-ffi/tests/layout_gates.rs`; the gate name appears in
brackets. A rule with no gate is not normative.

Companions: [`derivation.md`](derivation.md) for engine-internal recomputation,
[`frontend-contract.md`](frontend-contract.md) for what a frontend supplies,
[`backend-contract.md`](backend-contract.md) for what an output target reads.

The words **must**, **must not**, **should**, and **may** are normative.

## 1. What this owns

Composition IR owns one immutable published state per commit, and one exact
description of how two published states differ.

```text
commit -> Commit { Snapshot snapshot, Delta delta }
```

`snapshot` is the complete correctness result. `delta` is a discardable
exact-base acceleration. A consumer that ignores every delta and re-derives
from `snapshot` reaches the same state, and no binding may require a delta to
obtain, retain, walk, or compare a snapshot.

It does not own source bytes, syntax, or language semantics; it does not
implement layout, shaping, line breaking, font fallback, or colour management;
and it owns no workspace, network client, or consumer state.

It does *publish* the results of layout and paint, because a backend cannot
draw without them. Not implementing an algorithm and not carrying its outcome
are different claims, and only the first one holds here
([`backend-contract.md`](backend-contract.md) §4).

## 2. Integration paths

There are three ways to consume a commit. They see the same snapshot and the
same identities, none is a fallback for another, and all three must be
supported.

**A — hand over the snapshot.** The consumer reads no delta. A value-diffing
framework computes its own update directly from the IR. Its cost is `O(sum of
sibling counts along the dirty spine)`, with every comparison `O(1)`; a
virtualized container reduces it to the visible window.

**B — apply the delta to consumer-owned state.** The consumer maintains state
that must be updated in place rather than re-derived: a display list, a DOM or
view hierarchy, a measurement cache, an inspector model, a search index, a
binding's own platform value tree. It walks `diffs` and edits its own
structure.

**C — read the delta for location only.** The consumer keeps no mirror and only
needs to know *where* the state changed: a side-by-side editor highlighting the
preview region a keystroke affected, a telemetry probe, a test assertion.

B is why `Delta` is public. A is why it is optional.

## 3. Identity

```text
Domain          // opaque; compared for equality, never read
Revision        // positive, strictly monotonic within one domain
SnapshotVersion { Domain domain, Revision revision }

Id      { Domain domain, Ordinal ordinal }
Space   = Node | Frame | Source | Origin | Destination | Resource
Address { Space space, Id id }
```

A `Domain` is the scope that identities and revisions live in. It includes
every input that can affect IR truth — schema, frontend profile, options.
Changing one starts a fresh domain; that is not an ordinary revision.

The pair in `SnapshotVersion` is load-bearing. A bare counter cannot reject a
delta from a different lineage that happens to carry the same number, and that
failure is silent: the consumer would apply addresses from one state to
another. Passing an address from a foreign domain is a programmer error and
traps; it is not a result value. [`a_foreign_domain_address_traps`]

An `Id` is never reused after retirement. Delete and reinsert allocates a fresh
ordinal even if the value returns.

There is one revision scalar. Nothing has a private revision space, and no type
exists for a per-record, per-field, or per-edge revision.

## 4. Snapshot

Every published `Snapshot` must be immutable, self-contained when returned,
safe for concurrent reads, independent of later commits, and structurally
shared with adjacent revisions.

Concurrent readability is a property of the type, not a convention, and it is
asserted as one: the persistent structures underneath have reference-counted
variants that are *not* thread-safe by default, so this is exactly the rule
that decays silently if nothing checks it.
[`a_published_snapshot_is_readable_from_any_thread`]

**An unchanged subtree is the same instance in the next snapshot**, not merely
an equal one, so a consumer's identity short-circuit fires before any field is
read, and retaining the previous snapshot to diff against costs the changed
frontier rather than a second tree. [`an_unchanged_subtree_is_the_same_instance`]

A canonical no-op publishes nothing: it reuses the exact current snapshot and
advances no revision. Writing a value equal to the current one is a no-op.
[`a_canonical_no_op_reuses_the_exact_snapshot`]

### 4.1 What this requires of the record store

Instance sharing across simultaneously live revisions is a constraint on the
storage shape, not only on the API. A flat contiguous store addressed by index
gives the best locality but cannot satisfy it: publishing a revision would copy
the whole store, so no two revisions could share a record by instance.

The store must therefore be **persistent and chunked** — contiguous leaves, so
traversal is not a chain of dependent loads, and a path-copied spine, so an
untouched leaf is reachable by instance from every revision that still holds
it. Publishing copies the spine, which is `O(log n)`, and nothing else.

Flattening the store into a single vector for cache reasons is a proposal to
stop sharing records, which contradicts §4 and is visible to
[`an_unchanged_subtree_is_the_same_instance`]. Argue it on those terms or not
at all.

## 5. Delta

```text
Delta { SnapshotVersion before, SnapshotVersion after, [Diff] diffs }
Diff  { Address address, Parts parts }
```

That is the whole update surface. There is no per-component delta, transition,
rebuild plan, or damage record: each of those is one `Space` and one part of
this list.

### 5.1 Membership is a law

For every address live in `before` or `after`:

```text
address ∈ diffs  ⟺  proj_before(address) ≠ proj_after(address)

parts(address)   =  { p : proj_before(address).p ≠ proj_after(address).p }
                    restricted to the parts the address has in `after`
```

Everything else is a consequence, not an additional rule.
[`membership_law_is_a_pure_function_of_the_two_snapshots`]

- **A retired address has no parts in `after`, so its `parts` is empty.** That
  is why no lifecycle tag exists: a consumer distinguishes the case by looking
  the address up, which returns nothing for exactly those addresses. A created
  address differs from absence in every part it has.
  [`a_retired_address_carries_no_parts_and_a_created_one_carries_all_it_has`]
- **A pure placement move emits nothing**, because position is not a
  projection (§6).
- **Private cache, index, or storage maintenance emits nothing**, because it
  changes no projection.
- **`diffs` is a pure function of (`before`, `after`).** Two commits reaching
  the same state from the same state produce identical `diffs`, whatever route
  the producer took.

### 5.2 Parts, and why exactly these

`Parts` is closed, and the rule that closes it is a cost rule, not a taxonomy:

> a part exists if and only if a consumer that ignored it would either be
> wrong, or pay more than `O(1)`.

```text
Part = Structure         // child identity sequence           O(width)
     | Text              // canonical text bytes              O(length)
     | TextMap           // raw-source to canonical mapping   O(length)
     | Shaping           // font/script resolution            O(length)
     | IntrinsicLayout   // measurement independent of space
     | LineLayout        // breaking within available space
     | Fragmentation     // distribution across regions
     | Paint             // colour, decoration, opacity       O(1)
     | Interaction       // targets and destinations          O(1)
     | SourceLink        // origin and provenance joins       O(1)
     | Validation        // validation outcome                O(1)
     | Descendant        // nothing of its own; something below it
```

The `O(1)` parts are separate because a dependency layer prunes recomputation
by them; the costly ones are separate because a consumer's update must stay
proportional to the change. A paint-only change on a container with `W`
children emits `Paint` alone, so a consumer updates one field instead of
reprojecting a `W`-wide child list — a distinction an added/removed/changed
triple cannot express. [`a_paint_only_change_on_a_wide_container_emits_paint_alone`]

`Descendant` is the one part justified by wrongness rather than cost: a
consumer that ignored it while materializing a parent-linked structure would
retain a stale child. An ancestor whose own record is byte-identical still
appears, carrying `Descendant` alone, and masking that part recovers the
record-only reading of "changed" — so both readings come from the one list.
[`an_ancestor_whose_own_record_is_untouched_carries_descendant_alone`]

### 5.3 Discovery must walk up

`Descendant` must be discovered by walking **up** from the changed frontier,
never by testing a container's children. A retired child seeds that walk
exactly as a changed one does. Deciding a container's `Descendant` by comparing
its child list costs `O(width)` per container and breaks the bound of §7.

Record pointer equality is **not** a subtree short-circuit. Children are
referenced by address, so an untouched parent record says nothing about what
its children now resolve to; it only lets the own-part comparison be skipped.

A record reaches the frontier only if its own projection actually differs. An
edit that resolves to no change must not seed the walk, or a no-op publishes an
ancestor spine.

### 5.4 The whole precondition

```text
delta.applies_to(my_base)  ⟺  delta.before == my_base
```

A stale, skipped, wrong-domain, wrong-schema, or replayed delta fails that one
comparison, and the answer is always the same: rebuild from the self-contained
snapshot. There is no fallback reason enumeration and no acknowledgement round
trip. [`one_comparison_is_the_whole_precondition`]

Deltas compose by concatenation, which is a sound over-approximation: an
A → B → A address appears in both halves and in neither side of the direct
difference. Applying a superset is correct because an entry names an
**address** and never a value or an operation, so the consumer re-reads the
current snapshot at each address.

### 5.5 What the delta must not carry

Composition IR does not model, version, address, or validate consumer state. It
therefore emits **no consumer registry, route, interest, target registration,
edit program, contract-local operation, acknowledgement, nonce, advance plan,
or reconciliation directive**. A consumer holds its own map from address to its
own state; that map is the only index the update needs, and the consumer
necessarily already has it. Discovery through a producer-held registry cannot
beat it: both must visit the changed frontier, so both are `O(|diffs|)`.

## 6. Placement is a query

Absolute placement is resolved on demand in `O(log n)` over the placed
population. A move that touched no record therefore contributes no entry, and a
consumer that queries placement pays nothing for it.
[`placement_is_a_query_so_a_move_emits_nothing`]

A compact move aggregate is available for the consumer that has already
materialized absolute geometry into its own state and must replay the move
rather than re-query it. It is never a per-record delivery, and it is optional
for the same reason: a consumer that never denormalized has nothing to replay.

There is no coordinate event family, remap contract, or side channel.

## 7. Cost

- `|diffs|` is the changed frontier plus its deduplicated ancestor spine,
  bounded by `O(changed * depth)`, independent of document width and size.
- Applying a delta is `O(|diffs|)` plus the consumer's own projection cost.
  The second term is the consumer's and must be reported separately; a small
  diff list must never be advertised as constant total work.
- Path A's cost is stated in §2 and is measured against that statement, never
  against `|diffs|`.

An implementation must not meet the first bound with a whole-state scan hidden
inside commit, a sentinel entry standing in for exact discovery, or a
constant-size token whose required expansion is counted nowhere.

## 8. Composition is downstream

One document is one build unit. Cross-unit references are parsed and classified
by the frontend but never resolved here, so what this IR carries is the
*unresolved edges* of the cross-document graph. Resolving, loading, splicing,
cross-unit numbering, cycle detection, and cross-unit invalidation belong to
the layer that owns the resolver.

Materializing an imported subtree into a host state breaks three things at
once: fresh-build equivalence, because a rebuild from the host's own inputs
would not contain those records unless building also read another file;
identity, because imported records belong to another `Domain` and admitting
foreign domains ends the single-domain assumption §3 rests on; and placement,
because imported records have no position in the host's own coordinate space.

A proxy record, a forwarding identity, or a synthetic address invented to paper
over the third point is a symptom of the boundary being in the wrong place, not
a mechanism.

## 9. C ABI

The boundary is C, and crossing it costs nothing. Every public value type here
is `#[repr(C)]`, so a consumer reads the IR's own allocation: a delta crosses as
a pointer and a length, never as a per-entry conversion or a marshalling
buffer. Returning a converted copy would make every commit pay for a consumer
that might not read it.
[`the_value_types_are_abi_stable_and_pod`]
[`crossing_the_boundary_allocates_nothing_per_entry`]

`Domain`, `Revision`, and `Parts` are transparent over their scalars; `Space`
and `Part` are one byte; `Address` and `Diff` are fixed-layout aggregates with
no drop glue. A change that gives any of them a niche, a discriminant, or a
pointer silently reintroduces the copy, which is why their sizes are asserted
rather than assumed.

Snapshots and deltas cross as opaque retained handles. Retaining is explicit
and reference-counted, releasing one handle does not invalidate another, and a
handle is the only thing a caller must remember to release.
[`handles_are_retained_and_released_independently`]

Calls are named `cir_<subject>_<verb>`, the subject being the type the call
operates on or produces.

### 9.1 The layout manifest

Sizes are asserted above, but a size is not a layout. Swapping two fields of a
`#[repr(C)]` struct changes no size, and every consumer that reads by offset —
a Swift `UnsafeBufferPointer`, a Kotlin `ByteBuffer`, a JavaScript `DataView`
over `WebAssembly.Memory` — would go on reading at the old addresses, with no
error anywhere. A non-Rust binding is uniquely exposed to this because it
cannot ask the compiler where a field is.

So the layout is **published as an artifact** rather than left to each binding
to rederive. `packages/composition-ir-ffi/abi/layout.json` carries every ABI
type's size, alignment, field offsets, enum discriminants, and the `Parts` flag
values, generated from the compiled types. Bindings are generated from that
file. It is committed, so a layout change shows up as a diff on it rather than
as behaviour in the field, and it is checked against the compiler on every run.
[`the_committed_layout_matches_the_compiled_one`]

The manifest must stay complete as the ABI grows, which is how it would
otherwise rot: a type added here that nobody thought to describe there.
Completeness is checked against the IR's own source rather than against
anyone's memory. [`every_abi_type_the_ir_publishes_is_in_the_manifest`]

It carries two things an offset table cannot express by itself. `Domain`,
`Revision`, and an `Id`'s ordinal are niche-optimized — zero is not a value
they can hold, and a binding that writes one has produced something Rust
considers impossible — so the manifest marks them. And `Parts` publishes its
flag values outright instead of leaving every binding to rederive
`1 << discriminant`. [`parts_flags_agree_with_part_discriminants`]

**A manifest describes one ABI, and says which.** Offsets are not portable:
where `u64` is 4-aligned — Android's `x86` ABI is the one that reaches this
project — `Address.id` sits at 4 rather than 8 and a `Diff` is 24 bytes rather
than 32. So each manifest carries the pointer width, `u64` alignment, and
endianness it was generated under, a binding generator must check that block
against its target rather than assume there is one layout, and a target whose
ABI differs needs its own committed manifest. Regenerating over another ABI's
manifest is refused rather than allowed, because it would leave every binding
built for the first one reading at offsets nothing produces.

## 10. Streaming and long sessions

Streaming is ordinary editing. There is no chunk-shaped record, revision
domain, cache class, or update path: applying an edit as many small commits
reaches the same state as applying it as one.
[`chunked_and_whole_application_reach_the_same_state`]

Retaining history costs the changed frontier per revision, not a copy each. A
long session holds many revisions, and every record none of them touched is one
instance shared by all of them.
[`retaining_many_revisions_shares_everything_untouched`]

Revisions never wrap. Exhaustion is a hard failure rather than a silent
wraparound, because a wrapped revision would make a stale delta look applicable
to the one comparison of §5.4.
[`revision_exhaustion_fails_rather_than_wrapping`]

Storage compaction is private. It preserves every identity, instance, and
value, and it publishes nothing — a consumer cannot tell whether it ran.

## 11. Freeze record

The four items this document originally listed as open are settled:

- **The C ABI** is §9, and its zero-copy claim is measured rather than
  asserted.
- **`Space` is closed**, at one byte and exhaustively matchable. Every space
  proposed so far is a `Node` with a different part populated, and a frontend
  needing a genuinely disjoint identity space uses a separate `Domain`, which
  already gives non-collision without widening the enum.
  [`the_space_set_is_closed`]
- **The derivation layer** has its own contract in
  [`derivation.md`](derivation.md), separate because it is engine-internal and
  must stay that way.
- **Streaming and long-session behaviour** is §10.

Four further items, all about what an output target may read, were opened by
the backends and closed with them: a record now names its resources by role,
placement answers a rectangle, `Fragmentation` carries the fragments a record
occupies, and `Paint` is a colour with rich fills delegated to resources.
Transform is folded in by the placement query; clip is deliberately absent, at
the stated cost of conservative repaint regions.

Extending this contract means adding a rule and the gate that checks it in the
same change. A rule that cannot be checked will be violated silently.
