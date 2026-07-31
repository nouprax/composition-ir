//! Absolute placement is a query, not a delivery.
//!
//! Positions are resolved on demand, so a move that touched no record produces
//! no diff entry and costs a querying consumer nothing. The IR does not run
//! layout; it publishes the result, because a backend cannot draw without it.

use crate::address::Address;

/// A resolved absolute box, in the units the publisher used.
///
/// Two dimensions, because a rasterizer needs a bound to invalidate and a
/// retained target needs one to place an element. Transform and clip are not
/// here: an ancestor transform is folded in by the query, and without clip a
/// repaint region is conservative rather than wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

impl Rect {
    pub fn union(self, other: Rect) -> Rect {
        if self == Rect::default() {
            return other;
        }
        if other == Rect::default() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Placement {
    boxes: Vec<(Address, Rect)>,
    translations: Vec<(Vec<Address>, i64, i64)>,
}

impl Placement {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn push(&mut self, address: Address, rect: Rect) {
        self.boxes.push((address, rect));
    }
    pub fn translate(&mut self, covers: Vec<Address>, dx: i64, dy: i64) {
        self.translations.push((covers, dx, dy));
    }
    pub fn len(&self) -> usize {
        self.boxes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The absolute box of one address, resolved on demand with every
    /// translation that covers it folded in.
    pub fn rect(&self, address: Address) -> Option<Rect> {
        let mut r = self
            .boxes
            .iter()
            .find(|(a, _)| *a == address)
            .map(|(_, r)| *r)?;
        for (covers, dx, dy) in &self.translations {
            if covers.contains(&address) {
                r.x += dx;
                r.y += dy;
            }
        }
        Some(r)
    }
}
