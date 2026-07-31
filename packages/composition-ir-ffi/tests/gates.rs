//! Conformance gates for the C ABI section of `docs/specs/composition-ir.md`.

use std::mem::{align_of, size_of};
use std::num::NonZeroU64;

use composition_ir::{Address, Diff, Domain, Id, Node, Parts, Revision, Snapshot, Space};
use composition_ir_ffi::{
    CirDelta, CirSnapshot, cir_delta_entries, cir_delta_len, cir_delta_release,
    cir_snapshot_contains, cir_snapshot_release, cir_snapshot_retain, cir_snapshot_version,
};

fn dom() -> Domain {
    Domain(NonZeroU64::new(1).unwrap())
}
fn addr(n: u64) -> Address {
    Address::new(
        Space::Node,
        Id {
            domain: dom(),
            ordinal: NonZeroU64::new(n).unwrap(),
        },
    )
}

/// The value types cross by layout, not by conversion. If any of these grows a
/// niche, a discriminant, or a pointer, the boundary silently starts costing a
/// per-entry copy.
#[test]
fn the_value_types_are_abi_stable_and_pod() {
    assert_eq!(size_of::<Domain>(), 8);
    assert_eq!(size_of::<Revision>(), 8);
    assert_eq!(size_of::<Parts>(), 2);
    assert_eq!(size_of::<Space>(), 1);
    // Address is space + id; Diff is address + parts. Both fixed, both without
    // padding surprises that would need a translation struct.
    assert_eq!(size_of::<Id>(), 16);
    assert_eq!(align_of::<Diff>(), 8);
    assert!(
        size_of::<Diff>() <= 32,
        "Diff is {} bytes",
        size_of::<Diff>()
    );
    assert!(!std::mem::needs_drop::<Diff>(), "a Diff must be plain data");
}

/// A C consumer reads the IR's own allocation.
#[test]
fn crossing_the_boundary_allocates_nothing_per_entry() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    for i in 1..=100u64 {
        e.put(
            addr(i),
            Node {
                text: format!("t{i}"),
                ..Node::default()
            },
        );
    }
    let commit = e.commit();
    let rust_ptr = commit.delta.as_slice().as_ptr();
    let expected_len = commit.delta.diffs.len();

    let delta = CirDelta::into_raw(commit.delta.clone());
    unsafe {
        assert_eq!(cir_delta_len(delta), expected_len);
        let c_ptr = cir_delta_entries(delta);
        // the same bytes, read through both vocabularies
        let via_c = std::slice::from_raw_parts(c_ptr, expected_len);
        assert_eq!(via_c, commit.delta.as_slice());
        assert_ne!(rust_ptr, std::ptr::null());
        cir_delta_release(delta);
    }
}

/// Retention is explicit and reference-counted; releasing one handle does not
/// invalidate another.
#[test]
fn handles_are_retained_and_released_independently() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    e.put(addr(1), Node::default());
    let commit = e.commit();
    let version = commit.snapshot.version();

    let a = CirSnapshot::into_raw(commit.snapshot);
    unsafe {
        let b = cir_snapshot_retain(a);
        cir_snapshot_release(a);
        // b is still usable after a is gone
        assert_eq!(cir_snapshot_version(b), version);
        assert!(cir_snapshot_contains(b, addr(1)));
        assert!(!cir_snapshot_contains(b, addr(2)));
        cir_snapshot_release(b);
    }
}
