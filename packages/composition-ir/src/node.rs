//! The record and its projection parts.

use crate::address::Address;

/// One projection of a record. A part exists if and only if a consumer that
/// ignored it would either be wrong, or would pay more than `O(1)`.
///
/// This is the vocabulary the dependency layer already needs for pruning
/// recomputation; the delta reuses it rather than inventing a second one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Part {
    /// Ordered child identity sequence. `O(width)` to reproject.
    Structure,
    /// Canonical text bytes. `O(length)`.
    Text,
    /// Raw-source-to-canonical-text mapping. `O(length)`.
    TextMap,
    /// Font/script/feature resolution feeding measurement. `O(length)`.
    Shaping,
    /// Intrinsic measurement independent of available space.
    IntrinsicLayout,
    /// Line breaking within available space.
    LineLayout,
    /// Distribution across regions.
    Fragmentation,
    /// Everything paint-only: colour, decoration, opacity. `O(1)`.
    Paint,
    /// Interaction targets and destinations. `O(1)`.
    Interaction,
    /// Origin/provenance joins. `O(1)`.
    SourceLink,
    /// Validation outcome. `O(1)`.
    Validation,
    /// Nothing of this record's own projection differs; something reachable
    /// below it does. Ignoring this leaves a parent-linked consumer holding a
    /// stale child, which is wrongness rather than cost.
    Descendant,
}

pub const ALL_PARTS: [Part; 12] = [
    Part::Structure,
    Part::Text,
    Part::TextMap,
    Part::Shaping,
    Part::IntrinsicLayout,
    Part::LineLayout,
    Part::Fragmentation,
    Part::Paint,
    Part::Interaction,
    Part::SourceLink,
    Part::Validation,
    Part::Descendant,
];
/// Every part except `Descendant`, which is derived rather than stored.
pub const OWN_PARTS: [Part; 11] = [
    Part::Structure,
    Part::Text,
    Part::TextMap,
    Part::Shaping,
    Part::IntrinsicLayout,
    Part::LineLayout,
    Part::Fragmentation,
    Part::Paint,
    Part::Interaction,
    Part::SourceLink,
    Part::Validation,
];

/// A set of parts. One machine word; a `Diff` is an address plus this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct Parts(u16);

impl Parts {
    pub const EMPTY: Parts = Parts(0);

    pub fn of(part: Part) -> Self {
        Parts(1 << (part as u16))
    }
    pub fn contains(self, part: Part) -> bool {
        self.0 & Parts::of(part).0 != 0
    }
    pub fn insert(&mut self, part: Part) {
        self.0 |= Parts::of(part).0;
    }
    /// The raw flag word, as it crosses the C boundary.
    pub fn bits(self) -> u16 {
        self.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// Drop `Descendant`, recovering the record-only reading of "changed".
    pub fn own(self) -> Self {
        Parts(self.0 & !Parts::of(Part::Descendant).0)
    }
    pub fn iter(self) -> impl Iterator<Item = Part> {
        ALL_PARTS.into_iter().filter(move |p| self.contains(*p))
    }
}

/// A colour, in the one form every target can draw and compare.
///
/// Rich paint -- gradients, patterns, images -- is a `Resource` named by the
/// record, so this stays small rather than growing a paint model the IR would
/// then own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(C)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// What a referenced resource is *for*. The role decides which projection the
/// reference participates in, so swapping a font advances shaping rather than
/// structure, and swapping an image advances paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ResourceRole {
    /// feeds `Shaping`
    Font,
    /// feeds `Paint`
    Image,
    /// feeds `Paint`
    Gradient,
    /// feeds `Paint`
    ColorProfile,
}

impl ResourceRole {
    /// The projection this role participates in. A reference is not structure:
    /// routing it through `children` would make a font swap a structural change
    /// and invalidate every ancestor.
    pub fn part(self) -> Part {
        match self {
            ResourceRole::Font => Part::Shaping,
            ResourceRole::Image | ResourceRole::Gradient | ResourceRole::ColorProfile => {
                Part::Paint
            }
        }
    }
}

/// One resource reference: which resource, and what it is used for.
///
/// A named `#[repr(C)]` pair rather than a tuple, because a Rust tuple has no
/// guaranteed layout and so cannot be handed to a C consumer as a slice. The
/// alternative would be converting the list per record on every read, which is
/// the per-entry copy the whole boundary is shaped to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ResourceRef {
    pub role: ResourceRole,
    pub address: Address,
}

/// One published record. Immutable; snapshots share these by pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub children: Vec<Address>,
    pub text: String,
    pub text_map: String,
    pub font: String,
    pub intrinsic: i64,
    pub lines: i64,
    pub paint: Rgba,
    /// resources this record draws with, by role. Not children.
    pub resources: Vec<ResourceRef>,
    /// which fragments -- pages, columns, regions -- this record occupies, in
    /// order. Empty means unplaced or unfragmented.
    pub fragments: Vec<u32>,
    pub interaction: String,
    pub source_link: String,
    pub valid: bool,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            text: String::new(),
            text_map: String::new(),
            font: String::new(),
            intrinsic: 0,
            lines: 0,
            paint: Rgba::default(),
            resources: Vec::new(),
            fragments: Vec::new(),
            interaction: String::new(),
            source_link: String::new(),
            valid: true,
        }
    }
}

impl Node {
    /// The resources feeding one projection.
    pub fn resources_for(&self, part: Part) -> Vec<crate::address::Address> {
        self.resources
            .iter()
            .filter(|r| r.role.part() == part)
            .map(|r| r.address)
            .collect()
    }

    /// Exact per-part equality. No hashing: the membership law is stated over
    /// canonical values, so it is checked over canonical values.
    pub fn part_eq(&self, other: &Node, part: Part) -> bool {
        match part {
            Part::Structure => self.children == other.children,
            Part::Text => self.text == other.text,
            Part::TextMap => self.text_map == other.text_map,
            Part::Shaping => {
                self.font == other.font
                    && self.text == other.text
                    && self.resources_for(Part::Shaping) == other.resources_for(Part::Shaping)
            }
            Part::IntrinsicLayout => self.intrinsic == other.intrinsic,
            Part::LineLayout => self.lines == other.lines,
            Part::Fragmentation => self.fragments == other.fragments,
            Part::Paint => {
                self.paint == other.paint
                    && self.resources_for(Part::Paint) == other.resources_for(Part::Paint)
            }
            Part::Interaction => self.interaction == other.interaction,
            Part::SourceLink => self.source_link == other.source_link,
            Part::Validation => self.valid == other.valid,
            Part::Descendant => true,
        }
    }
}
