//! Identity: the scope that identities and revisions live in, and the one
//! address union over every record space the IR publishes.

use std::num::NonZeroU64;

/// The scope that identities and revisions live in. Opaque: compared for
/// equality, never read. Changing anything that can affect IR truth — schema,
/// frontend profile, parse options — starts a fresh domain rather than an
/// ordinary revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Domain(pub NonZeroU64);

/// The only revision scalar. Positive, strictly monotonic within one domain.
/// Every stamp in this contract is a value drawn from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Revision(pub NonZeroU64);

/// Which published snapshot. The pair is load-bearing: a bare counter cannot
/// reject a delta from a different lineage that happens to carry the same
/// number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct SnapshotVersion {
    pub domain: Domain,
    pub revision: Revision,
}

/// A record space. Each superseded component delta (semantic, frame, source
/// link, interaction, resource) is one space here rather than its own record.
///
/// The set is **closed**. An open set would make `Address` non-exhaustive for
/// every consumer, cost a fixed encoding at the C boundary, and buy nothing:
/// every space proposed so far is a `Node` with a different part populated. A
/// frontend that genuinely needs a disjoint identity space uses a separate
/// `Domain`, which already gives it non-collision without widening this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Space {
    Node,
    Frame,
    Source,
    Origin,
    Destination,
    Resource,
}

/// Stable identity of one record within one domain. Never reused after
/// retirement; delete and reinsert allocates a fresh ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Id {
    pub domain: Domain,
    pub ordinal: NonZeroU64,
}

/// The one public address union. `Delta` names these and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Address {
    pub space: Space,
    pub id: Id,
}

impl Address {
    pub fn new(space: Space, id: Id) -> Self {
        Self { space, id }
    }
    pub fn domain(&self) -> Domain {
        self.id.domain
    }
}
