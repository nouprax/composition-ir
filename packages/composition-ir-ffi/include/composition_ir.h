/* Composition IR — the C ABI.
 *
 * Generated from the Rust source; do not edit. The generator is
 * `the_committed_header_matches_what_this_crate_exports` in
 * packages/composition-ir-ffi/tests/header_gates.rs, and an edit made here is
 * reverted by the next run of it.
 *
 * Field offsets are deliberately absent. A C compiler computes them for the
 * target it is compiling for, which is the right answer and one no generator
 * beats. A binding that instead reads fields by offset — Swift, Kotlin, a
 * JavaScript DataView over wasm memory — reads abi/layout.json, which states
 * which ABI it describes.
 *
 * Two validity rules a C declaration cannot express. A CirDomain, a
 * CirRevision, and a CirId's ordinal are never zero; writing a zero produces a
 * value the Rust side considers impossible. And every pointer returned here is
 * borrowed from a retained handle: it is valid until that handle is released,
 * and nothing returned here is ever freed by the caller.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#ifndef COMPOSITION_IR_H
#define COMPOSITION_IR_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// A record space. Each superseded component delta (semantic, frame, source
// link, interaction, resource) is one space here rather than its own record.
//
// The set is **closed**. An open set would make `Address` non-exhaustive for
// every consumer, cost a fixed encoding at the C boundary, and buy nothing:
// every space proposed so far is a `Node` with a different part populated. A
// frontend that genuinely needs a disjoint identity space uses a separate
// `Domain`, which already gives it non-collision without widening this enum.
enum CirSpace
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
    CirSpace_Node,
    CirSpace_Frame,
    CirSpace_Source,
    CirSpace_Origin,
    CirSpace_Destination,
    CirSpace_Resource,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum CirSpace CirSpace;
#else
typedef uint8_t CirSpace;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

// What a referenced resource is *for*. The role decides which projection the
// reference participates in, so swapping a font advances shaping rather than
// structure, and swapping an image advances paint.
enum CirResourceRole
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
    // feeds `Shaping`
    CirResourceRole_Font,
    // feeds `Paint`
    CirResourceRole_Image,
    // feeds `Paint`
    CirResourceRole_Gradient,
    // feeds `Paint`
    CirResourceRole_ColorProfile,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum CirResourceRole CirResourceRole;
#else
typedef uint8_t CirResourceRole;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

// One projection of a record. A part exists if and only if a consumer that
// ignored it would either be wrong, or would pay more than `O(1)`.
//
// This is the vocabulary the dependency layer already needs for pruning
// recomputation; the delta reuses it rather than inventing a second one.
enum CirPart
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
    // Ordered child identity sequence. `O(width)` to reproject.
    CirPart_Structure,
    // Canonical text bytes. `O(length)`.
    CirPart_Text,
    // Raw-source-to-canonical-text mapping. `O(length)`.
    CirPart_TextMap,
    // Font/script/feature resolution feeding measurement. `O(length)`.
    CirPart_Shaping,
    // Intrinsic measurement independent of available space.
    CirPart_IntrinsicLayout,
    // Line breaking within available space.
    CirPart_LineLayout,
    // Distribution across regions.
    CirPart_Fragmentation,
    // Everything paint-only: colour, decoration, opacity. `O(1)`.
    CirPart_Paint,
    // Interaction targets and destinations. `O(1)`.
    CirPart_Interaction,
    // Origin/provenance joins. `O(1)`.
    CirPart_SourceLink,
    // Validation outcome. `O(1)`.
    CirPart_Validation,
    // Nothing of this record's own projection differs; something reachable
    // below it does. Ignoring this leaves a parent-linked consumer holding a
    // stale child, which is wrongness rather than cost.
    CirPart_Descendant,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum CirPart CirPart;
#else
typedef uint8_t CirPart;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

// An opaque retained delta.
typedef struct CirDelta CirDelta;

// An opaque retained snapshot. Retaining one is what keeps its records alive;
// releasing it is the only thing a caller must remember to do.
typedef struct CirSnapshot CirSnapshot;

// One published record. Immutable; snapshots share these by pointer.
typedef struct CirNode CirNode;

// The scope that identities and revisions live in. Opaque: compared for
// equality, never read. Changing anything that can affect IR truth — schema,
// frontend profile, parse options — starts a fresh domain rather than an
// ordinary revision.
typedef uint64_t CirDomain;

// The only revision scalar. Positive, strictly monotonic within one domain.
// Every stamp in this contract is a value drawn from it.
typedef uint64_t CirRevision;

// Which published snapshot. The pair is load-bearing: a bare counter cannot
// reject a delta from a different lineage that happens to carry the same
// number.
typedef struct CirSnapshotVersion {
    CirDomain domain;
    CirRevision revision;
} CirSnapshotVersion;

// Stable identity of one record within one domain. Never reused after
// retirement; delete and reinsert allocates a fresh ordinal.
typedef struct CirId {
    CirDomain domain;
    uint64_t ordinal;
} CirId;

// The one public address union. `Delta` names these and nothing else.
typedef struct CirAddress {
    CirSpace space;
    struct CirId id;
} CirAddress;

// A set of parts. One machine word; a `Diff` is an address plus this.
typedef uint16_t CirParts;
#define CirParts_EMPTY 0

// One address whose projection differs, and which parts differ.
//
// `parts` is empty exactly for an address this commit retired: a retired
// address has no parts in `after`. That is a consequence of the membership
// law, not an encoding, and it is why no lifecycle tag exists.
typedef struct CirDiff {
    struct CirAddress address;
    CirParts parts;
} CirDiff;

// Borrowed addresses. `len` counts addresses, not bytes.
typedef struct CirAddresses {
    const struct CirAddress *ptr;
    size_t len;
} CirAddresses;

// A resolved absolute box, in the units the publisher used.
//
// Two dimensions, because a rasterizer needs a bound to invalidate and a
// retained target needs one to place an element. Transform and clip are not
// here: an ancestor transform is folded in by the query, and without clip a
// repaint region is conservative rather than wrong.
typedef struct CirRect {
    int64_t x;
    int64_t y;
    int64_t width;
    int64_t height;
} CirRect;

// UTF-8 text, borrowed. Not NUL-terminated: the IR's text may contain NUL, and
// a length is what the IR already has.
typedef struct CirBytes {
    const uint8_t *ptr;
    size_t len;
} CirBytes;

// One resource reference: which resource, and what it is used for.
//
// A named `#[repr(C)]` pair rather than a tuple, because a Rust tuple has no
// guaranteed layout and so cannot be handed to a C consumer as a slice. The
// alternative would be converting the list per record on every read, which is
// the per-entry copy the whole boundary is shaped to avoid.
typedef struct CirResourceRef {
    CirResourceRole role;
    struct CirAddress address;
} CirResourceRef;

// Borrowed resource references.
typedef struct CirResources {
    const struct CirResourceRef *ptr;
    size_t len;
} CirResources;

// Borrowed fragment indices, in order.
typedef struct CirFragments {
    const uint32_t *ptr;
    size_t len;
} CirFragments;

// A colour, in the one form every target can draw and compare.
//
// Rich paint -- gradients, patterns, images -- is a `Resource` named by the
// record, so this stays small rather than growing a paint model the IR would
// then own.
typedef struct CirRgba {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} CirRgba;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// # Safety
// `snapshot` must be a pointer previously returned by this library and not yet
// released.
struct CirSnapshotVersion cir_snapshot_version(const struct CirSnapshot *snapshot);

// # Safety
// As `cir_snapshot_version`.
struct CirSnapshot *cir_snapshot_retain(const struct CirSnapshot *snapshot);

// # Safety
// `snapshot` must not be used again after this call.
void cir_snapshot_release(struct CirSnapshot *snapshot);

// Whether an address is live, without materializing anything.
//
// # Safety
// As `cir_snapshot_version`.
bool cir_snapshot_contains(const struct CirSnapshot *snapshot,
                           struct CirAddress address);

// # Safety
// `delta` must be a pointer previously returned by this library.
size_t cir_delta_len(const struct CirDelta *delta);

// A borrowed pointer to the entries, valid while `delta` is retained.
//
// This is the whole reason the value types are `#[repr(C)]`: the caller reads
// the IR's own allocation. Returning a converted copy here would make every
// commit pay for a consumer that might not read it.
//
// # Safety
// As `cir_delta_len`. The returned pointer is invalidated by
// `cir_delta_release`.
const struct CirDiff *cir_delta_entries(const struct CirDelta *delta);

// # Safety
// As `cir_delta_len`.
bool cir_delta_applies_to(const struct CirDelta *delta,
                          struct CirSnapshotVersion base);

// # Safety
// `delta` must not be used again after this call.
void cir_delta_release(struct CirDelta *delta);

// The record at `address`, or null when nothing is live there.
//
// Absence is answered once, here, rather than by every field accessor
// inventing a sentinel that a caller would have to tell apart from an empty
// string or a zero.
//
// # Safety
// `snapshot` must be a pointer previously returned by this library and not yet
// released. The result is borrowed from it and is invalidated by
// `cir_snapshot_release`.
const struct CirNode *cir_snapshot_node(const struct CirSnapshot *snapshot,
                                        struct CirAddress address);

// Where a document's tree starts, in order. A consumer renders a tree by
// walking children from here.
//
// Roots are declared entry points, not the census, and the two are not the
// same set. A builder decides what to call a root, nothing requires a live
// record to hang beneath one, and a `children` walk cannot reach the
// non-`Node` spaces even in principle -- a `Resource` is named by role, a
// `Frame` by fragment index, an `Origin` or `Destination` by an interaction
// string. `cir_snapshot_addresses` is the complete set, and it is what the
// reference renderers consume.
//
// # Safety
// As `cir_snapshot_node`.
struct CirAddresses cir_snapshot_roots(const struct CirSnapshot *snapshot);

// How many records are live: exactly the buffer `cir_snapshot_addresses`
// needs.
//
// # Safety
// As `cir_snapshot_node`.
size_t cir_snapshot_len(const struct CirSnapshot *snapshot);

// Every live address, written into a caller-owned buffer. Returns the total
// live count and writes at most `cap`, so a call with `cap` of 0 sizes the
// buffer without writing.
//
// This is the one call on this boundary that copies, and the reason is
// structural rather than an oversight worth fixing. Records live in a
// persistent hash trie because §4 requires unchanged records to be shared *by
// instance* across live snapshots -- the same requirement that rules out a
// flat `Vec` -- so the live addresses are not contiguous anywhere and there is
// nothing to borrow. Keeping a materialized `Vec<Address>` on the snapshot to
// borrow from would charge every commit for a consumer that may never
// enumerate, and would add a member the state can already answer. So the copy
// is `O(live)`, once per snapshot, on the initial-render path, and only the
// caller that asks for it pays.
//
// The order is **unspecified**. It is the trie's, and a consumer must not
// depend on it; the reference renderers sort by paint position, which is what
// a consumer actually needs and is not this call's business to guess.
//
// # Safety
// As `cir_snapshot_node`. `out` must be writable for `cap` addresses, and may
// be null when `cap` is 0.
size_t cir_snapshot_addresses(const struct CirSnapshot *snapshot,
                              struct CirAddress *out,
                              size_t cap);

// Absolute placement, with every translation covering the address folded in.
// Returns false when the address has no placement, leaving `out` untouched.
//
// Placement is a query rather than a published value, so a move costs a call
// here and nothing in the delta.
//
// # Safety
// As `cir_snapshot_node`. `out` must be writable.
bool cir_snapshot_rect(const struct CirSnapshot *snapshot,
                       struct CirAddress address,
                       struct CirRect *out);

// Canonical text bytes.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirBytes cir_node_text(const struct CirNode *node);

// The raw-source-to-canonical-text mapping.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirBytes cir_node_text_map(const struct CirNode *node);

// Resolved font, feeding shaping.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirBytes cir_node_font(const struct CirNode *node);

// Interaction target or destination.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirBytes cir_node_interaction(const struct CirNode *node);

// The join key a consumer asks the frontend about. Never a coordinate.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirBytes cir_node_source_link(const struct CirNode *node);

// Ordered child identities.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirAddresses cir_node_children(const struct CirNode *node);

// Resources this record draws with, by role. Not children: routing a font
// through `children` would make a font swap a structural change.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirResources cir_node_resources(const struct CirNode *node);

// The fragments -- pages, columns, regions -- this record occupies, in order.
// A record spanning a page break appears on each, which is why it is a list.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirFragments cir_node_fragments(const struct CirNode *node);

// Intrinsic measurement, independent of available space.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
int64_t cir_node_intrinsic(const struct CirNode *node);

// Line count after breaking within available space.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
int64_t cir_node_lines(const struct CirNode *node);

// Colour. Rich paint is a resource this record names.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
struct CirRgba cir_node_paint(const struct CirNode *node);

// Validation outcome.
//
// # Safety
// `node` must come from `cir_snapshot_node` on a retained snapshot.
bool cir_node_valid(const struct CirNode *node);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* COMPOSITION_IR_H */
