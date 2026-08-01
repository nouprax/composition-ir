//! Conformance gates for `docs/specs/composition-ir.md`.
//!
//! These are the executable form of the contract. The spec is frozen against
//! this file: a rule that cannot be checked here is a rule that will be
//! violated silently.

use std::num::NonZeroU64;
use std::sync::Arc;

use composition_ir::{
    Address, Domain, Id, Node, Part, Parts, Rect, Revision, Rgba, Snapshot, Space,
};

fn dom() -> Domain {
    Domain(NonZeroU64::new(1).unwrap())
}
fn rev(n: u64) -> Revision {
    Revision(NonZeroU64::new(n).unwrap())
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
fn node(text: &str) -> Node {
    Node {
        text: text.into(),
        ..Node::default()
    }
}

/// A reference implementation of the membership law that scans both whole
/// snapshots. The emitter must agree with it exactly.
fn reference(before: &Snapshot, after: &Snapshot) -> Vec<(Address, Parts)> {
    fn sub_eq(b: &Snapshot, a: &Snapshot, x: Address) -> bool {
        match (b.get(x), a.get(x)) {
            (None, None) => true,
            (None, Some(_)) | (Some(_), None) => false,
            (Some(bn), Some(an)) => bn == an && bn.children.iter().all(|c| sub_eq(b, a, *c)),
        }
    }
    let mut all: Vec<Address> = before
        .addresses()
        .chain(after.addresses())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    all.sort();
    let own = [
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
    let mut out = Vec::new();
    for x in all {
        if sub_eq(before, after, x) {
            continue;
        }
        let mut parts = Parts::EMPTY;
        if let Some(an) = after.get(x) {
            match before.get(x) {
                None => own.iter().for_each(|p| parts.insert(*p)),
                Some(bn) => {
                    for p in own {
                        if !bn.part_eq(an, p) {
                            parts.insert(p);
                        }
                    }
                }
            }
            let kids: std::collections::BTreeSet<Address> = before
                .get(x)
                .map(|n| n.children.clone())
                .unwrap_or_default()
                .into_iter()
                .chain(an.children.iter().copied())
                .collect();
            if kids.into_iter().any(|c| !sub_eq(before, after, c)) {
                parts.insert(Part::Descendant);
            }
        }
        out.push((x, parts));
    }
    out
}

#[test]
fn membership_law_is_a_pure_function_of_the_two_snapshots() {
    let mut seed = 0x5eed_u64;
    let mut rnd = move |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    for _ in 0..200 {
        let mut b = Snapshot::empty(dom(), rev(1)).edit();
        for i in 1..=8u64 {
            let mut n = node(&format!("t{i}"));
            if i <= 3 {
                n.children = vec![addr(Space::Node, i + 4)];
            }
            b.put(addr(Space::Node, i), n);
        }
        let base = b.commit().snapshot;

        let mut e = base.edit();
        for _ in 0..4 {
            let i = rnd(8) + 1;
            match rnd(6) {
                0 => {
                    e.update_if_live(addr(Space::Node, i), |n| n.text = format!("T{}", rnd(4)));
                }
                1 => {
                    e.update_if_live(addr(Space::Node, i), |n| {
                        n.paint = Rgba {
                            r: rnd(200) as u8,
                            g: 0,
                            b: 0,
                            a: 0xff,
                        }
                    });
                }
                2 => {
                    e.put(addr(Space::Frame, i), node("frame"));
                }
                3 => {
                    e.remove(addr(Space::Node, rnd(4) + 5));
                }
                4 => {
                    e.update_if_live(addr(Space::Node, i), |n| n.valid = rnd(2) == 0);
                }
                _ => {
                    e.put(addr(Space::Node, 20 + rnd(3)), node("new"));
                }
            }
        }
        // the discriminating case: a parent whose own record is untouched while
        // a descendant changes. Without it the trace never separates the two
        // readings of "changed".
        e.update_if_live(addr(Space::Node, 5), |n| n.text = format!("deep{}", rnd(3)));
        let commit = e.commit();

        let emitted: Vec<(Address, Parts)> = commit
            .delta
            .diffs
            .iter()
            .map(|d| (d.address, d.parts))
            .collect();
        assert_eq!(emitted, reference(&base, &commit.snapshot));
    }
}

#[test]
fn an_ancestor_whose_own_record_is_untouched_carries_descendant_alone() {
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    let (p, c) = (addr(Space::Node, 1), addr(Space::Node, 2));
    b.put(
        p,
        Node {
            children: vec![c],
            ..Node::default()
        },
    );
    b.put(c, node("old"));
    let base = b.commit().snapshot;

    let mut e = base.edit();
    e.update(c, |n| n.text = "new".into());
    let commit = e.commit();

    let parent = commit.delta.diffs.iter().find(|d| d.address == p).unwrap();
    assert_eq!(
        parent.parts.iter().collect::<Vec<_>>(),
        vec![Part::Descendant]
    );
    assert_eq!(
        base.get(p).unwrap().as_ref(),
        commit.snapshot.get(p).unwrap().as_ref()
    );
    // masking Descendant recovers the record-only reading the superseded
    // component deltas used, so both readings come from the one list
    assert!(parent.parts.own().is_empty());
}

#[test]
fn an_unchanged_subtree_is_the_same_instance() {
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    for i in 1..=200u64 {
        b.put(addr(Space::Node, i), node(&format!("t{i}")));
    }
    let base = b.commit().snapshot;

    let mut e = base.edit();
    e.update(addr(Space::Node, 7), |n| n.text = "edited".into());
    let commit = e.commit();

    let untouched = addr(Space::Node, 150);
    assert!(Arc::ptr_eq(
        base.get(untouched).unwrap(),
        commit.snapshot.get(untouched).unwrap()
    ));
    assert!(!Arc::ptr_eq(
        base.get(addr(Space::Node, 7)).unwrap(),
        commit.snapshot.get(addr(Space::Node, 7)).unwrap()
    ));
}

#[test]
fn a_paint_only_change_on_a_wide_container_emits_paint_alone() {
    const W: u64 = 5000;
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    let kids: Vec<Address> = (2..=W + 1).map(|i| addr(Space::Frame, i)).collect();
    for k in &kids {
        b.put(*k, node("leaf"));
    }
    b.put(
        addr(Space::Frame, 1),
        Node {
            children: kids.clone(),
            paint: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0xff,
            },
            ..Node::default()
        },
    );
    let base = b.commit().snapshot;

    let mut e = base.edit();
    e.update(addr(Space::Frame, 1), |n| {
        n.paint = Rgba {
            r: 1,
            g: 1,
            b: 1,
            a: 0xff,
        }
    });
    let commit = e.commit();

    assert_eq!(commit.delta.diffs.len(), 1);
    assert_eq!(
        commit.delta.diffs[0].parts.iter().collect::<Vec<_>>(),
        vec![Part::Paint]
    );
    // a parts-driven consumer never touches the W-wide child list; a bare
    // changed-key would force it to reproject all of them
    assert!(!commit.delta.diffs[0].parts.contains(Part::Structure));
}

#[test]
fn placement_is_a_query_so_a_move_emits_nothing() {
    const N: u64 = 4000;
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    for i in 1..=N {
        let a = addr(Space::Frame, i);
        b.put(a, node("f"));
        b.place(
            a,
            Rect {
                x: (i as i64 - 1) * 10,
                y: 0,
                width: 10,
                height: 4,
            },
        );
    }
    let base = b.commit().snapshot;
    let watched = addr(Space::Frame, 3000);
    let before = base.placement().rect(watched).unwrap();

    let mut e = base.edit();
    e.translate((1000..=N).map(|i| addr(Space::Frame, i)).collect(), 25, 0);
    let commit = e.commit();

    assert!(
        commit.delta.diffs.is_empty(),
        "a move changes no projection"
    );
    let after = commit.snapshot.placement().rect(watched).unwrap();
    assert_eq!(after.x, before.x + 25, "the query folds the translation in");
    assert_eq!(after.y, before.y);
    assert_eq!(after.width, before.width, "a move does not resize");
}

#[test]
fn a_canonical_no_op_reuses_the_exact_snapshot() {
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    b.put(addr(Space::Node, 1), node("x"));
    let base = b.commit().snapshot;

    let commit = base.edit().commit();
    assert!(commit.delta.is_empty());
    assert_eq!(commit.snapshot.version(), base.version());

    // and a write of an equal value is still a no-op
    let mut e = base.edit();
    e.put(addr(Space::Node, 1), node("x"));
    let commit = e.commit();
    assert!(commit.delta.is_empty());
    assert_eq!(commit.snapshot.version(), base.version());
}

#[test]
fn a_retired_address_carries_no_parts_and_a_created_one_carries_all_it_has() {
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    b.put(addr(Space::Node, 1), node("gone"));
    let base = b.commit().snapshot;

    let mut e = base.edit();
    e.remove(addr(Space::Node, 1));
    e.put(addr(Space::Node, 2), node("fresh"));
    let commit = e.commit();

    let retired = commit
        .delta
        .diffs
        .iter()
        .find(|d| d.address == addr(Space::Node, 1))
        .unwrap();
    let created = commit
        .delta
        .diffs
        .iter()
        .find(|d| d.address == addr(Space::Node, 2))
        .unwrap();
    assert!(
        retired.parts.is_empty(),
        "no lifecycle tag is needed: absence is the answer"
    );
    assert!(commit.snapshot.get(addr(Space::Node, 1)).is_none());
    assert!(created.parts.contains(Part::Text));
}

#[test]
fn one_comparison_is_the_whole_precondition() {
    let mut b = Snapshot::empty(dom(), rev(1)).edit();
    b.put(addr(Space::Node, 1), node("a"));
    let s1 = b.commit().snapshot;
    let mut e = s1.edit();
    e.update(addr(Space::Node, 1), |n| n.text = "b".into());
    let c2 = e.commit();
    let mut e = c2.snapshot.edit();
    e.update(addr(Space::Node, 1), |n| n.text = "c".into());
    let c3 = e.commit();

    assert!(c2.delta.applies_to(s1.version()));
    assert!(
        !c3.delta.applies_to(s1.version()),
        "a skipped commit fails the one check"
    );
}

#[test]
#[should_panic(expected = "foreign domain")]
fn a_foreign_domain_address_traps() {
    let base = Snapshot::empty(dom(), rev(1));
    let foreign = Address::new(
        Space::Node,
        Id {
            domain: Domain(NonZeroU64::new(999).unwrap()),
            ordinal: NonZeroU64::new(1).unwrap(),
        },
    );
    let _ = base.get(foreign);
}

/// Many small commits and one large commit reach the same state. Streaming is
/// ordinary editing: no chunk-shaped node, revision domain, or update path.
#[test]
fn chunked_and_whole_application_reach_the_same_state() {
    let words: Vec<String> = (0..64).map(|i| format!("w{i}")).collect();

    let mut chunked = Snapshot::empty(dom(), rev(1));
    for (i, w) in words.iter().enumerate() {
        let mut e = chunked.edit();
        e.put(addr(Space::Node, i as u64 + 1), node(w));
        chunked = e.commit().snapshot;
    }

    let mut e = Snapshot::empty(dom(), rev(1)).edit();
    for (i, w) in words.iter().enumerate() {
        e.put(addr(Space::Node, i as u64 + 1), node(w));
    }
    let whole = e.commit().snapshot;

    let mut a: Vec<_> = chunked.addresses().collect();
    let mut b: Vec<_> = whole.addresses().collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    for x in a {
        assert_eq!(
            chunked.get(x).unwrap().as_ref(),
            whole.get(x).unwrap().as_ref()
        );
    }
}

/// Retaining many revisions costs the changed frontier, not a copy each. A long
/// editing session holds a lot of history; if each revision were a full copy the
/// session would be the memory profile, not the document.
#[test]
fn retaining_many_revisions_shares_everything_untouched() {
    let mut e = Snapshot::empty(dom(), rev(1)).edit();
    for i in 1..=500u64 {
        e.put(addr(Space::Node, i), node(&format!("t{i}")));
    }
    let base = e.commit().snapshot;

    let mut retained = vec![base.clone()];
    for step in 0..50u64 {
        let mut e = retained.last().unwrap().edit();
        e.update(addr(Space::Node, 1), |n| n.text = format!("edit{step}"));
        retained.push(e.commit().snapshot);
    }

    // one untouched record, shared by instance across every retained revision
    let untouched = addr(Space::Node, 400);
    let first = retained[0].get(untouched).unwrap();
    for s in &retained {
        assert!(Arc::ptr_eq(first, s.get(untouched).unwrap()));
    }
    // and the edited one is distinct at every step
    for w in retained.windows(2) {
        assert!(!Arc::ptr_eq(
            w[0].get(addr(Space::Node, 1)).unwrap(),
            w[1].get(addr(Space::Node, 1)).unwrap()
        ));
    }
}

/// Revisions never wrap. Exhaustion is a hard failure, because a wrapped
/// revision would silently make a stale delta look applicable.
#[test]
#[should_panic(expected = "revision exhausted")]
fn revision_exhaustion_fails_rather_than_wrapping() {
    let last = Revision(NonZeroU64::new(u64::MAX).unwrap());
    let mut e = Snapshot::empty(dom(), last).edit();
    e.put(addr(Space::Node, 1), node("x"));
    let _ = e.commit();
}

/// The space set is closed, and closed cheaply: one byte, exhaustively
/// matchable, fixed at the C boundary.
#[test]
fn the_space_set_is_closed() {
    let all = [
        Space::Node,
        Space::Frame,
        Space::Source,
        Space::Origin,
        Space::Destination,
        Space::Resource,
    ];
    assert_eq!(std::mem::size_of::<Space>(), 1);
    let mut seen: Vec<u8> = all.iter().map(|s| *s as u8).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), all.len(), "space discriminants collide");
    // a frontend needing a disjoint identity space uses a Domain, not a variant
    let other = Domain(NonZeroU64::new(2).unwrap());
    let a = Address::new(
        Space::Node,
        Id {
            domain: dom(),
            ordinal: NonZeroU64::new(1).unwrap(),
        },
    );
    let b = Address::new(
        Space::Node,
        Id {
            domain: other,
            ordinal: NonZeroU64::new(1).unwrap(),
        },
    );
    assert_ne!(a, b, "the same ordinal in two domains must not collide");
}

/// The contract says a published snapshot is safe for concurrent reads. That is
/// a property of the type, so it is asserted on the type: the persistent map
/// underneath has an Rc-backed default that would quietly make this false.
#[test]
fn a_published_snapshot_is_readable_from_any_thread() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Snapshot>();
    assert_send_sync::<composition_ir::Delta>();
    assert_send_sync::<Node>();

    let mut e = Snapshot::empty(dom(), rev(1)).edit();
    for i in 1..=64u64 {
        e.put(addr(Space::Node, i), node(&format!("t{i}")));
    }
    let snapshot = std::sync::Arc::new(e.commit().snapshot);
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let s = std::sync::Arc::clone(&snapshot);
            std::thread::spawn(move || s.addresses().count())
        })
        .collect();
    for r in readers {
        assert_eq!(r.join().unwrap(), 64);
    }
}

/// The spec cites a gate for every normative rule. This checks the citation
/// actually resolves, so a rule cannot quietly lose its check.
#[test]
fn every_gate_the_spec_cites_exists() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/composition-ir.md"
    ))
    .expect("the contract must be readable from the suite that enforces it");
    // The contract's rules are checked here and at the C boundary, so every
    // suite counts as resolving a citation. Discovered rather than listed: a
    // hardcoded list silently stops covering a suite the moment one is added,
    // and this gate reported three real citations as missing the first time
    // that happened -- which is the good outcome, but only because the list was
    // short enough to notice. A longer one would have gone the other way.
    let mut source = String::new();
    let mut dirs = vec![std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../.."
    ))];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n == "target" || n == ".git")
                {
                    continue;
                }
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && path.parent().is_some_and(|p| p.ends_with("tests"))
            {
                source.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    assert!(
        source.contains("fn every_gate_the_spec_cites_exists("),
        "the search did not even find this file; it would resolve nothing"
    );

    let mut cited = Vec::new();
    let mut rest = spec.as_str();
    while let Some(open) = rest.find("[`") {
        rest = &rest[open + 2..];
        if let Some(close) = rest.find("`]") {
            let name = &rest[..close];
            if name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
                && name.contains('_')
            {
                cited.push(name.to_string());
            }
            rest = &rest[close + 2..];
        }
    }
    assert!(!cited.is_empty(), "the spec cites no gates at all");

    let missing: Vec<&String> = cited
        .iter()
        .filter(|name| !source.contains(&format!("fn {name}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "spec cites gates that do not exist: {missing:?}"
    );
}
