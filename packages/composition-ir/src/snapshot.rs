//! The published state and how the next one is built from it.

use std::sync::Arc;

// the Sync variant: the contract requires published snapshots to be readable
// from any thread, which the Rc-backed default cannot provide
use rpds::HashTrieMapSync;

use crate::address::{Address, Domain, Revision, SnapshotVersion};
use crate::delta::Delta;
use crate::node::Node;
use crate::placement::Placement;

/// One immutable published composition state.
///
/// Self-contained: it answers every query it exposes without consulting a live
/// session, and it stays readable after later commits. Adjacent snapshots share
/// every unchanged record by pointer, so retaining the previous one to diff
/// against costs the changed frontier rather than a second tree.
#[derive(Debug, Clone)]
pub struct Snapshot {
    version: SnapshotVersion,
    records: HashTrieMapSync<Address, Arc<Node>>,
    roots: Vec<Address>,
    placement: Placement,
}

impl Snapshot {
    pub fn version(&self) -> SnapshotVersion {
        self.version
    }
    pub fn get(&self, address: Address) -> Option<&Arc<Node>> {
        assert_eq!(
            address.domain(),
            self.version.domain,
            "address from a foreign domain: this is a programmer error, not a result value"
        );
        self.records.get(&address)
    }
    pub fn contains(&self, address: Address) -> bool {
        self.records.contains_key(&address)
    }
    pub fn roots(&self) -> &[Address] {
        &self.roots
    }
    pub fn placement(&self) -> &Placement {
        &self.placement
    }
    pub fn len(&self) -> usize {
        self.records.size()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn addresses(&self) -> impl Iterator<Item = Address> + '_ {
        self.records.keys().copied()
    }

    pub fn empty(domain: Domain, revision: Revision) -> Self {
        Self {
            version: SnapshotVersion { domain, revision },
            records: HashTrieMapSync::new_sync(),
            roots: Vec::new(),
            placement: Placement::empty(),
        }
    }

    pub fn edit(&self) -> SnapshotBuilder {
        SnapshotBuilder {
            base: self.clone(),
            records: self.records.clone(),
            roots: self.roots.clone(),
            placement: self.placement.clone(),
        }
    }
}

/// What a commit returns: the complete correctness result, and a discardable
/// exact-base description of how it differs from its predecessor.
#[derive(Debug, Clone)]
pub struct Commit {
    pub snapshot: Snapshot,
    pub delta: Delta,
}

/// Accumulates one candidate state. Nothing is published until `commit`.
#[derive(Debug)]
pub struct SnapshotBuilder {
    base: Snapshot,
    records: HashTrieMapSync<Address, Arc<Node>>,
    roots: Vec<Address>,
    placement: Placement,
}

impl SnapshotBuilder {
    pub fn put(&mut self, address: Address, node: Node) -> &mut Self {
        self.records.insert_mut(address, Arc::new(node));
        self
    }
    pub fn update(&mut self, address: Address, f: impl FnOnce(&mut Node)) -> &mut Self {
        let mut node = (**self.records.get(&address).expect("no such address")).clone();
        f(&mut node);
        self.records.insert_mut(address, Arc::new(node));
        self
    }
    /// Update only if the address is live. A builder that silently created a
    /// record on a stale address would let a caller resurrect a retired
    /// identity, which 5.2 forbids.
    pub fn update_if_live(&mut self, address: Address, f: impl FnOnce(&mut Node)) -> &mut Self {
        if self.records.contains_key(&address) {
            self.update(address, f);
        }
        self
    }
    pub fn contains(&self, address: Address) -> bool {
        self.records.contains_key(&address)
    }
    pub fn remove(&mut self, address: Address) -> &mut Self {
        self.records.remove_mut(&address);
        self.roots.retain(|r| *r != address);
        self
    }
    pub fn root(&mut self, address: Address) -> &mut Self {
        self.roots.push(address);
        self
    }
    /// Discard placement so it can be rebuilt from the current order. Placement
    /// is a query, so rebuilding it publishes nothing.
    pub fn reset_placement(&mut self) -> &mut Self {
        self.placement = Placement::empty();
        self
    }
    pub fn place(&mut self, address: Address, rect: crate::placement::Rect) -> &mut Self {
        self.placement.push(address, rect);
        self
    }
    /// A translation is one compact aggregate over moved coverage. It changes
    /// no projection, so it produces no diff entry.
    pub fn translate(&mut self, covers: Vec<Address>, dx: i64, dy: i64) -> &mut Self {
        self.placement.translate(covers, dx, dy);
        self
    }

    /// Publish, or report that nothing was published.
    ///
    /// A candidate whose canonical values equal the base is a no-op: it reuses
    /// the exact current snapshot and advances no revision.
    pub fn commit(self) -> Commit {
        let candidate = Snapshot {
            version: SnapshotVersion {
                domain: self.base.version.domain,
                revision: next_revision(self.base.version.revision),
            },
            records: self.records,
            roots: self.roots,
            placement: self.placement,
        };
        let diffs = crate::delta::diffs_between(&self.base, &candidate);
        if diffs.is_empty() && candidate.placement == self.base.placement {
            let version = self.base.version;
            return Commit {
                snapshot: self.base,
                delta: Delta {
                    before: version,
                    after: version,
                    diffs: Vec::new(),
                },
            };
        }
        Commit {
            delta: Delta {
                before: self.base.version,
                after: candidate.version,
                diffs,
            },
            snapshot: candidate,
        }
    }
}

fn next_revision(r: Revision) -> Revision {
    Revision(
        r.0.checked_add(1)
            .expect("revision exhausted: start a fresh domain"),
    )
}
