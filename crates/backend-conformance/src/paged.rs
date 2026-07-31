//! A write-once, paginated target with a deduplicated resource table.

use std::collections::{BTreeMap, BTreeSet};

use composition_ir::{Address, Snapshot, Space};

use crate::Work;

#[derive(Debug, Default)]
pub struct Paged {
    /// page index -> the records that landed on it
    pub pages: BTreeMap<u32, Vec<Address>>,
    /// every resource referenced, once
    pub resources: BTreeSet<Address>,
}

impl Paged {
    pub fn render(snapshot: &Snapshot) -> (Self, Work) {
        let mut out = Self::default();
        let mut work = Work::default();
        for a in snapshot.addresses() {
            let n = snapshot.get(a).unwrap();
            if a.space == Space::Resource {
                out.resources.insert(a);
                continue;
            }
            // a record occupies the fragments it lists, in order; a record
            // spanning a page break appears on each of them
            for f in &n.fragments {
                out.pages.entry(*f).or_default().push(a);
            }
            work.emitted += 1;
            work.relaid_out += 1;
        }
        for v in out.pages.values_mut() {
            v.sort();
        }
        (out, work)
    }

    /// Which fragments a record occupies. A record that spans a break appears
    /// on each of them, which is why this is a list rather than an index.
    pub fn pages_of(&self, address: Address) -> Vec<u32> {
        self.pages
            .iter()
            .filter(|(_, v)| v.contains(&address))
            .map(|(p, _)| *p)
            .collect()
    }
}
