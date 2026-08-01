//! The C ABI.
//!
//! The design goal is that crossing costs nothing. Every public value type in
//! `composition-ir` is already `#[repr(C)]`, so a C consumer reads the diff as
//! a pointer and a length into the same allocation the IR built. There is no
//! per-entry conversion, no marshalling buffer, and nothing for the caller to
//! free except the handles it explicitly retained.
//!
//! Naming follows `cir_<subject>_<verb>`, the subject being the type the call
//! operates on or produces.

pub mod layout;

use std::sync::Arc;

use composition_ir::{Address, Delta, Diff, Snapshot, SnapshotVersion};

/// An opaque retained snapshot. Retaining one is what keeps its records alive;
/// releasing it is the only thing a caller must remember to do.
#[derive(Debug)]
pub struct CirSnapshot {
    inner: Arc<Snapshot>,
}

/// An opaque retained delta.
#[derive(Debug)]
pub struct CirDelta {
    inner: Arc<Delta>,
}

/// # Safety
/// `snapshot` must be a pointer previously returned by this library and not yet
/// released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_version(snapshot: *const CirSnapshot) -> SnapshotVersion {
    let s = unsafe { &*snapshot };
    s.inner.version()
}

/// # Safety
/// As `cir_snapshot_version`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_retain(snapshot: *const CirSnapshot) -> *mut CirSnapshot {
    let s = unsafe { &*snapshot };
    Box::into_raw(Box::new(CirSnapshot {
        inner: Arc::clone(&s.inner),
    }))
}

/// # Safety
/// `snapshot` must not be used again after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_release(snapshot: *mut CirSnapshot) {
    if !snapshot.is_null() {
        drop(unsafe { Box::from_raw(snapshot) });
    }
}

/// Whether an address is live, without materializing anything.
///
/// # Safety
/// As `cir_snapshot_version`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_snapshot_contains(
    snapshot: *const CirSnapshot,
    address: Address,
) -> bool {
    let s = unsafe { &*snapshot };
    s.inner.contains(address)
}

/// # Safety
/// `delta` must be a pointer previously returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_delta_len(delta: *const CirDelta) -> usize {
    let d = unsafe { &*delta };
    d.inner.diffs.len()
}

/// A borrowed pointer to the entries, valid while `delta` is retained.
///
/// This is the whole reason the value types are `#[repr(C)]`: the caller reads
/// the IR's own allocation. Returning a converted copy here would make every
/// commit pay for a consumer that might not read it.
///
/// # Safety
/// As `cir_delta_len`. The returned pointer is invalidated by
/// `cir_delta_release`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_delta_entries(delta: *const CirDelta) -> *const Diff {
    let d = unsafe { &*delta };
    d.inner.as_slice().as_ptr()
}

/// # Safety
/// As `cir_delta_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_delta_applies_to(
    delta: *const CirDelta,
    base: SnapshotVersion,
) -> bool {
    let d = unsafe { &*delta };
    d.inner.applies_to(base)
}

/// # Safety
/// `delta` must not be used again after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cir_delta_release(delta: *mut CirDelta) {
    if !delta.is_null() {
        drop(unsafe { Box::from_raw(delta) });
    }
}

// Constructors used by the Rust side to hand ownership across.
impl CirSnapshot {
    pub fn into_raw(snapshot: Snapshot) -> *mut CirSnapshot {
        Box::into_raw(Box::new(CirSnapshot {
            inner: Arc::new(snapshot),
        }))
    }
}
impl CirDelta {
    pub fn into_raw(delta: Delta) -> *mut CirDelta {
        Box::into_raw(Box::new(CirDelta {
            inner: Arc::new(delta),
        }))
    }
}
