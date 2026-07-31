//! A retained, declarative target.

use std::collections::BTreeMap;

use composition_ir::{Address, Delta, Part, Rgba, Snapshot};

/// Both targets must agree on what the paint became, not merely that it changed.
pub fn hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

use crate::Work;

#[derive(Debug, Default)]
pub struct Svg {
    /// element per address, so a later delta can patch in place
    pub elements: BTreeMap<String, String>,
}

fn element(snapshot: &Snapshot, address: Address) -> String {
    let n = snapshot.get(address).expect("live");
    let r = snapshot.placement().rect(address).unwrap_or_default();
    format!(
        "<g id=\"{address:?}\" transform=\"translate({},{})\"><text fill=\"{}\">{}</text></g>",
        r.x,
        r.y,
        hex(n.paint),
        n.text
    )
}

impl Svg {
    /// Full render from a snapshot alone.
    pub fn render(snapshot: &Snapshot) -> (Self, Work) {
        let mut out = Self::default();
        let mut work = Work::default();
        for a in snapshot.addresses() {
            out.elements.insert(format!("{a:?}"), element(snapshot, a));
            work.emitted += 1;
            work.relaid_out += 1;
        }
        (out, work)
    }

    /// Patch from a delta. A paint-only entry must not cost a re-layout.
    pub fn patch(&mut self, snapshot: &Snapshot, delta: &Delta) -> Work {
        let mut work = Work::default();
        for d in &delta.diffs {
            let key = format!("{:?}", d.address);
            match snapshot.get(d.address) {
                None => {
                    self.elements.remove(&key);
                }
                Some(n) => {
                    work.emitted += 1;
                    let needs_geometry = d.parts.contains(Part::Structure)
                        || d.parts.contains(Part::IntrinsicLayout)
                        || d.parts.contains(Part::LineLayout)
                        || d.parts.contains(Part::Fragmentation);
                    if needs_geometry {
                        work.relaid_out += 1;
                        self.elements.insert(key, element(snapshot, d.address));
                    } else if let Some(existing) = self.elements.get_mut(&key) {
                        // paint-only: rewrite the fill, keep the transform
                        let head = existing.split("fill=\"").next().unwrap().to_string();
                        let tail = existing.split("\">").nth(1).unwrap_or("").to_string();
                        *existing = format!("{head}fill=\"{}\">{tail}", hex(n.paint));
                    }
                }
            }
        }
        work
    }
}
