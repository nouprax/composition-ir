//! The proposed queries, written the way a consumer must write them today.
//!
//! None of this is a proposal for where the code should live. It is here so
//! that what a consumer pays, and what a consumer *cannot express at all*, is
//! measured against three real targets before the boundary is decided. Anything
//! that turns out to be impossible here is a hole in the proposal, not a
//! shortcoming of the PoC.

use composition_ir::{Address, Rect, Snapshot};

/// A point in the document's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

/// What a query cost, in the units a consumer would feel.
///
/// `records_scanned` is the whole point: every query below is `O(live)` because
/// nothing in the IR is indexed by geometry, and at keystroke rate a consumer
/// pays it per frame and per pointer move.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub records_scanned: usize,
    pub rects_resolved: usize,
}

fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.width && p.y >= r.y && p.y < r.y + r.height
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// Every record whose box contains the point.
///
/// Deliberately *every* one rather than a single answer. Which of them a click
/// means is the question this returns unanswered, and the PoC's job is to show
/// that the IR has nothing to answer it with: there is no z-order, no paint
/// order, and no containment relation between two records that overlap.
pub fn addresses_at(snapshot: &Snapshot, point: Point) -> (Vec<Address>, Cost) {
    let mut hits = Vec::new();
    let mut cost = Cost::default();
    for a in snapshot.addresses() {
        cost.records_scanned += 1;
        if let Some(r) = snapshot.placement().rect(a) {
            cost.rects_resolved += 1;
            if contains(r, point) {
                hits.push(a);
            }
        }
    }
    hits.sort();
    (hits, cost)
}

/// Every record whose box intersects the viewport.
pub fn visible_set(snapshot: &Snapshot, viewport: Rect) -> (Vec<Address>, Cost) {
    let mut visible = Vec::new();
    let mut cost = Cost::default();
    for a in snapshot.addresses() {
        cost.records_scanned += 1;
        if let Some(r) = snapshot.placement().rect(a) {
            cost.rects_resolved += 1;
            if intersects(r, viewport) {
                visible.push(a);
            }
        }
    }
    visible.sort();
    (visible, cost)
}

/// Every record the paginated target puts on one page.
///
/// Not expressible as a rectangle query, which is the finding: a page is a
/// membership list a record carries, and two records on different pages can
/// hold the same box. So this reads `fragments` rather than geometry, and a
/// consumer that wanted "what is visible on page 2" would have to intersect two
/// unrelated answers itself.
pub fn on_fragment(snapshot: &Snapshot, fragment: u32) -> (Vec<Address>, Cost) {
    let mut on = Vec::new();
    let mut cost = Cost::default();
    for a in snapshot.addresses() {
        cost.records_scanned += 1;
        if let Some(n) = snapshot.get(a)
            && n.fragments.contains(&fragment)
        {
            on.push(a);
        }
    }
    on.sort();
    (on, cost)
}

/// The frontend's inverse: a caret offset in its own source, to the record that
/// produced the text there.
///
/// This is the frontend's to answer -- it owns the source and the IR never sees
/// it. Written here to find out what the *contract* would have to require, and
/// what a frontend pays to provide it.
pub fn address_for_offset(
    ast: &frontend_conformance::fixture::MdAst,
    offset: usize,
) -> Option<u64> {
    ast.nodes
        .iter()
        .find(|n| offset >= n.span.0 && offset < n.span.1)
        .map(|n| n.id)
}

/// The same caret, as a UTF-16 host counts it.
///
/// A Swift `String.Index`, a Kotlin `CharSequence`, and a JavaScript string all
/// count UTF-16 code units; the frontend counts UTF-8 bytes. Converting means
/// walking the source from the start, which is why an editor doing it per caret
/// move is doing `O(document)` work at keystroke rate.
pub fn utf16_offset_of(source: &str, byte_offset: usize) -> (usize, Cost) {
    let mut units = 0;
    let mut cost = Cost::default();
    for (at, ch) in source.char_indices() {
        if at >= byte_offset {
            break;
        }
        cost.records_scanned += 1;
        units += ch.len_utf16();
    }
    (units, cost)
}

/// And back, which an editor needs to place a caret the frontend named.
pub fn byte_offset_of(source: &str, utf16_offset: usize) -> (usize, Cost) {
    let mut units = 0;
    let mut cost = Cost::default();
    for (at, ch) in source.char_indices() {
        if units >= utf16_offset {
            return (at, cost);
        }
        cost.records_scanned += 1;
        units += ch.len_utf16();
    }
    (source.len(), cost)
}
