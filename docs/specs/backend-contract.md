# Backend contract

Status: **frozen**, 2026-07-31. Every normative rule cites a gate in
`conformance/backend/tests/gates.rs`. Section 5 records what was open,
how each item closed, and what is deliberately still absent.

## 1. What a backend is

A backend turns a published state into output: an SVG document, a display list
for an immediate-mode rasterizer, a paginated PDF, an accessibility tree, a
plain-text export.

It is an ordinary consumer. It reads a `Snapshot`, optionally a `Delta`, and
nothing else — no frontend, no adapter, no derivation engine, no session.
[`a_backend_renders_from_the_snapshot_alone`]

Three stand-in targets exercise different shapes, because a single target
proves only that the IR fits that target:

- **retained and declarative** — needs stable identity to patch against;
- **immediate-mode** — needs a flat draw list and a region to invalidate;
- **paginated and write-once** — needs to know where a record landed and to
  share resources across the document.

## 2. What is settled

**Identity is the patch key.** A retained target keys its elements by address,
so a delta patches in place rather than rebuilding.

**Parts buy real work at the backend, not only at the IR.** A paint-only change
emits one element and forces no re-layout, against a full render that lays out
every record. This is the same economy the IR's part vocabulary claims,
measured one layer further out.
[`a_paint_only_change_repaints_without_relayout`]

**Incremental output is proportional to the delta.** One edited record in two
thousand repaints one record and invalidates a bounded region, not the
document. [`incremental_output_is_proportional_to_the_change`]

**Differently shaped targets agree.** Two targets over one snapshot draw the
same records, so the snapshot rather than the target decides what exists.
[`two_backends_over_one_snapshot_draw_the_same_records`]

**A move is not a content change.** A translation publishes nothing, and the
new position comes from the placement query. A backend that stores positions
replays the move; one that queries pays nothing.
[`a_move_repaints_position_without_reissuing_content`]

**Pagination and resource sharing have somewhere to live.** Records carry a
fragmentation value and the `Resource` space exists.
[`a_paginated_target_can_place_records_and_share_resources`]

## 3. What a backend must not do

- Depend on a frontend, an adapter, or the derivation engine.
- Require a delta. A backend that renders from the snapshot alone must reach
  the same output.
- Infer identity from position, draw order, or content.
- Treat a `Descendant`-only entry as a content change.

## 4. What a backend is entitled to read

Everything the snapshot publishes: every part of every record, and placement.

The apparent contradiction — the IR contract disclaiming layout and paint while
carrying `Placement` and a `Paint` part — was a wording error, not a design
one. **The IR does not *implement* layout, shaping, or paint policy; it
*publishes* their results.** Something upstream of publication runs layout;
what a snapshot holds is the outcome. A backend cannot draw without that
outcome, so withholding it would mean either that the IR is not a publishable
state or that every backend needs a private side channel to the layout engine.

The line that does hold is the one about algorithms. The IR fixes no line
breaker, no shaper, no font fallback order, no colour management. Two
publishers may produce different results from the same source and both be
valid; a backend reads whichever it was given.

## 5. What was open, and how it closed

Four gaps were found by building the three targets. Each is now a capability
with a gate, and the two that were asserted as limitations had to be rewritten
to close — which is why they were written that way.

1. **A record names the resources it draws with, by role.** A record carries
   typed references, and the role decides which projection the reference feeds:
   a font feeds `Shaping`, an image or gradient feeds `Paint`. They are not
   children, because routing a font through `children` would put it in the
   record's structural projection and its subtree, making a font swap a
   structural change that invalidates every ancestor.
   [`a_record_names_its_resources_without_them_becoming_children`]

2. **Placement answers a rectangle**, with every translation covering the
   address folded in by the query. Both axes, so a rasterizer has a bound to
   invalidate and a retained target has somewhere to place an element.
   [`placement_answers_a_two_dimensional_box`]
   [`a_move_repaints_position_without_reissuing_content`]

3. **`Fragmentation` carries the fragments a record occupies**, in order. A
   record that spans a page break appears on each of them, which is why it is a
   list rather than an index.
   [`a_paginated_target_can_place_records_and_share_resources`]

4. **`Paint` is a colour every target can draw and compare.** Rich paint —
   gradients, patterns, images — is a `Resource` the record names, which is how
   this stays small instead of growing into a paint model the IR would then
   own. The two capabilities interlock: item 1 is what lets item 4 be simple.

### Still absent, deliberately

**Transform and clip.** An ancestor transform is folded in by the placement
query, so a backend never composes one itself. Clip is absent, and the
consequence is stated rather than hidden: without it a repaint region is
*conservative* — larger than necessary — which costs redraw and not
correctness. Adding clip means adding the region-intersection rule and its
gate in the same change.

## 6. Consequence for the IR contract

`composition-ir.md` was marked frozen before any backend existed. That was
premature: three of its claims about consumers — that a backend can drive from
a snapshot, that parts pay off at the output layer, that a move costs no
repaint — were unexercised, and four things a backend needs were missing.

The lesson holds even though the gaps closed: **a claim about a consumer that
no consumer has exercised is a hypothesis.** Two of the four were invisible
from inside the IR and appeared the moment something tried to draw. A fifth
target — an accessibility tree, a plain-text export, an incremental search
index — should be expected to find more, and the contract should be reopened
when it does rather than defended.
