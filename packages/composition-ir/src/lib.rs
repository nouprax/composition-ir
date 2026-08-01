//! Composition IR: one immutable published state per commit, and one exact-base
//! `Delta` describing how two published states differ.
//!
//! The contract this implements is `docs/specs/composition-ir.md`. The two rules
//! everything else follows from are:
//!
//! * a `Snapshot` is immutable, self-contained, and shares every unchanged
//!   subtree with its predecessor by pointer; and
//! * an address is in `Delta::diffs` **iff** its projection differs between the
//!   two snapshots, carrying exactly the parts that differ.

mod address;
mod delta;
#[cfg(feature = "derive")]
pub mod derive;
mod node;
mod placement;
mod snapshot;

pub use address::{Address, Domain, Id, Revision, SnapshotVersion, Space};
pub use delta::{Delta, Diff};
pub use node::{Node, Part, Parts, ResourceRef, ResourceRole, Rgba};
pub use placement::{Placement, Rect};
pub use snapshot::{Commit, Snapshot, SnapshotBuilder};
