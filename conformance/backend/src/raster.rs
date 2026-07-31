//! An immediate-mode target: a flat draw list plus the region to repaint.

use composition_ir::{Address, Delta, Rect, Rgba, Snapshot};

use crate::Work;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawOp {
    pub address: Address,
    pub bounds: Rect,
    pub fill: Rgba,
    pub text: String,
}

/// The region a compositor must invalidate. A raster target cannot repaint
/// without one, which makes it the sharpest question to ask the IR.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub bounds: Rect,
}

impl Region {
    pub fn union(&mut self, r: Rect) {
        self.bounds = self.bounds.union(r);
    }
    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct Raster {
    pub ops: Vec<DrawOp>,
}

fn op(snapshot: &Snapshot, address: Address) -> Option<DrawOp> {
    let n = snapshot.get(address)?;
    Some(DrawOp {
        address,
        bounds: snapshot.placement().rect(address).unwrap_or_default(),
        fill: n.paint,
        text: n.text.clone(),
    })
}

impl Raster {
    pub fn render(snapshot: &Snapshot) -> (Self, Work) {
        let mut ops = Vec::new();
        let mut work = Work::default();
        for a in snapshot.addresses() {
            if let Some(o) = op(snapshot, a) {
                ops.push(o);
                work.emitted += 1;
                work.relaid_out += 1;
            }
        }
        ops.sort_by_key(|o| (o.bounds.y, o.bounds.x, o.address));
        (Self { ops }, work)
    }

    /// Repaint only what the delta named, and report the region a compositor
    /// must invalidate. Without a clip model the region is conservative, which
    /// is a cost rather than an error.
    pub fn repaint(&mut self, snapshot: &Snapshot, delta: &Delta) -> (Work, Region) {
        let mut work = Work::default();
        let mut region = Region::default();
        for d in &delta.diffs {
            let existing = self.ops.iter().position(|o| o.address == d.address);
            if let Some(pos) = existing {
                region.union(self.ops[pos].bounds);
            }
            match (existing, op(snapshot, d.address)) {
                (Some(pos), None) => {
                    self.ops.remove(pos);
                }
                (Some(pos), Some(next)) => {
                    region.union(next.bounds);
                    self.ops[pos] = next;
                    work.emitted += 1;
                }
                (None, Some(next)) => {
                    region.union(next.bounds);
                    self.ops.push(next);
                    work.emitted += 1;
                }
                (None, None) => {}
            }
        }
        self.ops
            .sort_by_key(|o| (o.bounds.y, o.bounds.x, o.address));
        (work, region)
    }
}
