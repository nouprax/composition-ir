//! Conformance gates for `docs/specs/derivation.md`.
//!
//! Does recording what a computation read, and intersecting that with the
//! commit diff, invalidate everything it must and nothing it need not?

use std::num::NonZeroU64;

use composition_derive::{Engine, Reader};
use composition_ir::{Address, Domain, Id, Node, Part, Revision, Rgba, Snapshot, Space};

fn dom() -> Domain {
    Domain(NonZeroU64::new(1).unwrap())
}
fn addr(n: u64) -> Address {
    Address::new(
        Space::Node,
        Id {
            domain: dom(),
            ordinal: NonZeroU64::new(n).unwrap(),
        },
    )
}
fn base(n: u64) -> Snapshot {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    for i in 1..=n {
        e.put(
            addr(i),
            Node {
                intrinsic: i as i64,
                paint: Rgba {
                    r: i as u8,
                    g: 0,
                    b: 0,
                    a: 0xff,
                },
                text: format!("t{i}"),
                ..Node::default()
            },
        );
    }
    e.commit().snapshot
}

fn total_width(r: &mut Reader) -> i64 {
    (1..=6u64)
        .filter_map(|i| r.part(addr(i), Part::IntrinsicLayout).map(|n| n.intrinsic))
        .sum()
}
fn paint_len(r: &mut Reader) -> i64 {
    (1..=6u64)
        .filter_map(|i| r.part(addr(i), Part::Paint).map(|n| n.paint.r as i64))
        .sum()
}
fn text_of_one(r: &mut Reader) -> i64 {
    r.part(addr(3), Part::Text)
        .map(|n| n.text.len() as i64)
        .unwrap_or(0)
}
/// The classic silent-staleness shape: a conclusion drawn from absence.
fn slot_is_free(r: &mut Reader) -> i64 {
    i64::from(r.is_absent(addr(99)))
}

fn engine() -> Engine<i64> {
    let mut e = Engine::new();
    e.define("total_width", Box::new(total_width));
    e.define("paint_len", Box::new(paint_len));
    e.define("text_of_one", Box::new(text_of_one));
    e.define("slot_is_free", Box::new(slot_is_free));
    e
}
const KEYS: [&str; 4] = ["total_width", "paint_len", "text_of_one", "slot_is_free"];

fn fresh(key: &str, s: &Snapshot) -> i64 {
    let mut r = Reader::new(s);
    match key {
        "total_width" => total_width(&mut r),
        "paint_len" => paint_len(&mut r),
        "text_of_one" => text_of_one(&mut r),
        _ => slot_is_free(&mut r),
    }
}

#[test]
fn invalidation_has_no_false_negatives() {
    let mut seed = 0xC0FFEEu64;
    let mut rnd = move |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    for _ in 0..200 {
        let mut snap = base(6);
        let mut eng = engine();
        for k in KEYS {
            eng.get(k, &snap);
        }
        for _ in 0..6 {
            let mut e = snap.edit();
            let i = rnd(6) + 1;
            match rnd(5) {
                0 => {
                    e.update_if_live(addr(i), |n| n.intrinsic = rnd(50) as i64);
                }
                1 => {
                    e.update_if_live(addr(i), |n| {
                        n.paint = Rgba {
                            r: rnd(200) as u8,
                            g: 1,
                            b: 2,
                            a: 0xff,
                        }
                    });
                }
                2 => {
                    e.update_if_live(addr(i), |n| n.text = "x".repeat(rnd(6) as usize + 1));
                }
                3 => {
                    e.put(addr(99), Node::default());
                }
                _ => {
                    e.remove(addr(99));
                }
            }
            let commit = e.commit();
            eng.invalidate(&commit.delta, &commit.snapshot);
            snap = commit.snapshot;

            for k in KEYS {
                assert_eq!(
                    eng.get(k, &snap),
                    fresh(k, &snap),
                    "{k} kept a stale value across a commit"
                );
            }
        }
    }
}

#[test]
fn a_paint_change_does_not_invalidate_a_measurement_reader() {
    let snap = base(6);
    let mut eng = engine();
    for k in KEYS {
        eng.get(k, &snap);
    }
    let before = eng.recomputations;

    let mut e = snap.edit();
    e.update(addr(2), |n| {
        n.paint = Rgba {
            r: 9,
            g: 9,
            b: 9,
            a: 0xff,
        }
    });
    let commit = e.commit();
    let dropped = eng.invalidate(&commit.delta, &commit.snapshot);

    assert_eq!(dropped, 1, "only the paint reader should have been dropped");
    assert!(
        eng.is_cached("total_width"),
        "a measurement reader survived a paint change"
    );
    assert!(eng.is_cached("text_of_one"));
    assert!(!eng.is_cached("paint_len"));

    for k in KEYS {
        eng.get(k, &commit.snapshot);
    }
    assert_eq!(
        eng.recomputations - before,
        1,
        "recomputation must be proportional to affected readers, not to all readers"
    );
}

#[test]
fn a_conclusion_drawn_from_absence_is_invalidated_when_the_address_appears() {
    let snap = base(6);
    let mut eng = engine();
    assert_eq!(eng.get("slot_is_free", &snap), 1);

    let mut e = snap.edit();
    e.put(
        addr(99),
        Node {
            text: "now here".into(),
            ..Node::default()
        },
    );
    let commit = e.commit();
    eng.invalidate(&commit.delta, &commit.snapshot);

    assert!(
        !eng.is_cached("slot_is_free"),
        "a negative observation went stale silently: this is the failure a positive-only \
         dependency algebra cannot see"
    );
    assert_eq!(eng.get("slot_is_free", &commit.snapshot), 0);
}

#[test]
fn an_unrelated_edit_invalidates_nothing() {
    let snap = base(6);
    let mut eng = engine();
    for k in KEYS {
        eng.get(k, &snap);
    }
    let mut e = snap.edit();
    // address 7 is outside every recipe's read set
    e.put(
        addr(7),
        Node {
            intrinsic: 999,
            ..Node::default()
        },
    );
    let commit = e.commit();

    assert!(!commit.delta.is_empty(), "the commit did publish something");
    assert_eq!(
        eng.invalidate(&commit.delta, &commit.snapshot),
        0,
        "a commit outside every read set invalidated a cell"
    );
}

/// The derivation contract cites a gate for every normative rule; this checks
/// the citations resolve.
#[test]
fn every_gate_the_spec_cites_exists() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/derivation.md"
    ))
    .expect("the contract must be readable from the suite that enforces it");
    let source = include_str!("gates.rs");
    let mut cited = 0usize;
    let mut rest = spec.as_str();
    while let Some(open) = rest.find("[`") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find("`]") else { break };
        let name = &rest[..close];
        if name.contains('_') && name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            assert!(
                source.contains(&format!("fn {name}(")),
                "spec cites a missing gate: {name}"
            );
            cited += 1;
        }
        rest = &rest[close + 2..];
    }
    assert!(cited >= 4, "the contract cites only {cited} gates");
}
