# Direction

What has been decided, what is next, and what is deliberately still open.

This file is **not normative** — `docs/specs/` is, and `AGENTS.md` says how code
is written and reviewed here. This one exists because neither of those records
*why a road was not taken*, and without that the same arguments get relitigated
from scratch. Every decision below is written with its reason, because the
reason is the part that has to survive.

## The goal

One composition IR, with `markdown-core` and `tex-core` as frontends, consumed
from C, Swift, Kotlin Multiplatform, and WASM/TypeScript. It has to be fast
enough to drive side-by-side editing at keystroke rate and pleasant enough that
a binding is a few hundred lines rather than a project.

`tex-core`'s output is *defined to be* composition IR, so `tex-core` depends on
this repository. `markdown-core` stays C for now.

The three workloads that decide whether a design is right:

1. **SBS live preview** — edit on the left, rendered document on the right, at
   keystroke rate.
2. **Element select → editor** — click a rendered element, put the caret on the
   source that produced it, and the reverse.
3. **Native formulas in Markdown** — LaTeX or Typst inside a Markdown document,
   without either frontend learning the other's grammar.

## Decisions, with their reasons

### One crate on crates.io

`composition-ir` only. The derivation engine is a default feature of it, not a
separate crate: it depends on the IR and nothing else, and nobody reaches for a
derivation engine without the state it derives over.

The FFI shim is `publish = false`. crates.io distributes source for Rust
dependents, and no Rust dependent wants an FFI shim; a C consumer links a built
library. Publishing it taught consumers a dependency edge that does not exist.

### Bindings live in this repository, not their own

The field layout of the C ABI is generated from the Rust build, and every
binding reads it. Split across repositories there is always a window in which a
binding's offsets disagree with the shipped library — which is the exact
failure bindings exist to prevent. One commit changes the ABI and every binding
together, or the change does not land.

The counter-argument was CI: Swift needs macOS runners, KMP needs Gradle. That
costs runner minutes, not correctness, and a `runs-on` matrix handles it.
Release cadence is likewise easier here — one tag, N publish jobs, all gated on
the same green.

Tiers, so this does not become an open-ended obligation:

- **Own it**: the C header and the layout manifest. This is the ABI's single
  source of truth and must have exactly one author.
- **Own it, thin**: Swift and KMP. Accessors generated from the manifest; only
  the scoped-lifetime wrapper is written by hand. These are the platforms the
  products actually target.
- **Do not**: Python, Go, C#. The C header is public.

WASM is different in kind: for a web SBS editor the wasm module *is* the
product, not a binding.

**Constraint to plan for:** `Package.swift` must sit at the repository root —
SwiftPM cannot consume a subdirectory — with its targets pointing into
`packages/`. The root will end up holding `Cargo.toml`, `Package.swift`, and
`settings.gradle.kts`. That is the monorepo tax, and it is accepted.

No directory is created before the package inside it exists.

### A formula is two frontends, not a delegation

A LaTeX or Typst span inside Markdown is an **unresolved cross-unit reference**
(`frontend-contract.md` §2.4), exactly like a named embed. Markdown classifies
it, carries its bytes, and stops. The math is its own unit, in its own domain,
built by its own frontend. The consumer holds both and composes.

The wrong answer, which was proposed and rejected: the host frontend delegating
the span to a math engine and memoizing the result. It is worse on three
counts.

- It makes `markdown-core` depend on `tex-core`. Two independent frontends mean
  a new math engine is a new frontend and the host does not change.
- The incrementality it was meant to buy is **free** with separate units.
  Editing prose does not reach the formula's unit at all, so there is no cached
  layout to invalidate and no memoization to get wrong.
- It takes a decision away from the consumer that belongs to the consumer:
  render the formula, show its source, or export plain text are all valid
  readings of one snapshot.

The mistake underneath the wrong answer was assuming *one source buffer implies
one build unit*. A unit boundary is **the smallest thing that rebuilds
independently**, not a file. A formula's only input is its own bytes.

### Where the line between us and a consumer falls

**We own everything with one correct answer. The consumer owns everything with
a platform answer.** Not for DRY — duplicated effort is fine. Because if two
consumers write the same thing differently, they disagree about what the IR
*means*, and that disagreement is invisible until it is a bug.

| | ours |
|---|---|
| diff → dirty set, with `Descendant` bubbling | yes — the delta *is* the dirty set: `diffs_between` bubbles `Descendant` in, and `Delta::as_slice` hands over the raw slice for a consumer fusing it into its own diffing |
| point → address hit test | yes — **does not exist yet** |
| visible-set culling | yes — **does not exist yet**; `Placement` resolves one address at a time and has no viewport query |
| caret offset → address | the frontend's, and the contract should require it — **does not exist yet** |
| UTF-8 ↔ UTF-16 offset profiles | the frontend's — **does not exist yet** |
| math layout | `tex-core`'s |
| SwiftUI / Compose / DOM emission | the consumer's |
| setting an editor selection | the consumer's |

### Placement is a query

A move publishes nothing; the new position comes from asking. This holds across
the C boundary too. It is what keeps an edit's delta proportional to content
rather than to everything that shifted below it.

### A layout manifest describes one ABI and says which

Offsets are not portable. Where `u64` is 4-aligned — Android's `x86` ABI is the
one that reaches this project — `Address.id` sits at 4 rather than 8 and a
`Diff` is 24 bytes rather than 32. Each manifest carries the pointer width,
`u64` alignment, and endianness it was generated under; a target whose ABI
differs needs its own committed manifest; and regenerating over another ABI's
manifest is refused rather than allowed.

## Working method

`AGENTS.md` has the engineering standard. Two things belong here because they
were learned rather than assumed.

**Mutate every gate before believing it.** Break the rule the gate claims to
check and watch it go red. This has caught a false PASS every single time it
was skipped:

- A gate asserting a prose edit publishes no diff on a formula record survived
  a mutation making the host re-derive the span on every reparse — the
  membership law absorbs a redundant put, so the delta could not observe it.
  Rewritten to check both directions.
- Three "caught it" results in a mutation run were the script erroring on
  `mapfile`, which macOS bash 3.2 does not have. Nothing had been checked.
- The ABI completeness scan preferred `pub struct` over `pub enum` instead of
  taking whichever came first, so it checked a type that is not in the ABI at
  all while silently not checking the one that is.

**A check that finds no files is a check that passes.** This has now happened
three times — a `crates/*/tests/` glob after a directory move, an `include_str!`
list that did not know a new test file existed, a four-file source inventory
that could not see a new module. Discover, and assert the discovery found
something.

## Next, in order

1. **cbindgen header and the release-artifact pipeline.** `.a`/`.dylib`/`.so`,
   the generated header, and `abi/layout.json` attached to a GitHub release.
   This is what a binding consumes, and no binding can be written before it
   exists.
2. **The queries the SBS workloads need**, which are the four "does not exist
   yet" rows above: `address_at(point)`, a viewport query over `Placement` for
   visible-set culling, the frontend's inverse `offset → address`, and an
   encoding profile so a UTF-16 host is not rescanning the source per caret
   move. The last two are `frontend-contract.md` changes and are cheap now,
   expensive once a frontend ships against it.
3. **The first binding**, Swift or KMP, generated from the manifest.
4. **`tex-core` onto composition-ir**, staged, starting with the downstream
   that was going to be rewritten anyway. The math engine goes last.

## Open, deliberately

- **No transform or clip.** An ancestor transform is folded in by the placement
  query. Without clip a repaint region is *conservative* — larger than
  necessary, costing redraw and not correctness. Adding clip means adding the
  region-intersection rule and its gate in the same change.
- **`Diff` is 32 bytes, 13 of them padding**; `Address` is 24 with 7. Visible
  now rather than inferred. Not worth acting on at keystroke rate, and
  shrinking it means packing `Space` into `Id`'s spare bits — a design change,
  not a reorder.
- **Threads on WASM.** A `Snapshot` is `Send + Sync` and there is a gate for it,
  which is what lets Android and Swift render off the main thread. WASM without
  shared memory cannot share one across workers at all; the IR would run in one
  worker and post delta bytes to the main thread. That copy is
  `O(changed-only)`, so it is acceptable, but it is a real difference.
- The four items in `frontend-contract.md` §6 and the four in `derivation.md`
  §7, which are the contracts' own open lists.

## Operational state

- **`v0.1.0` published `composition-ir`, `composition-derive`, and
  `composition-ir-ffi`.** Both of the latter **still need yanking** — an owner
  action. Their workspace status differs, and conflating the two would misread
  this record: `composition-derive` is gone entirely, folded into
  `composition-ir` as a default feature, while `composition-ir-ffi` is still a
  workspace member and is simply `publish = false`.
- **Trusted Publishing (OIDC) has never been exercised.** `v0.1.0` released
  before it landed, using the stored token. No pull request can exercise the
  OIDC exchange, so the stored `CARGO_REGISTRY_TOKEN` must stay until the first
  OIDC release verifies, and only then be revoked.
- `main` is protected by rulesets: linear history, PR required, `nouprax-core`
  review, CI green. Admins may bypass, which is intended.
