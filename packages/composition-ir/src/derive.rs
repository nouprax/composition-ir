//! Memoized computations that record what they read, invalidated by
//! intersecting those reads with a commit's diff.
//!
//! This is not a consumer registry. Nothing here is published to a consumer and
//! no consumer registers anything: the readers are the engine's own derived
//! cells, and the direction of ownership is what separates the two
//! (`docs/specs/derivation.md` section 1).
//!
//! Behind the default `derive` feature. It lived in its own crate until 0.2.0,
//! which was a packaging accident rather than a decision: it depends on this
//! crate and nothing else, and nobody would reach for it without the IR.

use std::collections::HashMap;

use crate::{Address, Delta, Node, Part, Snapshot};

/// One thing a computation read. Negative and positive reads are both
/// recorded: a computation that concluded "this address is absent" is just as
/// dependent on that fact as one that read a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Obs {
    Read(Address, Part),
    /// A conclusion drawn from whether an address is live, carrying which way
    /// it was observed. Recording only the absent direction is the bug this
    /// shape exists to prevent: a reader that concluded something *because the
    /// address was present* goes equally stale when it is removed, and a
    /// one-directional rule reports that cell as still valid.
    Liveness {
        address: Address,
        was_absent: bool,
    },
}

/// What one execution observed. Never published; it exists so the engine can
/// decide whether that execution is still valid.
#[derive(Debug, Clone, Default)]
pub struct Receipt {
    pub obs: Vec<Obs>,
}

impl Receipt {
    /// Would this execution still produce the same answer after `delta`?
    pub fn survives(&self, delta: &Delta, after: &Snapshot) -> bool {
        !self.obs.iter().any(|o| match *o {
            Obs::Read(a, p) => delta
                .diffs
                .iter()
                .any(|d| d.address == a && d.parts.contains(p)),
            // liveness flipped in either direction
            Obs::Liveness {
                address,
                was_absent,
            } => (!after.contains(address)) != was_absent,
        })
    }
}

/// A reading view over a snapshot that records every access.
#[derive(Debug)]
pub struct Reader<'a> {
    snapshot: &'a Snapshot,
    receipt: Receipt,
}

impl<'a> Reader<'a> {
    pub fn new(snapshot: &'a Snapshot) -> Self {
        Self {
            snapshot,
            receipt: Receipt::default(),
        }
    }
    /// Read one part of one record. Reading through this is the only way a
    /// derived computation may touch the IR.
    pub fn part(&mut self, address: Address, part: Part) -> Option<&Node> {
        self.receipt.obs.push(Obs::Read(address, part));
        self.snapshot.get(address).map(|n| n.as_ref())
    }
    /// Observe whether an address is live. Both answers are recorded, because
    /// both are conclusions that a later commit can falsify.
    pub fn is_absent(&mut self, address: Address) -> bool {
        let absent = !self.snapshot.contains(address);
        self.receipt.obs.push(Obs::Liveness {
            address,
            was_absent: absent,
        });
        absent
    }
    pub fn finish(self) -> Receipt {
        self.receipt
    }
}

type Recipe<V> = Box<dyn Fn(&mut Reader) -> V>;

/// Memoized derived cells over one IR lineage.
pub struct Engine<V> {
    recipes: HashMap<String, Recipe<V>>,
    cells: HashMap<String, (V, Receipt)>,
    pub recomputations: usize,
}

impl<V> std::fmt::Debug for Engine<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("cells", &self.cells.len())
            .field("recomputations", &self.recomputations)
            .finish()
    }
}

impl<V: Clone + PartialEq> Default for Engine<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone + PartialEq> Engine<V> {
    pub fn new() -> Self {
        Self {
            recipes: HashMap::new(),
            cells: HashMap::new(),
            recomputations: 0,
        }
    }
    pub fn define(&mut self, key: &str, recipe: Recipe<V>) {
        self.recipes.insert(key.to_string(), recipe);
    }
    pub fn get(&mut self, key: &str, snapshot: &Snapshot) -> V {
        if let Some((v, _)) = self.cells.get(key) {
            return v.clone();
        }
        self.recompute(key, snapshot)
    }
    fn recompute(&mut self, key: &str, snapshot: &Snapshot) -> V {
        let recipe = self.recipes.get(key).expect("no such recipe");
        let mut reader = Reader::new(snapshot);
        let value = recipe(&mut reader);
        self.recomputations += 1;
        self.cells
            .insert(key.to_string(), (value.clone(), reader.finish()));
        value
    }
    /// Drop exactly the cells whose observations the commit disturbed.
    /// Returns how many were dropped.
    pub fn invalidate(&mut self, delta: &Delta, after: &Snapshot) -> usize {
        let doomed: Vec<String> = self
            .cells
            .iter()
            .filter(|(_, (_, r))| !r.survives(delta, after))
            .map(|(k, _)| k.clone())
            .collect();
        for k in &doomed {
            self.cells.remove(k);
        }
        doomed.len()
    }
    pub fn is_cached(&self, key: &str) -> bool {
        self.cells.contains_key(key)
    }
}
