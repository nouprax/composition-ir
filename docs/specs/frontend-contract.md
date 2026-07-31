# Frontend contract

Status: **freeze candidate**, 2026-07-31. Every normative rule is checked by a
gate in `crates/frontend-conformance/tests/gates.rs`; the gate name appears in
brackets.

This document states what Composition IR requires of a frontend. It is short on
purpose, and the shortness is measured rather than claimed: the whole surface
is one projection function, and a gate fails if it grows.
[`the_adapter_is_the_whole_frontend_contract`]

## 1. What this document is not

It does not define, constrain, or version a frontend's AST. There is no
universal node inventory, no required parse semantics, and no requirement that
a frontend expose Composition records from its own API. A frontend owns its
source bytes, its syntax, its language semantics, its identities, and its
repository.

The reason is structural, not stylistic. A contract that re-specifies a
frontend's AST breaks every time that frontend evolves, and it puts one
repository in the position of freezing another's public surface. If a rule
here would be invalidated by a frontend changing a node kind, the rule is in
the wrong document.

A frontend must not depend on Composition IR, and Composition IR must not
depend on a frontend. The seam is an adapter that depends on both, and it is
the only place the two vocabularies meet.
[`the_ir_has_never_heard_of_a_frontend`]

## 2. What a frontend must provide

Everything below is stated in terms of the frontend's own concepts. How it is
spelled is the frontend's business.

### 2.1 Stable identity

Each node a frontend intends to be continuous across edits must carry an
identity that:

- survives an edit elsewhere in the document;
- is never reused after the node is retired, even if an equal value returns;
  and
- is positive, so it maps onto an IR ordinal without an offset convention.

Identity continuity is a frontend judgement — it knows whether a reparsed
region is the same logical node. The IR does not second-guess it, and an
adapter must not invent one from position, order, or content hash.

### 2.2 Projectable values

For each node, whatever the frontend already knows that maps onto the IR's
parts: its text, its kind, its scalar attributes, its ordered children.

A frontend does not have to expose a value for every part. A part it never
populates simply never differs, and therefore never appears in a diff.

### 2.3 A join key, never a coordinate

A frontend must expose a stable key by which a consumer can ask the *frontend*
about a node — for source spans, line numbers, or anything else in the
frontend's own coordinate space.

It must not hand absolute coordinates to the adapter for storage as record
values. Carrying a span as a value makes every later record change on any
earlier edit, because the IR would be republishing a derived position as if it
were content. Side-by-side works from the key: the diff names the record, the
key names the node, and the frontend answers the span.
[`side_by_side_joins_source_and_preview_across_the_boundary`]

### 2.4 Unresolved cross-unit references

A reference to another unit is classified and carried as an authored string.
The frontend must not open, read, or expand the target, and nothing
target-derived may appear on the node. Two shapes qualify:

- **Named** — an embed, an import, an include. The node carries the reference.
  [`a_target_commit_never_touches_its_host`]
- **Inline** — a span written in a grammar the frontend does not speak: a LaTeX
  or Typst formula in Markdown, a fenced block destined for another parser. The
  node carries the span's own bytes.
  [`a_foreign_grammar_span_is_carried_unparsed`]

The second is a unit despite sharing a source buffer with its host, because a
unit boundary is **the smallest thing that rebuilds independently**, not a
file. A formula's only input is its own bytes.

Two consequences follow, and they are why this is not a delegation mechanism.

A host frontend that parsed the span itself would take on a dependency on every
grammar its documents might embed. Carrying it instead means a new math engine
is a new frontend, and the host does not change.

And incrementality comes free rather than needing a cache. The host publishes a
diff on its formula record exactly when the formula's bytes change, so a
consumer re-runs the math engine then and never otherwise — there is no cached
layout to invalidate and no memoization to get wrong, because nothing here ever
held one.
[`the_host_signals_a_formula_rebuild_exactly_when_its_bytes_change`]

Composing units is downstream, for the reasons in
[`composition-ir.md`](composition-ir.md) §8. A commit on one unit produces
nothing on another — including which units to resolve at all, and with what,
which is consumer policy: rendering a formula, showing its source, or exporting
plain text are all valid readings of the same snapshot.

### 2.5 A change report

A frontend should report which of its identities one reparse touched and which
it retired.

This is the only requirement that exists purely for cost, and the cost is not
marginal. Adapting a 2000-line document after a one-line edit touches four
records with the report and every record without it, while both routes produce
an identical snapshot.
[`a_frontend_delta_is_what_makes_adaptation_proportional`]

So it is a **should**, not a **must**: a frontend that cannot report changes
still adapts correctly, at a cost linear in the document per commit. A frontend
that intends to support editing will want it.

The report is a list of identities. It is not a delta in the IR's shape, does
not carry parts, and does not have to be ordered — the IR derives its own diff
from the two snapshots, so a report that is conservatively too large costs
recomputation and never correctness.

## 3. What the adapter does

The adapter is expected to be small enough to read in one sitting. In the
conformance fixture, the function that says what the IR needs — the projection
— is under thirty lines, and the whole adapter including both the incremental
and the linear route is under a hundred.

Its work is: map frontend identity to an IR address; project each touched node
into record values; remove the retired ones; declare placement order. It does
not diff, because the IR's membership law does that from the two snapshots.
[`an_edit_crosses_the_boundary_as_parts_not_as_frontend_concepts`]

An adapter may live in the frontend's repository, in a consumer, or on its own.
Nothing in this contract depends on where it is compiled.

## 4. What a frontend must not do

- Depend on Composition IR, or expose IR types from its own API.
- Resolve a cross-unit reference.
- Store absolute coordinates in the IR as record values.
- Assume the IR will report changes in the frontend's own vocabulary. The diff
  names IR addresses and IR parts; translating back is the adapter's job or
  the consumer's, through the join key.
- Require a consumer to read a delta. A consumer that ignores every delta and
  re-derives from the snapshot reaches the same state.
  [`a_consumer_that_ignores_every_delta_agrees_with_one_that_does_not`]

## 5. Conformance

`crates/frontend-conformance` is the executable form of this document: a
fixture frontend providing exactly the surface above, its adapter, and the
gates. A real frontend is expected to be checkable by the same gates with its
own fixture swapped in, which is the intended acceptance path for
`markdown-core` and `tex-core`.

## 6. Open before freeze

1. Whether an adapter may declare additional address spaces, or whether `Space`
   stays closed to the set `composition-ir.md` §3 defines.
2. How a frontend reports identity continuity it *could not* preserve, so a
   consumer can distinguish "this node changed" from "this node is a different
   node that happens to occupy the same place".
3. Whether the change report should carry the frontend's own notion of which
   part changed, or whether deriving parts from the snapshots is always
   cheaper than transporting them.
4. Multi-source frontends: a single frontend unit that owns several source
   buffers, and whether that is one domain or several.
