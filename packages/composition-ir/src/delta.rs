//! The delta: one answer to "what differs", defined rather than enumerated.

use crate::address::{Address, SnapshotVersion};
use crate::node::{OWN_PARTS, Part, Parts};
use crate::snapshot::Snapshot;

/// One address whose projection differs, and which parts differ.
///
/// `parts` is empty exactly for an address this commit retired: a retired
/// address has no parts in `after`. That is a consequence of the membership
/// law, not an encoding, and it is why no lifecycle tag exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Diff {
    pub address: Address,
    pub parts: Parts,
}

/// The exact-base difference between two published snapshots.
///
/// It replaces every per-component delta, transition, rebuild plan, and damage
/// record: each of those was one address space and one part of this list.
/// It carries no consumer edit program, because Composition IR does not model,
/// version, or validate consumer state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    pub before: SnapshotVersion,
    pub after: SnapshotVersion,
    pub diffs: Vec<Diff>,
}

impl Delta {
    /// The entries as one contiguous slice.
    ///
    /// `Diff` is `#[repr(C)]`, so this is what a C consumer reads directly:
    /// crossing the boundary is a pointer and a length, never a per-entry
    /// conversion or a second allocation.
    pub fn as_slice(&self) -> &[Diff] {
        &self.diffs
    }

    /// A consumer's whole precondition check. A stale, skipped, wrong-domain,
    /// or replayed delta fails this one comparison, and the answer is always
    /// the same: rebuild from the self-contained snapshot.
    pub fn applies_to(&self, base: SnapshotVersion) -> bool {
        self.before == base
    }
    pub fn is_empty(&self) -> bool {
        self.diffs.is_empty()
    }
}

/// The membership law, implemented literally:
///
/// ```text
/// address ∈ diffs  ⟺  proj_before(address) ≠ proj_after(address)
/// parts(address)   =  { p : proj_before(address).p ≠ proj_after(address).p }
///                     restricted to the parts the address has in `after`
/// ```
pub fn diffs_between(before: &Snapshot, after: &Snapshot) -> Vec<Diff> {
    let mut addresses: Vec<Address> = before
        .addresses()
        .chain(after.addresses())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    addresses.sort();

    let mut out = Vec::new();
    for address in addresses {
        if subtree_eq(before, after, address) {
            continue;
        }
        let mut parts = Parts::EMPTY;
        if let Some(a) = after.get(address) {
            match before.get(address) {
                None => {
                    // Created: differs from absence in every part it has.
                    for p in OWN_PARTS {
                        parts.insert(p);
                    }
                }
                Some(b) => {
                    for p in OWN_PARTS {
                        if !b.part_eq(a, p) {
                            parts.insert(p);
                        }
                    }
                }
            }
            if descendant_differs(before, after, address) {
                parts.insert(Part::Descendant);
            }
        }
        out.push(Diff { address, parts });
    }
    out
}

fn children(s: &Snapshot, address: Address) -> &[Address] {
    s.get(address).map(|n| n.children.as_slice()).unwrap_or(&[])
}

fn descendant_differs(before: &Snapshot, after: &Snapshot, address: Address) -> bool {
    let mut seen: std::collections::BTreeSet<Address> =
        children(before, address).iter().copied().collect();
    seen.extend(children(after, address).iter().copied());
    seen.into_iter().any(|c| !subtree_eq(before, after, c))
}

/// Subtree projection equality.
///
/// Pointer equality of the record is **not** a subtree short-circuit. Children
/// are referenced by address, so an untouched parent record says nothing about
/// what its children now resolve to; it only lets the own-part comparison be
/// skipped. Answering subtree equality in `O(1)` requires a stored subtree
/// stamp, and a production emitter does not ask this question at all — it
/// propagates upward from the changed frontier, where a retired child seeds the
/// walk exactly as a changed one does. This recursion is the law's reference
/// form, used to check that emitter.
fn subtree_eq(before: &Snapshot, after: &Snapshot, address: Address) -> bool {
    match (before.get(address), after.get(address)) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(b), Some(a)) => {
            let own_equal =
                std::sync::Arc::ptr_eq(b, a) || OWN_PARTS.iter().all(|p| b.part_eq(a, *p));
            own_equal && b.children.iter().all(|c| subtree_eq(before, after, *c))
        }
    }
}
