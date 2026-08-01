//! The proof of concept, run against all three output targets.
//!
//! This file is evidence, not contract. Each test drives a proposed query
//! through a frontend, the IR, and `svg`, `raster`, and `paged`, and asserts
//! what the run actually showed -- including where a proposal turned out not to
//! be expressible. A finding recorded as a passing assertion is a finding that
//! stays true; one recorded in a commit message is one nobody can re-run.

use composition_ir::{Domain, Node, Part, Rect, Snapshot};

use backend_conformance::{paged::Paged, raster::Raster, svg::Svg};
use workload_conformance::candidate::{
    Point, address_for_offset, addresses_at, byte_offset_of, on_fragment, utf16_offset_of,
    visible_set,
};
use workload_conformance::document::{PAGE_HEIGHT, VIEWPORT, address, ast, snapshot};

fn dom() -> Domain {
    Domain(std::num::NonZeroU64::new(7).unwrap())
}

fn doc() -> Snapshot {
    snapshot(dom())
}

/// The chain end to end, before anything is asked of it: a frontend AST becomes
/// a snapshot that all three targets can render. If this cannot be built, no
/// finding below means anything.
#[test]
fn all_three_targets_render_the_same_document() {
    let s = doc();
    let (svg, svg_work) = Svg::render(&s);
    let (raster, raster_work) = Raster::render(&s);
    let (paged, paged_work) = Paged::render(&s);

    assert_eq!(s.len(), 6, "the fixture document is six records");
    assert_eq!(svg_work.emitted, 6);
    assert_eq!(raster_work.emitted, 6);
    assert_eq!(paged_work.emitted, 6);
    assert_eq!(svg.elements.len(), 6);
    assert_eq!(raster.ops.len(), 6);
    // Two pages, because one record straddles the break and lands on both.
    assert_eq!(paged.pages.keys().copied().collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(paged.pages_of(address(dom(), 5)), vec![0, 1]);
}

/// **Finding 1: a point is not enough for a paginated target.**
///
/// `Placement::rect` gives one box per address, in one coordinate space. The
/// paginated target puts record 5 on two pages, and both of them are the same
/// box -- so a point that hits record 5 hits it on *both* pages, and the query
/// cannot say which one the reader clicked. A hit test on page 1 and the same
/// hit test on page 0 are indistinguishable.
///
/// This is not a defect in the prototype: there is nothing in the IR to consult.
/// A `cir_snapshot_address_at(point)` as proposed would be answerable for `svg`
/// and `raster` and meaningless for `paged`.
#[test]
fn a_point_alone_cannot_name_a_page() {
    let s = doc();
    let spanning = address(dom(), 5);

    // A point inside the record that straddles the break.
    let (hits, _) = addresses_at(&s, Point { x: 10, y: 95 });
    assert!(hits.contains(&spanning));

    // The paginated target has it on both pages, and the geometry that answered
    // the hit test carries no way to tell them apart.
    let (paged, _) = Paged::render(&s);
    assert_eq!(paged.pages_of(spanning), vec![0, 1]);
    let r = s.placement().rect(spanning).expect("placed");
    assert!(
        r.y < PAGE_HEIGHT && r.y + r.height > PAGE_HEIGHT,
        "the record straddles the break, and one Rect describes both halves"
    );
}

/// **Finding 2: a point can hit two records, and the one ordering the IR has
/// does not cover the set that must be ordered.**
///
/// Records 2 and 3 overlap, so a click in the overlap is two answers and
/// something has to rank them. There *is* an order in the IR -- `children` is
/// ordered, and a painter's algorithm over a tree walk is the usual reading of
/// it. It does not reach far enough.
///
/// §9 states the reason outright: roots are entry points, not the census, and
/// the live set is deliberately larger than what a `children` walk reaches. Here
/// one record of six is reachable from the roots, and the two that overlap are
/// both outside it. Ordering the hits by tree position would leave most of the
/// document unordered, and `cir_snapshot_addresses` -- the call a hit test would
/// have to scan -- publishes its order as *unspecified*.
///
/// So a query returning one `Address` would be inventing a rule. Returning every
/// hit leaves the choice with the consumer that has one: `svg` takes the DOM's
/// answer, `raster` has no scene graph and must be told.
#[test]
fn a_point_in_an_overlap_has_two_answers_and_no_order_that_covers_them() {
    let s = doc();
    let (hits, _) = addresses_at(&s, Point { x: 50, y: 40 });
    assert_eq!(
        hits.len(),
        2,
        "the fixture overlaps two records precisely so this is not one answer"
    );
    assert!(hits.contains(&address(dom(), 2)) && hits.contains(&address(dom(), 3)));

    // Neither is the other's child, and neither is reachable from a root, so a
    // tree walk ranks neither of them.
    let (a, b) = (s.get(hits[0]).unwrap(), s.get(hits[1]).unwrap());
    assert!(!a.children.contains(&hits[1]) && !b.children.contains(&hits[0]));

    let mut reachable = Vec::new();
    let mut stack: Vec<_> = s.roots().to_vec();
    while let Some(next) = stack.pop() {
        if reachable.contains(&next) {
            continue;
        }
        reachable.push(next);
        if let Some(n) = s.get(next) {
            stack.extend(n.children.iter().copied());
        }
    }
    assert_eq!(
        reachable.len(),
        1,
        "one record of six is reachable from the roots, which is what §9 says to expect"
    );
    assert!(
        !reachable.contains(&hits[0]) && !reachable.contains(&hits[1]),
        "and the two that need ranking are not in it"
    );
}

/// **Finding 3: culling works, and costs the whole document to compute.**
///
/// The viewport holds four of six records, and both `svg` and `raster` can use
/// that set directly -- so the query's *shape* is right for them. What it costs
/// is the problem: every record is scanned and every rect resolved, per frame,
/// because nothing in the IR is indexed by geometry.
#[test]
fn a_viewport_query_is_the_right_shape_and_the_wrong_cost() {
    let s = doc();
    let (visible, cost) = visible_set(&s, VIEWPORT);

    assert_eq!(visible.len(), 4, "four of six records are on screen");
    assert_eq!(
        cost.records_scanned,
        s.len(),
        "culling scanned the whole document to find them"
    );
    assert_eq!(cost.rects_resolved, s.len());

    // Both viewport-shaped targets can consume it as-is.
    let (raster, _) = Raster::render(&s);
    let drawn: Vec<_> = raster
        .ops
        .iter()
        .filter(|o| visible.contains(&o.address))
        .collect();
    assert_eq!(drawn.len(), 4);
    let (svg, _) = Svg::render(&s);
    assert_eq!(
        visible
            .iter()
            .filter(|a| svg.elements.contains_key(&format!("{a:?}")))
            .count(),
        4
    );
}

/// **Finding 4: fragment membership is not derivable from geometry.**
///
/// This one was expected to fall out of the flowing document above and did not:
/// where a publisher lays pages out as bands of one coordinate space, a
/// rectangle over page 1 and the set of records on page 1 are the *same set*.
/// Recorded here because it is the more useful half of the finding -- the two
/// agreeing is a property of one layout, not a rule the IR states.
///
/// Nothing ties `fragments` to `Rect`. A running header is the ordinary
/// construct that separates them: it is on every page and has one box, so it is
/// on page 1 while its geometry is not. `paged` needs it on page 1; a rectangle
/// query over page 1 will never return it.
///
/// So "what is on page N" and "what is inside this rectangle" are two
/// questions, and a viewport query answers only the second.
#[test]
fn a_page_is_a_membership_list_and_not_a_region() {
    let s = doc();

    // In a plain flowing layout the two agree, which is why the rectangle
    // reading looks sufficient until something is repeated.
    let (on_page_1, _) = on_fragment(&s, 1);
    let page_1 = Rect {
        x: 0,
        y: PAGE_HEIGHT,
        width: 1_000,
        height: PAGE_HEIGHT,
    };
    let (by_rect, _) = visible_set(&s, page_1);
    assert_eq!(
        by_rect, on_page_1,
        "a flowing document lays pages out as bands, so here the two coincide"
    );

    // A running header does not flow. It is published on both pages and placed
    // once, which is exactly what `fragments` is for.
    let header = address(dom(), 9);
    let mut edit = s.edit();
    edit.put(
        header,
        Node {
            text: "running header".to_string(),
            fragments: vec![0, 1],
            ..Node::default()
        },
    );
    edit.place(
        header,
        Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 8,
        },
    );
    let s = edit.commit().snapshot;

    let (on_page_1, _) = on_fragment(&s, 1);
    let (by_rect, _) = visible_set(&s, page_1);
    assert!(
        on_page_1.contains(&header),
        "the paginated target must draw the header on page 1"
    );
    assert!(
        !by_rect.contains(&header),
        "and no rectangle over page 1 contains its box"
    );
    assert_ne!(by_rect, on_page_1);
}

/// **Finding 5: select → editor works today, and only in one direction.**
///
/// Forward is already carried: a record's `source_link` is a stable join key,
/// the frontend owns the source, and the round trip lands on the right node.
/// The inverse -- a caret offset back to the record -- is the frontend's, is
/// not required by `frontend-contract.md`, and the fixture happens to be able
/// to answer it only because its nodes keep spans.
#[test]
fn the_join_key_round_trips_but_the_contract_only_requires_one_direction() {
    let s = doc();
    let ast = ast();

    // Rendered element -> record -> frontend node.
    let clicked = address(dom(), 3);
    let link = s.get(clicked).unwrap().source_link.clone();
    assert_eq!(link, "md:3");
    let node = ast
        .nodes
        .iter()
        .find(|n| format!("md:{}", n.id) == link)
        .expect("the join key names a live node");

    // Caret -> record, which is the direction nothing requires.
    let caret = node.span.0 + 1;
    assert_eq!(address_for_offset(&ast, caret), Some(3));

    // And it is answerable only because the frontend kept a span per node. The
    // IR carries no offset, by design -- a coordinate on a record would make
    // every later record change on any earlier edit.
    assert!(
        s.get(clicked).unwrap().text_map.is_empty(),
        "the IR holds no source offset for this record"
    );
}

/// **Finding 6: a UTF-16 host pays per caret move, and the cost is the source.**
///
/// The frontend counts UTF-8 bytes; Swift, Kotlin, and JavaScript count UTF-16
/// code units. The two agree until the first non-ASCII character and diverge
/// after it, so the conversion is not a constant and cannot be cached as one.
/// Done the only way available today it walks the source from the start, which
/// an editor repeats on every caret move.
#[test]
fn utf8_and_utf16_offsets_diverge_and_converting_walks_the_source() {
    let ast = ast();
    let third = ast.nodes.iter().find(|n| n.id == 3).unwrap();

    // Up to the first non-ASCII byte the two agree...
    let (units, _) = utf16_offset_of(&ast.source, third.span.0);
    assert_eq!(units, third.span.0, "ASCII prefix, so far identical");

    // ...and after it they do not.
    let after = third.span.1;
    let (units_after, cost) = utf16_offset_of(&ast.source, after);
    assert!(
        units_after < after,
        "each of the four multi-byte characters is three UTF-8 bytes and one UTF-16 unit"
    );
    assert!(
        cost.records_scanned >= units,
        "the conversion walked the source from the start"
    );

    // The inverse agrees, so the profile is a mapping rather than an estimate.
    let (back, _) = byte_offset_of(&ast.source, units_after);
    assert_eq!(back, after);
}

/// **Finding 7: an edit's repaint is already proportional; culling does not
/// make it less so.**
///
/// The reason to be careful about adding a viewport query: the delta is already
/// the dirty set, and the raster target already repaints only what it names. A
/// culling query must not become a second path that a consumer uses *instead*
/// of the delta, or an edit off screen stops being observed at all.
#[test]
fn culling_narrows_a_repaint_without_replacing_the_delta() {
    let s = doc();
    let (mut raster, _) = Raster::render(&s);

    // A paint-only edit to a record that is on screen.
    let visible_target = address(dom(), 2);
    let mut edit = s.edit();
    edit.update(visible_target, |n| n.paint.r = 0xff);
    let commit = edit.commit();

    let (work, region) = raster.repaint(&commit.snapshot, &commit.delta);
    assert_eq!(work.emitted, 1, "one record repainted, not six");
    assert_eq!(work.relaid_out, 0);
    assert!(!region.is_empty());
    assert_eq!(commit.delta.diffs.len(), 1);
    assert!(commit.delta.diffs[0].parts.contains(Part::Paint));

    // The same edit to a record *outside* the viewport still publishes a diff.
    // A consumer that filtered its input by the visible set would drop it and
    // hold a stale record the moment it scrolled.
    let (visible, _) = visible_set(&commit.snapshot, VIEWPORT);
    let off_screen = address(dom(), 6);
    assert!(!visible.contains(&off_screen));
    let mut edit = commit.snapshot.edit();
    edit.update(off_screen, |n| n.paint.b = 0xff);
    let commit = edit.commit();
    assert_eq!(
        commit.delta.diffs.len(),
        1,
        "the IR publishes it whether or not anyone can see it, which is correct"
    );
}
