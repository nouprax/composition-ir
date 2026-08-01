//! The three workloads, end to end.
//!
//! `docs/direction.md` names three workloads that decide whether a design is
//! right — side-by-side live preview, element select → editor, and native
//! formulas in Markdown — and the frontend and backend contracts each exercise
//! one half of the chain. Neither exercises the whole of it, and a boundary is
//! only as good as the whole chain it sits in.
//!
//! This crate joins them: a frontend AST, adapted into the IR, rendered by all
//! three output targets. It exists so that a proposed change to the C ABI or to
//! a contract boundary can be tried against every consumer *before* it is
//! frozen, rather than discovered by the first consumer to ship against it.
//! That has already happened once here in the other order — the backend
//! contract reopened the IR contract, because building an SVG, a raster, and a
//! paginated target found four things they needed that the IR did not carry.
//!
//! One consumer's opinion is not evidence about a boundary. Three
//! differently-shaped consumers are: `svg` is retained and has its own hit
//! testing, `raster` is immediate-mode and has none, and `paged` has neither a
//! viewport nor a single coordinate space.
//!
//! Nothing here is normative. `candidate` holds *proposed* shapes, written the
//! way a consumer would have to write them today, so what they cost and what
//! they cannot express is measured rather than argued.

pub mod candidate;
pub mod document;
