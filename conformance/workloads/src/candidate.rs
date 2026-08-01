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
/// Counting calls is what made the first version of this misleading, and the
/// mistake is worth keeping visible: `Placement::rect` is not `O(1)`. It scans
/// the placement's boxes for a match and then walks every translation's cover
/// list, so a loop calling it once per live address is quadratic in the
/// document, not linear. `box_comparisons` is the term that dominates, and a
/// measurement without it understates the work by a factor of the document.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub records_scanned: usize,
    pub rect_lookups: usize,
    /// Box comparisons the placement lookup performs, at its upper bound of one
    /// pass over the placement per call. A consumer cannot do better: nothing
    /// on `Placement` iterates, so there is no way to resolve many addresses in
    /// one pass from outside.
    pub box_comparisons: usize,
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
        cost.rect_lookups += 1;
        cost.box_comparisons += snapshot.placement().len();
        if let Some(r) = snapshot.placement().rect(a)
            && contains(r, point)
        {
            hits.push(a);
        }
    }
    hits.sort();
    (hits, cost)
}

/// Every record whose box intersects the viewport, as a consumer must compute
/// it: one placement lookup per live address, because nothing on `Placement`
/// iterates and there is no way in from outside to resolve many at once.
pub fn visible_set(snapshot: &Snapshot, viewport: Rect) -> (Vec<Address>, Cost) {
    let mut visible = Vec::new();
    let mut cost = Cost::default();
    for a in snapshot.addresses() {
        cost.records_scanned += 1;
        cost.rect_lookups += 1;
        cost.box_comparisons += snapshot.placement().len();
        if let Some(r) = snapshot.placement().rect(a)
            && intersects(r, viewport)
        {
            visible.push(a);
        }
    }
    visible.sort();
    (visible, cost)
}

/// The same answer, computed the way the IR could compute it: one pass over the
/// placement, resolving every box as it goes.
///
/// The PoC has to rebuild the placement to do this, because `Placement` exposes
/// `len` and `rect` and nothing else. That is the point of the comparison --
/// this is not an optimisation a consumer could apply, it is one only the side
/// that owns the vector can.
pub fn visible_set_single_pass(placed: &[(Address, Rect)], viewport: Rect) -> (Vec<Address>, Cost) {
    let mut visible = Vec::new();
    let mut cost = Cost::default();
    for (a, r) in placed {
        cost.box_comparisons += 1;
        if intersects(*r, viewport) {
            visible.push(*a);
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

/// The frontend's inverse: a caret offset in its own source, to the nodes whose
/// text covers it.
///
/// **Every** node, not the first one found. Spans nest -- a formula inside a
/// paragraph is one of the three workloads -- so a caret is routinely inside
/// more than one node, and `find` would return whichever the frontend happened
/// to list first. `frontend-contract.md` constrains neither AST shape nor node
/// inventory, so a singular answer would be a rule about a frontend rather than
/// a rule for frontends.
///
/// This is the frontend's to answer: it owns the source, and the IR never sees
/// it. Written here to find out what the *contract* would have to require.
pub fn addresses_for_offset(ast: &frontend_conformance::fixture::MdAst, offset: usize) -> Vec<u64> {
    let mut covering: Vec<&frontend_conformance::fixture::MdNode> = ast
        .nodes
        .iter()
        .filter(|n| offset >= n.span.0 && offset < n.span.1)
        .collect();
    // Innermost first: the narrowest span containing the caret is the most
    // specific thing the caret is in, and it is what an editor resolving a
    // selection wants first. Unlike the IR's hit test, the frontend *does* have
    // a ranking rule available -- its own tree -- so a contract can require an
    // order here where the IR cannot. It has to say which one, and this is the
    // proposal.
    covering.sort_by_key(|n| (n.span.1 - n.span.0, n.id));
    covering.iter().map(|n| n.id).collect()
}

/// An offset that does not fall on a scalar boundary.
///
/// Both encodings can name a position inside one character: a byte offset can
/// land inside a multi-byte UTF-8 sequence, and a UTF-16 offset can land
/// between the two surrogate units of a supplementary-plane character. Neither
/// has an image in the other encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotOnABoundary {
    pub offset: usize,
    /// The scalar boundary at or before it, which is what a rounding policy
    /// would return.
    pub previous_boundary: usize,
}

/// The same caret, as a UTF-16 host counts it.
///
/// A Swift `String.Index`, a Kotlin `CharSequence`, and a JavaScript string all
/// count UTF-16 code units; the frontend counts UTF-8 bytes. Converting means
/// walking the source from the start, which is why an editor doing it per caret
/// move is doing `O(document)` work at keystroke rate.
///
/// Returns an error rather than rounding. The first version of this silently
/// rounded, which made the round trip below look like an identity while it was
/// only an identity on boundaries -- and an encoding profile that rounds
/// silently moves a user's caret without saying so.
pub fn utf16_offset_of(source: &str, byte_offset: usize) -> Result<(usize, Cost), NotOnABoundary> {
    let mut units = 0;
    let mut cost = Cost::default();
    let mut previous = 0;
    for (at, ch) in source.char_indices() {
        if at == byte_offset {
            return Ok((units, cost));
        }
        if at > byte_offset {
            return Err(NotOnABoundary {
                offset: byte_offset,
                previous_boundary: previous,
            });
        }
        previous = at;
        cost.records_scanned += 1;
        units += ch.len_utf16();
    }
    if byte_offset == source.len() {
        Ok((units, cost))
    } else {
        Err(NotOnABoundary {
            offset: byte_offset,
            previous_boundary: previous,
        })
    }
}

/// And back, which an editor needs to place a caret the frontend named.
pub fn byte_offset_of(source: &str, utf16_offset: usize) -> Result<(usize, Cost), NotOnABoundary> {
    let mut units = 0;
    let mut cost = Cost::default();
    let mut previous = 0;
    for (at, ch) in source.char_indices() {
        if units == utf16_offset {
            return Ok((at, cost));
        }
        if units > utf16_offset {
            // The requested position was inside the surrogate pair just passed.
            return Err(NotOnABoundary {
                offset: utf16_offset,
                previous_boundary: previous,
            });
        }
        previous = units;
        cost.records_scanned += 1;
        units += ch.len_utf16();
    }
    if units == utf16_offset {
        Ok((source.len(), cost))
    } else {
        Err(NotOnABoundary {
            offset: utf16_offset,
            previous_boundary: previous,
        })
    }
}
