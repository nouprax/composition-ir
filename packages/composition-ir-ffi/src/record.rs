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

/// Where the document starts. A consumer renders by walking children from here;
/// there is no separate enumeration call because a traversal needs none.
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
