//! A frontend conformance fixture.
//!
//! `docs/specs/frontend-contract.md` states what Composition IR requires of a
//! frontend. This crate is the executable form of that statement: a stand-in
//! frontend that provides exactly the required surface and nothing more, plus
//! the adapter that projects it. The gates measure the adapter, so a contract
//! that grew would show up as a failing size assertion rather than as prose.
//!
//! A real frontend — markdown-core, tex-core — is expected to be checkable by
//! the same gates with its own fixture swapped in.

pub mod adapter;
pub mod fixture;
