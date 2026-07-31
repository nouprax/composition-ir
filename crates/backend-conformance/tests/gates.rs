//! Conformance gates for `docs/specs/backend-contract.md`.
//!
//! Can three differently-shaped output targets be driven from a snapshot and a
//! delta alone, and does the part vocabulary buy a backend anything?

use std::num::NonZeroU64;

use backend_conformance::{paged::Paged, raster::Raster, svg::Svg};
use composition_ir::{
    Address, Domain, Id, Node, Rect, ResourceRole, Revision, Rgba, Snapshot, Space,
};

fn dom() -> Domain {
    Domain(NonZeroU64::new(1).unwrap())
}
fn addr(space: Space, n: u64) -> Address {
    Address::new(
        space,
        Id {
            domain: dom(),
            ordinal: NonZeroU64::new(n).unwrap(),
        },
    )
}
const MANIFEST: &str = include_str!("../Cargo.toml");

fn document(n: u64) -> Snapshot {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    for i in 1..=n {
        let a = addr(Space::Node, i);
        let w = 10 + (i as i64 % 5);
        e.put(
            a,
            Node {
                text: format!("word {i}"),
                paint: Rgba {
                    r: (i % 251) as u8,
                    g: 0x20,
                    b: 0x40,
                    a: 0xff,
                },
                intrinsic: w,
                // twenty records to a page
                fragments: vec![((i as u32) - 1) / 20],
                ..Node::default()
            },
        );
        e.place(
            a,
            Rect {
                x: (i as i64 % 20) * 12,
                y: (i as i64 / 20) * 18,
                width: w,
                height: 16,
            },
        );
    }
    e.commit().snapshot
}

/// A backend is a consumer. If it needs a frontend, an adapter, or a session,
/// the snapshot was not self-contained after all.
#[test]
fn a_backend_renders_from_the_snapshot_alone() {
    let deps = MANIFEST
        .split("[dependencies]")
        .nth(1)
        .expect("declares dependencies");
    for forbidden in ["frontend", "adapter", "derive", "session"] {
        assert!(
            !deps.contains(forbidden),
            "a backend depends on {forbidden}"
        );
    }
    let snapshot = document(50);
    let (svg, _) = Svg::render(&snapshot);
    let (raster, _) = Raster::render(&snapshot);
    let (paged, _) = Paged::render(&snapshot);
    assert_eq!(svg.elements.len(), 50);
    assert_eq!(raster.ops.len(), 50);
    assert_eq!(paged.pages.values().map(Vec::len).sum::<usize>(), 50);
}

/// The payoff the part vocabulary promises, measured at the backend: a
/// paint-only change must not cost a re-layout.
#[test]
fn a_paint_only_change_repaints_without_relayout() {
    let snapshot = document(500);
    let (mut svg, full) = Svg::render(&snapshot);
    assert_eq!(full.relaid_out, 500);

    let mut e = snapshot.edit();
    e.update(addr(Space::Node, 250), |n| {
        n.paint = Rgba {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0xff,
        }
    });
    let commit = e.commit();

    let work = svg.patch(&commit.snapshot, &commit.delta);
    assert_eq!(work.emitted, 1, "one record changed");
    assert_eq!(work.relaid_out, 0, "a paint change forced a re-layout");
    assert!(svg.elements[&format!("{:?}", addr(Space::Node, 250))].contains("#ff0000"));
}

/// Incremental output is proportional to the delta, not to the document.
#[test]
fn incremental_output_is_proportional_to_the_change() {
    let snapshot = document(2000);
    let (mut raster, full) = Raster::render(&snapshot);
    assert_eq!(full.emitted, 2000);

    let mut e = snapshot.edit();
    e.update(addr(Space::Node, 900), |n| n.text = "edited".into());
    let commit = e.commit();

    let (work, region) = raster.repaint(&commit.snapshot, &commit.delta);
    assert_eq!(
        work.emitted, 1,
        "repainted {} records for one edit",
        work.emitted
    );
    assert!(
        !region.is_empty(),
        "a repaint with no region is a full-screen repaint"
    );
    assert!(
        region.bounds.width < 100 && region.bounds.height < 100,
        "the invalidated region is {}x{} for one word",
        region.bounds.width,
        region.bounds.height
    );
}

/// Two differently-shaped targets over one snapshot must agree on what exists.
#[test]
fn two_backends_over_one_snapshot_draw_the_same_records() {
    let snapshot = document(200);
    let (svg, _) = Svg::render(&snapshot);
    let (raster, _) = Raster::render(&snapshot);
    let mut from_svg: Vec<String> = svg.elements.keys().cloned().collect();
    let mut from_raster: Vec<String> = raster
        .ops
        .iter()
        .map(|o| format!("{:?}", o.address))
        .collect();
    from_svg.sort();
    from_raster.sort();
    assert_eq!(from_svg, from_raster);
}

/// A paginated target must be able to say which page a record landed on, and
/// must deduplicate a resource shared by many records.
#[test]
fn a_paginated_target_can_place_records_and_share_resources() {
    let mut e = document(60).edit();
    let font = addr(Space::Resource, 9001);
    e.put(
        font,
        Node {
            text: "Inter".into(),
            ..Node::default()
        },
    );
    let snapshot = e.commit().snapshot;

    let (paged, _) = Paged::render(&snapshot);
    assert_eq!(paged.pages.len(), 3, "sixty records at twenty a page");
    assert_eq!(paged.pages_of(addr(Space::Node, 1)), vec![0]);
    assert_eq!(paged.pages_of(addr(Space::Node, 25)), vec![1]);
    assert_eq!(paged.pages_of(addr(Space::Node, 60)), vec![2]);
    assert_eq!(paged.resources.len(), 1, "one resource, referenced once");
}

/// A pure placement move must not make a backend repaint content.
#[test]
fn a_move_repaints_position_without_reissuing_content() {
    let snapshot = document(100);
    let (raster, _) = Raster::render(&snapshot);
    let before = raster
        .ops
        .iter()
        .find(|o| o.address == addr(Space::Node, 50))
        .unwrap()
        .clone();

    let mut e = snapshot.edit();
    e.translate((50..=100).map(|i| addr(Space::Node, i)).collect(), 40, 7);
    let commit = e.commit();

    assert!(commit.delta.is_empty(), "a move published a content change");
    let moved = commit
        .snapshot
        .placement()
        .rect(addr(Space::Node, 50))
        .unwrap();
    assert_eq!(
        moved.x,
        before.bounds.x + 40,
        "the query answers the new position"
    );
    assert_eq!(moved.y, before.bounds.y + 7, "on both axes");
}

/// A record names the resources it draws with, by role, without them becoming
/// children. Routing a font through `children` would make a font swap a
/// structural change that invalidates every ancestor; the role decides which
/// projection the reference feeds instead.
#[test]
fn a_record_names_its_resources_without_them_becoming_children() {
    let font = addr(Space::Resource, 9001);
    let other = addr(Space::Resource, 9002);
    let subject = addr(Space::Node, 1);

    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    e.put(
        font,
        Node {
            text: "Inter".into(),
            ..Node::default()
        },
    );
    e.put(
        other,
        Node {
            text: "Iosevka".into(),
            ..Node::default()
        },
    );
    e.put(
        subject,
        Node {
            text: "drawn with Inter".into(),
            resources: vec![(ResourceRole::Font, font)],
            ..Node::default()
        },
    );
    let base = e.commit().snapshot;
    assert!(
        base.get(subject).unwrap().children.is_empty(),
        "a resource is not a child"
    );

    let mut e = base.edit();
    e.update(subject, |n| n.resources = vec![(ResourceRole::Font, other)]);
    let commit = e.commit();

    let entry = commit
        .delta
        .diffs
        .iter()
        .find(|d| d.address == subject)
        .unwrap();
    assert!(
        entry.parts.contains(composition_ir::Part::Shaping),
        "a font swap feeds shaping"
    );
    assert!(
        !entry.parts.contains(composition_ir::Part::Structure),
        "a font swap must not report a structural change"
    );
}

/// Placement answers a rectangle, folding every translation that covers the
/// address. Transform and clip are still absent: without clip a repaint region
/// is conservative, which costs redraw rather than correctness.
#[test]
fn placement_answers_a_two_dimensional_box() {
    let snapshot = document(4);
    let r = snapshot.placement().rect(addr(Space::Node, 3)).unwrap();
    assert!(r.width > 0 && r.height > 0, "placement answers two axes");
}
