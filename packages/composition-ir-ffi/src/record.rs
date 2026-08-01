//! Reading records across the boundary.
//!
//! A `Node` cannot cross by value: it holds `String` and `Vec`, whose layouts
//! Rust does not guarantee. Copying one out per record would be the per-entry
//! conversion this boundary exists to avoid, and it would be paid on the
//! initial render of every document.
//!
//! So a record crosses as a **borrowed pointer**, and its fields as borrowed
//! views into the IR's own allocations. Nothing is copied, nothing is owned by
//! the caller, and nothing has to be freed. Every view stays valid for exactly
//! as long as the snapshot handle it came from is retained.
//!
//! Looking the record up once and reading fields from that pointer is not only
//! tidier than an accessor per field that each takes an address -- it is the
//! difference between one lookup per record and one per field.

use composition_ir::{Address, Node, Rect, ResourceRef, Rgba};

use crate::CirSnapshot;

/// A borrowed record. Valid while the snapshot it came from is retained.
pub type CirNode = Node;

/// UTF-8 text, borrowed. Not NUL-terminated: the IR's text may contain NUL, and
/// a length is what the IR already has.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CirBytes {
    pub ptr: *const u8,
    pub len: usize,
}

/// Borrowed addresses. `len` counts addresses, not bytes.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CirAddresses {
    pub ptr: *const Address,
    pub len: usize,
}

/// Borrowed resource references.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CirResources {
    pub ptr: *const ResourceRef,
    pub len: usize,
}

/// Borrowed fragment indices, in order.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CirFragments {
    pub ptr: *const u32,
    pub len: usize,
}

impl CirBytes {
    fn of(s: &str) -> Self {
        CirBytes {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

/// The record at `address`, or null when nothing is live there.
///
/// Absence is answered once, here, rather than by every field accessor
/// inventing a sentinel that a caller would have to tell apart from an empty
/// string or a zero.
///
/// # Safety
/// `snapshot` must be a pointer previously returned by this library and not yet
/// released. The result is borrowed from it and is invalidated by
/// `cir_snapshot_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_node(
    snapshot: *const CirSnapshot,
    address: Address,
) -> *const CirNode {
    let s = unsafe { &*snapshot };
    s.inner
        .get(address)
        .map_or(std::ptr::null(), std::sync::Arc::as_ptr)
}

/// Where a document's tree starts, in order. A consumer renders a tree by
/// walking children from here.
///
/// Roots are declared entry points, not the census, and the two are not the
/// same set. A builder decides what to call a root, nothing requires a live
/// record to hang beneath one, and a `children` walk cannot reach the
/// non-`Node` spaces even in principle -- a `Resource` is named by role, a
/// `Frame` by fragment index, an `Origin` or `Destination` by an interaction
/// string. `cir_snapshot_addresses` is the complete set, and it is what the
/// reference renderers consume.
///
/// # Safety
/// As `cir_snapshot_node`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_roots(snapshot: *const CirSnapshot) -> CirAddresses {
    let s = unsafe { &*snapshot };
    let roots = s.inner.roots();
    CirAddresses {
        ptr: roots.as_ptr(),
        len: roots.len(),
    }
}

/// How many records are live: exactly the buffer `cir_snapshot_addresses`
/// needs.
///
/// # Safety
/// As `cir_snapshot_node`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_len(snapshot: *const CirSnapshot) -> usize {
    unsafe { &*snapshot }.inner.len()
}

/// Every live address, written into a caller-owned buffer. Returns the total
/// live count and writes at most `cap`, so a call with `cap` of 0 sizes the
/// buffer without writing.
///
/// This is the one call on this boundary that copies, and the reason is
/// structural rather than an oversight worth fixing. Records live in a
/// persistent hash trie because §4 requires unchanged records to be shared *by
/// instance* across live snapshots -- the same requirement that rules out a
/// flat `Vec` -- so the live addresses are not contiguous anywhere and there is
/// nothing to borrow. Keeping a materialized `Vec<Address>` on the snapshot to
/// borrow from would charge every commit for a consumer that may never
/// enumerate, and would add a member the state can already answer. So the copy
/// is `O(live)`, once per snapshot, on the initial-render path, and only the
/// caller that asks for it pays.
///
/// The order is **unspecified**. It is the trie's, and a consumer must not
/// depend on it; the reference renderers sort by paint position, which is what
/// a consumer actually needs and is not this call's business to guess.
///
/// # Safety
/// As `cir_snapshot_node`. `out` must be writable for `cap` addresses, and may
/// be null when `cap` is 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_addresses(
    snapshot: *const CirSnapshot,
    out: *mut Address,
    cap: usize,
) -> usize {
    let s = unsafe { &*snapshot };
    for (i, a) in s.inner.addresses().take(cap).enumerate() {
        unsafe { *out.add(i) = a };
    }
    s.inner.len()
}

/// Absolute placement, with every translation covering the address folded in.
/// Returns false when the address has no placement, leaving `out` untouched.
///
/// Placement is a query rather than a published value, so a move costs a call
/// here and nothing in the delta.
///
/// # Safety
/// As `cir_snapshot_node`. `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_rect(
    snapshot: *const CirSnapshot,
    address: Address,
    out: *mut Rect,
) -> bool {
    let s = unsafe { &*snapshot };
    match s.inner.placement().rect(address) {
        Some(r) => {
            unsafe { *out = r };
            true
        }
        None => false,
    }
}

// The field accessors. Each borrows; none allocates.
//
// # Safety
// In every case `node` must be a non-null pointer from `cir_snapshot_node`
// whose snapshot is still retained.

macro_rules! text_accessor {
    ($name:ident, $field:ident, $doc:literal) => {
        #[doc = $doc]
        ///
        /// # Safety
        /// `node` must come from `cir_snapshot_node` on a retained snapshot.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(node: *const CirNode) -> CirBytes {
            CirBytes::of(&unsafe { &*node }.$field)
        }
    };
}

text_accessor!(cir_node_text, text, "Canonical text bytes.");
text_accessor!(
    cir_node_text_map,
    text_map,
    "The raw-source-to-canonical-text mapping."
);
text_accessor!(cir_node_font, font, "Resolved font, feeding shaping.");
text_accessor!(
    cir_node_interaction,
    interaction,
    "Interaction target or destination."
);
text_accessor!(
    cir_node_source_link,
    source_link,
    "The join key a consumer asks the frontend about. Never a coordinate."
);

/// Ordered child identities.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_children(node: *const CirNode) -> CirAddresses {
    let n = unsafe { &*node };
    CirAddresses {
        ptr: n.children.as_ptr(),
        len: n.children.len(),
    }
}

/// Resources this record draws with, by role. Not children: routing a font
/// through `children` would make a font swap a structural change.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_resources(node: *const CirNode) -> CirResources {
    let n = unsafe { &*node };
    CirResources {
        ptr: n.resources.as_ptr(),
        len: n.resources.len(),
    }
}

/// The fragments -- pages, columns, regions -- this record occupies, in order.
/// A record spanning a page break appears on each, which is why it is a list.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_fragments(node: *const CirNode) -> CirFragments {
    let n = unsafe { &*node };
    CirFragments {
        ptr: n.fragments.as_ptr(),
        len: n.fragments.len(),
    }
}

/// Intrinsic measurement, independent of available space.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_intrinsic(node: *const CirNode) -> i64 {
    unsafe { &*node }.intrinsic
}

/// Line count after breaking within available space.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_lines(node: *const CirNode) -> i64 {
    unsafe { &*node }.lines
}

/// Colour. Rich paint is a resource this record names.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_paint(node: *const CirNode) -> Rgba {
    unsafe { &*node }.paint
}

/// Validation outcome.
///
/// # Safety
/// `node` must come from `cir_snapshot_node` on a retained snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_node_valid(node: *const CirNode) -> bool {
    unsafe { &*node }.valid
}
