//! Three stand-in output targets, chosen because they stress different things:
//!
//! * **svg** is retained and declarative — it needs stable ids to diff against
//!   and absolute geometry to place elements;
//! * **raster** is immediate-mode — it needs a flat draw list and a repaint
//!   region, in the shape a Skia or Direct2D consumer would build; and
//! * **paged** is write-once and paginated — it needs to know which page a
//!   record landed on and to deduplicate shared resources.
//!
//! None of them may depend on a frontend, an adapter, or a session. If a target
//! cannot be driven from a snapshot and a delta alone, the IR is incomplete for
//! backends and the contract must say so.

pub mod paged;
pub mod raster;
pub mod svg;

/// What a backend touched, so a gate can check that incremental output is
/// proportional to the change rather than to the document.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub emitted: usize,
    pub relaid_out: usize,
}
