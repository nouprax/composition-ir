# Derivation contract

Status: **freeze candidate**, 2026-07-31. Every normative rule is checked by a
gate in `crates/composition-derive/tests/gates.rs`; the gate name appears in
brackets.

Companion: [`composition-ir.md`](composition-ir.md) defines the published state
and its delta. This document defines what the engine does *behind* that
surface, and it is separate for a reason stated in §1.

## 1. This is not a consumer API

Derivation is internal. Nothing here is published, no consumer registers
anything, and a consumer cannot observe whether a value was recomputed or
reused. If any of that changes, this stops being an implementation detail and
becomes the producer-held consumer registry that
[`composition-ir.md`](composition-ir.md) §5.5 forbids.

The distinction is worth naming precisely, because the two look alike and only
one is legitimate:

- **A derived cell** is the engine's own computation over its own state. It
  records what it read so the engine can decide whether its own cached answer
  is still valid. That is what this document specifies.
- **A consumer registry** is a consumer telling the producer what it cares
  about, so the producer can tell the consumer what to do. That duplicates a
  map the consumer already has, cannot beat `O(|diffs|)`, and is forbidden.

Same machinery, opposite direction of ownership.

## 2. Observations

A derived computation must read authoritative state only through a recording
reader. What it records is closed:

```text
Obs = Read(Address, Part)
    | Liveness { Address address, bool was_absent }
```

`Read` is a positive dependency on one projection of one record. `Liveness` is
a dependency on whether an address is live at all.

**`Liveness` carries which way it was observed, and that is load-bearing.** A
computation that concluded something *because an address was absent* goes stale
when it appears; a computation that concluded something *because it was
present* goes equally stale when it is removed. An algebra that records only
the absent direction reports the second cell as still valid, and the result is
a wrong answer with no error — the failure mode incremental systems are worst
at detecting.

This is not hypothetical. The first implementation of this layer recorded only
absence, passed three hand-written gates, and was caught by the randomized
equivalence gate of §4.

## 3. Survival

A cached execution survives a commit if and only if no observation it recorded
was disturbed:

```text
Read(a, p)                  survives  ⟺  no diff entry names `a` with part `p`
Liveness { a, was_absent }  survives  ⟺  (!after.contains(a)) == was_absent
```

A cell that does not survive is dropped and recomputed on next demand. Nothing
is recomputed eagerly: invalidation removes, it does not rebuild.

## 4. Correctness: no false negatives

The law is equivalence with a fresh build:

> after any sequence of commits, every live cell's value equals what its recipe
> would produce when run against the current snapshot from scratch.

This is checked by running both over randomized edit traces, not by inspection.
[`invalidation_has_no_false_negatives`]

False *positives* — dropping a cell that would have produced the same value —
are permitted and cost only recomputation. False negatives are silent
incorrectness and are not.

## 5. Pruning: the part granularity earns its place

Invalidation is keyed by `(Address, Part)`, not by address. A commit that
changes only `Paint` must not invalidate a computation that read only
`IntrinsicLayout` from the same address, and recomputation must be proportional
to affected readers rather than to all readers.
[`a_paint_change_does_not_invalidate_a_measurement_reader`]

A commit entirely outside every read set must invalidate nothing.
[`an_unrelated_edit_invalidates_nothing`]

This is what the `Part` vocabulary of [`composition-ir.md`](composition-ir.md)
§5.2 is *for*. The delta reuses it rather than introducing a second one, so
there is one closed set of projections and one meaning for each, whether it is
being used to prune the engine's own recomputation or to keep a consumer's
update proportional.

A liveness observation is separately checked when the address appears.
[`a_conclusion_drawn_from_absence_is_invalidated_when_the_address_appears`]

## 6. Cost

- Invalidation is proportional to the observations that the commit's diff
  actually intersects, never to the number of live cells.
- A cell's recorded observations are proportional to what it read.
- Recomputation is demand-driven: a dropped cell that is never asked for again
  costs nothing.

An implementation must not meet these by conservatively invalidating
everything, nor by an all-cells sentinel presented as exact discovery.

## 7. Open before freeze

1. Cycle handling: whether a derived cell may read another derived cell, and if
   so how a cycle is detected and reported rather than looped.
2. Whether observations may be *ranged* — "no address in this ordered interval"
   — which the current point-wise algebra cannot express, and which an ordered
   query would need.
3. Eviction policy under memory pressure, which must remain unobservable in
   every rule above.
4. Concurrency: whether cells may be computed in parallel, and what that
   implies for the reader's recording discipline.
