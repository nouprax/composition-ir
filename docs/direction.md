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
| point → address hit test | yes — **does not exist yet**; and it returns every hit, because the PoC found no order in the IR that covers the set it would have to rank |
| visible-set culling | the consumer's, for now — a rectangle scan costs the same written by hand as it would inside the IR, so this is a question about an index rather than a call |
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

### The header is generated, and checked against the library rather than itself

`cbindgen`, from the Rust source, committed. Committed for the reason
`abi/layout.json` is: a change to the ABI should arrive as a diff someone
reviews, not as behaviour someone finds. Unlike the manifest it is not
per-target — a C compiler computes offsets for whatever it compiles for, so one
header covers every ABI.

The generator being a dev-dependency rather than an installed CLI is the whole
reason the check runs: `cargo test` regenerates and compares with nothing
installed, and a check that needs a tool nobody has is a check that gets
skipped.

What was learned writing it: **a generated artifact must be checked against the
thing it describes, not against its own generator.** Comparing the committed
header to a freshly generated one proves only that the generator is
deterministic. It cannot see the case that actually happened here — `cbindgen`
does not expand `macro_rules!`, so five accessors written as one macro, and the
`CirBytes` they return, were absent from the header while every text comparison
agreed. Nothing failed to compile and nothing failed to link; a C consumer
simply had no way to read any text. Completeness is now checked against the
built library's symbol table, which no spelling in the source can hide from, and
the macro is written out longhand.

### What a release ships, and what it may not ship

Per target: both libraries, the header, and `abi/layout.json`, as one archive.

**Both libraries, not one.** A Swift or Kotlin consumer links the archive into
its own binary; a JVM host or a plugin loads the shared library at run time.
Shipping one kind picks a consumer's linkage model for it.

**The publish waits for the artifacts.** A `cargo publish` cannot be undone,
only yanked, so nothing irreversible happens until every target has built. The
GitHub release is created last, so a release never advertises a version that
failed to publish.

**The dry run builds the whole matrix, not a sample.** A target that fails to
build is otherwise found at the tag, which is the one moment nothing can be done
about it. This costs macOS runner minutes; that trade was already made above,
for this same class of thing, when the bindings were put in this repository.

The set is every target that shares one ABI — 64-bit, little endian, `u64`
8-aligned — which is not a coincidence and is not maintained by eye. `layout.rs`
compares the target it is compiling for against the ABI the manifest describes
and fails the build on a mismatch, so a target needing its own manifest cannot
join the list quietly. `i686-linux-android` is refused today, which is the
first time that rule has been anything but prose.

Deferred with a reason rather than forgotten: **Android**, because its four
ABIs need the NDK and its `x86` one forces a second committed manifest and the
rule for choosing between manifests, which is a design change rather than a
matrix entry; and **`aarch64-unknown-linux-gnu`**, which needs a cross linker
and has no consumer asking for it yet.

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
- The bundle script collected the libraries it found in the target directory
  rather than the ones the build reported producing. Dropping a `--crate-type`
  still shipped a bundle, because the previous run's file was still sitting
  there — and CI restores that directory from a cache.
- The ABI completeness scan preferred `pub struct` over `pub enum` instead of
  taking whichever came first, so it checked a type that is not in the ABI at
  all while silently not checking the one that is.

**A check that finds no files is a check that passes.** This has now happened
three times — a `crates/*/tests/` glob after a directory move, an `include_str!`
list that did not know a new test file existed, a four-file source inventory
that could not see a new module. Discover, and assert the discovery found
something.

**macOS bash is 3.2, and its failures do not look like failures.** `mapfile` is
absent, and a heredoc inside a process substitution is a *parse* error that
leaves the script exiting 0 having done nothing. Anything a release runs on a
macOS runner is written for 3.2 and run under `/bin/bash` before it is believed.

## What the SBS query PoC found

`conformance/workloads/` runs a frontend, the IR, and all three targets over one
document, with each proposed query written the way a consumer must write it
today. Seven findings, and they change the proposal rather than confirm it.

**`address_at(point)` cannot return one address.** Two records overlap and
something must rank them. The IR does have an order — `children` is ordered, and
a painter's algorithm over a tree walk is the usual reading — but §9 says roots
are entry points and not the census, and the live set is deliberately larger
than a `children` walk reaches. In the PoC document one record of six is
reachable from the roots, and the two that need ranking are not among them.
Returning one address would be inventing a rule. It returns every hit, and the
consumer with a rule applies it: `svg` takes the DOM's answer, `raster` has no
scene graph and must be told.

**A point does not name a page.** `Placement::rect` gives one box in one
coordinate space, and the record straddling the break is on two pages with one
box. A hit test on page 0 and the same hit test on page 1 are indistinguishable.
Whatever the query becomes, a paginated consumer needs the fragment in the
question or in the answer.

**Fragment membership is not derivable from geometry, and this was the finding
that contradicted the proposal.** A rectangle over page 1 was expected to differ
from the set of records on page 1 and did not — in a flowing document the two
are the same set. They separate on a running header: on every page, placed once,
so it is on page 1 while its box is not. "What is on page N" is therefore
already answered by `fragments` and needs no new API, and a viewport query
answers only "what is inside this rectangle". Two questions, not one.

**The viewport query is the right shape and the wrong cost.** Both `svg` and
`raster` consume the set directly. Computing it scans every record and resolves
every rect, because nothing is indexed by geometry — so a consumer writing it
by hand pays exactly what the IR would. **Putting it in the IR is only worth
anything if it is indexed**, which is a data-structure change and not a call.

**Culling must not replace the delta.** An edit to an off-screen record still
publishes a diff, which is correct; a consumer that filtered its input by the
visible set would hold a stale record the moment it scrolled. Whatever ships has
to say so.

**The frontend inverse works and nothing requires it.** `source_link` round
trips today. The caret-to-record direction is answerable only because the
fixture happens to keep a span per node, which `frontend-contract.md` does not
ask for.

**UTF-8 and UTF-16 offsets agree until the first non-ASCII character** and
diverge after it, so the difference is not a constant and cannot be cached as
one. Converting walks the source from the start, which an editor repeats per
caret move.

## Next, in order

1. **The queries the SBS workloads need**, redesigned from the findings above:
   a hit test returning every hit rather than one, fragment-qualified; the
   frontend's inverse `offset → address`; and an encoding profile. Visible-set
   culling is *not* in this list as a call — the PoC showed it buys a consumer
   nothing without an index, so it is a data-structure question and is deferred
   until one is worth building. The last two are `frontend-contract.md` changes
   and are cheap now, expensive once a frontend ships against it.
2. **The first binding**, Swift or KMP, generated from the manifest.
3. **`tex-core` onto composition-ir**, staged, starting with the downstream
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
