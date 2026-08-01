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
    Point, addresses_at, addresses_for_offset, byte_offset_of, on_fragment, utf16_offset_of,
    visible_set, visible_set_single_pass,
};
use workload_conformance::document::{PAGE_HEIGHT, VIEWPORT, address, ast, layout, snapshot};

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

/// **Finding 3: culling is the right shape, and a consumer cannot compute it as
/// cheaply as the IR could.**
///
/// This started as "the query buys a consumer nothing", on a measurement that
/// counted calls to `Placement::rect` as if each were `O(1)`. It is not: it
/// scans the placement's boxes for a match and then walks every translation's
/// cover list. A loop calling it once per live address is quadratic in the
/// document.
///
/// A consumer has no way out of that loop, because `Placement` exposes `len`
/// and `rect` and nothing that iterates. The side that owns the vector resolves
/// the same answer in one pass. So the query is not a convenience wrapper over
/// what a consumer can already write -- it is asymptotically cheaper, before any
/// index is built at all.
#[test]
fn culling_costs_a_consumer_more_than_it_would_cost_the_ir() {
    let s = doc();
    let (visible, consumer) = visible_set(&s, VIEWPORT);

    assert_eq!(visible.len(), 4, "four of six records are on screen");
    assert_eq!(consumer.records_scanned, s.len());
    assert_eq!(consumer.rect_lookups, s.len());
    assert!(
        consumer.box_comparisons > consumer.rect_lookups,
        "each lookup is a scan, so the work is superlinear in the document: \
         {} comparisons for {} lookups",
        consumer.box_comparisons,
        consumer.rect_lookups
    );

    // The same answer, computed once over the placement the IR owns.
    let placed: Vec<_> = layout()
        .iter()
        .map(|p| (address(dom(), p.id), p.rect))
        .collect();
    let (same, inside) = visible_set_single_pass(&placed, VIEWPORT);
    assert_eq!(same, visible, "the two agree on the answer");
    assert!(
        inside.box_comparisons < consumer.box_comparisons,
        "and the one that can see the placement does strictly less work: {} vs {}",
        inside.box_comparisons,
        consumer.box_comparisons
    );

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

/// **Finding 5: the inverse has the same shape as the hit test -- one caret is
/// several nodes -- and unlike the hit test, the frontend can rank them.**
///
/// Forward is already carried: `source_link` is a stable join key and the round
/// trip lands on the right node. The inverse is the frontend's, and nothing in
/// `frontend-contract.md` requires it.
///
/// A caret inside the inline formula is inside the formula *and* inside the
/// paragraph containing it, which is the ordinary Markdown shape and one of the
/// three workloads. So `offset → address` is no more singular than
/// `point → address`. The difference is that the frontend has a tree and can
/// order the answers by it, where the IR has no order covering the set it would
/// have to rank -- so a contract may require an order here, and must say which.
#[test]
fn one_caret_is_several_nodes_and_the_frontend_is_what_can_rank_them() {
    let s = doc();
    let ast = ast();

    // Rendered element -> record -> frontend node.
    let clicked = address(dom(), 3);
    let link = s.get(clicked).unwrap().source_link.clone();
    assert_eq!(link, "md:3");
    let paragraph = ast
        .nodes
        .iter()
        .find(|n| format!("md:{}", n.id) == link)
        .expect("the join key names a live node");

    // A caret in the paragraph's prose is in the paragraph alone.
    let in_prose = paragraph.span.0 + 1;
    assert_eq!(addresses_for_offset(&ast, in_prose), vec![3]);

    // A caret in the inline formula is in two nodes, innermost first.
    let formula = ast.nodes.iter().find(|n| n.id == 4).unwrap();
    assert!(
        formula.span.0 > paragraph.span.0 && formula.span.1 <= paragraph.span.1,
        "the fixture nests the formula inside the paragraph"
    );
    assert_eq!(
        addresses_for_offset(&ast, formula.span.0 + 1),
        vec![4, 3],
        "a singular answer here would be whichever the frontend happened to list first"
    );

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
    let (units, _) = utf16_offset_of(&ast.source, third.span.0).expect("a scalar boundary");
    assert_eq!(units, third.span.0, "ASCII prefix, so far identical");

    // ...and after it they do not.
    let after = third.span.1;
    let (units_after, cost) = utf16_offset_of(&ast.source, after).expect("a scalar boundary");
    assert!(
        units_after < after,
        "each of the four multi-byte characters is three UTF-8 bytes and one UTF-16 unit"
    );
    assert!(
        cost.records_scanned >= units,
        "the conversion walked the source from the start"
    );

    // On boundaries the two are inverses.
    let (back, _) = byte_offset_of(&ast.source, units_after).expect("a scalar boundary");
    assert_eq!(back, after);
}

/// **Finding 7: not every offset has an image in the other encoding, and a
/// profile that rounds silently moves the caret.**
///
/// Both encodings can name a position inside one character: a byte offset can
/// land inside a multi-byte UTF-8 sequence, and a UTF-16 offset can land
/// between the surrogate halves of a supplementary-plane character. Neither has
/// an image in the other.
///
/// The first version of these helpers rounded forward without saying so, which
/// made the round trip above look like an identity when it was one only on
/// boundaries -- `1` came back as `2`. So the profile has a decision to make
/// that is not about performance: reject, or round and say in which direction.
/// It has to be stated at the boundary, because a consumer cannot tell a
/// rounded answer from an exact one.
#[test]
fn an_offset_inside_a_character_has_no_image_in_the_other_encoding() {
    let source = "😀x";

    // The emoji is one scalar: four UTF-8 bytes and two UTF-16 units.
    assert_eq!(source.chars().next().unwrap().len_utf8(), 4);
    assert_eq!(source.chars().next().unwrap().len_utf16(), 2);

    // Between the surrogate halves.
    let split = byte_offset_of(source, 1);
    assert_eq!(
        split.unwrap_err().previous_boundary,
        0,
        "UTF-16 offset 1 is inside the pair, and the boundary before it is 0"
    );

    // And inside the UTF-8 sequence.
    let mid = utf16_offset_of(source, 2);
    assert_eq!(mid.unwrap_err().previous_boundary, 0);

    // The boundaries either side do convert, so this is a hole in the mapping
    // rather than a failure of it.
    assert_eq!(byte_offset_of(source, 0).unwrap().0, 0);
    assert_eq!(byte_offset_of(source, 2).unwrap().0, 4);
    assert_eq!(utf16_offset_of(source, 4).unwrap().0, 2);
}

/// **Finding 8: whether culling may replace the delta depends on what the
/// consumer keeps, and only one of the two shapes is at risk.**
///
/// The first reading of this was that a consumer must never filter its delta by
/// the visible set. That is too strong. §1 makes the snapshot the complete
/// result and a consumer may ignore every delta, so a consumer that materializes
/// only the viewport is safe: it drops the off-screen diff and repopulates from
/// the current snapshot when the record scrolls in.
///
/// The one that goes stale is the consumer that **retains** what it has scrolled
/// past and filters its updates anyway -- it keeps the old record and never
/// hears about the change. Both are built here, because the difference is the
/// whole rule and asserting that the producer emitted a diff does not show it.
#[test]
fn filtering_a_delta_by_the_visible_set_is_safe_only_if_nothing_off_screen_is_retained() {
    let s = doc();

    // A paint-only edit to a record on screen: repaint is proportional.
    let (mut raster, _) = Raster::render(&s);
    let mut edit = s.edit();
    edit.update(address(dom(), 2), |n| n.paint.r = 0xff);
    let commit = edit.commit();
    let (work, region) = raster.repaint(&commit.snapshot, &commit.delta);
    assert_eq!(work.emitted, 1, "one record repainted, not six");
    assert_eq!(work.relaid_out, 0);
    assert!(!region.is_empty());
    assert!(commit.delta.diffs[0].parts.contains(Part::Paint));

    // Now an edit to a record nobody can see. The IR publishes it either way.
    let s = commit.snapshot;
    let (visible, _) = visible_set(&s, VIEWPORT);
    let off_screen = address(dom(), 6);
    assert!(!visible.contains(&off_screen));
    let mut edit = s.edit();
    edit.update(off_screen, |n| n.paint.b = 0xff);
    let commit = edit.commit();
    assert_eq!(commit.delta.diffs.len(), 1);

    // A consumer that retains the whole document and filters by the visible set
    // keeps the record it already had, and it is now wrong.
    let mut retained = Raster::render(&s).0;
    let filtered: Vec<_> = commit
        .delta
        .diffs
        .iter()
        .filter(|d| visible.contains(&d.address))
        .copied()
        .collect();
    assert!(filtered.is_empty(), "the only diff was off screen");
    let stale = retained
        .ops
        .iter()
        .find(|o| o.address == off_screen)
        .expect("it retained the off-screen record")
        .fill;
    let current = commit.snapshot.get(off_screen).unwrap().paint;
    assert_ne!(stale, current, "and what it retained is stale");

    // A consumer that keeps only the viewport is not exposed to that at all: it
    // has nothing to go stale, and rebuilds from the snapshot on scroll.
    let scrolled = Rect {
        x: 0,
        y: 120,
        width: 200,
        height: 60,
    };
    let (now_visible, _) = visible_set(&commit.snapshot, scrolled);
    assert!(now_visible.contains(&off_screen));
    let rebuilt = Raster::render(&commit.snapshot).0;
    assert_eq!(
        rebuilt
            .ops
            .iter()
            .find(|o| o.address == off_screen)
            .unwrap()
            .fill,
        current,
        "materialized from the snapshot, it is current without ever seeing the diff"
    );

    // The retained consumer is only saved by not filtering.
    retained.repaint(&commit.snapshot, &commit.delta);
    assert_eq!(
        retained
            .ops
            .iter()
            .find(|o| o.address == off_screen)
            .unwrap()
            .fill,
        current
    );
}
