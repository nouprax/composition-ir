# Composition IR

Composition IR is the shared intermediate representation that document
frontends produce and that editors, renderers, and inspectors consume. It owns
one thing: **an immutable published state per commit, and an exact description
of how two published states differ.**

It is optimized for three workloads that a batch-oriented IR handles badly:

- **editing** — a keystroke must cost work proportional to what it changed, not
  to document size;
- **side-by-side** — an editor must be able to say *where* the preview changed
  and map that back to source; and
- **element inspection** — a tool must be able to ask what one element is, what
  it came from, and what depends on it, without materializing everything.

## Status

Four contracts, all frozen against a green suite: the IR itself, the
engine-internal derivation layer, what a frontend must supply, and what an
output target reads. The last one reopened the first — building an SVG, a
raster, and a paginated target found four things they needed that the IR did
not carry — which is the working rule here: **a claim about a consumer that no
consumer has exercised is a hypothesis.** **A rule
that cannot be checked by a gate is a rule that will be violated silently**,
so every rule cites the gate that checks it and a gate asserts those citations
resolve. Extending a contract means adding the rule and its gate in the same
change.

```
cargo test        # the conformance gates
cargo clippy --all-targets
```

## Position

```text
markdown-core ─┐
               ├─→ Composition IR ─→ layout / paint / export / inspection
tex-core     ──┘
```

Frontends own source bytes, syntax, and language semantics. They keep their own
ASTs and their own repositories; Composition IR does not define, contain, or
version a frontend's AST. `tex-core` produces Composition IR directly and
depends on this crate; `markdown-core` is adapted into it.

Nothing here resolves an embed, import, or include. Composing several documents
is the consumer's layer, for reasons stated in the contract: materializing an
imported subtree breaks fresh-build equivalence, single-domain identity, and
coordinate resolution at once.

## Implementation language

Rust, for one decisive reason rather than taste. The load-bearing guarantee is
that *an unchanged subtree is the same instance in the next snapshot, and every
published snapshot is immutable, self-contained, and safe for concurrent
reads*. That is an aliasing and lifetime guarantee across many simultaneously
live revisions. In C it is hand-written reference counting at every path-copy
site, where one missed decrement is a leak that only appears in long editing
sessions — exactly the target workload. Here it is checked at compile time, and
concurrent reads of a published snapshot are free.

Consumers are not required to be Rust. The public boundary is a C ABI, the same
one the existing frontends and their Swift/Kotlin/ECMAScript bindings already
speak.

## Layout

Two trees, because a thing that ships and a thing that proves a contract are
not the same kind of thing and should not be siblings.

```text
packages/                    what ships
  composition-ir/            the IR: snapshots, deltas, placement
    src/address.rs           identity: domain, revision, address spaces
    src/node.rs              records and the closed projection parts
    src/snapshot.rs          published state and the commit builder
    src/delta.rs             the membership law
    src/placement.rs         absolute placement as a query
    src/derive.rs            memoization and invalidation (`derive` feature)
    tests/gates.rs           the contract, executable
    tests/derive_gates.rs    the derivation contract, executable
  composition-ir-ffi/        the C ABI: opaque handles, zero-copy value types
    abi/layout.json          the published field layout; bindings generate from it
    src/layout.rs            generates it from the compiled types
    tests/layout_gates.rs    fails when the ABI and that file disagree

conformance/                 what proves the contracts, and never ships
  frontend/                  the frontend contract, executable
    src/fixture.rs           a stand-in frontend providing the required surface
    src/adapter.rs           the whole seam between a frontend and the IR
    tests/gates.rs           boundary and cost gates a real frontend must pass
  backend/                   three output targets and their gates
    src/{svg,raster,paged}.rs  retained, immediate-mode, and paginated
    tests/gates.rs           what a backend may read, and what it costs

docs/specs/                  the contracts, normative
```

`packages/` is where the language bindings will go — Swift, Kotlin
Multiplatform, and a WASM/TypeScript package, each publishing to its own
ecosystem from this repository rather than from its own. They live here for one
reason: the field layout of the C ABI is generated from the Rust build, and
every binding reads it. Split across repositories there is always a window in
which a binding's offsets disagree with the shipped library, which is the exact
failure the bindings exist to prevent. One commit changes the ABI and every
binding together, or the change does not land.

Directories are not created before the package inside them exists.

`composition-ir` is the only crate published to crates.io. The FFI shim is not:
crates.io distributes source for Rust dependents, and a C consumer links a
built library rather than adding a Cargo dependency.

## License

Apache-2.0.
