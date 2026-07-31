//! Conformance gates for .
//!
//! These assemble frontend -> adapter -> IR for real and check the boundary
//! claims that the contract would otherwise only assert.

use std::num::NonZeroU64;

use composition_ir::{Domain, Part, Revision, Snapshot, Space};
use frontend_conformance::adapter;
use frontend_conformance::fixture::MdSession;

fn dom(n: u64) -> Domain {
    Domain(NonZeroU64::new(n).unwrap())
}
fn rev1() -> Revision {
    Revision(NonZeroU64::new(1).unwrap())
}
const ADAPTER: &str = include_str!("../src/adapter.rs");
const IR_MANIFEST: &str = include_str!("../../composition-ir/Cargo.toml");

/// The frontend contract is whatever the adapter has to do. If that is large,
/// the IR is specifying the frontend rather than accepting one.
#[test]
fn the_adapter_is_the_whole_frontend_contract() {
    let loc = |src: &str| {
        src.lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("//")
            })
            .count()
    };
    // `project` is the contract's true size: it is the only place that says
    // what the IR needs a frontend to supply. Everything else in the adapter is
    // bookkeeping that any frontend would write the same way.
    let projection = ADAPTER
        .split("fn project(")
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .expect("the adapter has a projection function");
    // 40, having been 30 before structured paint and typed resource references
    // landed. This number moves only with a stated reason in the same change,
    // because it is the only thing keeping the contract from growing quietly.
    assert!(
        loc(projection) < 40,
        "the frontend contract is {} lines of projection",
        loc(projection)
    );
    // 130, having been 100. Both routes plus the projection now carry
    // structured paint and a placement rectangle; the projection bound above is
    // the one that matters, and this is the outer envelope.
    assert!(
        loc(ADAPTER) < 130,
        "the whole adapter is {} lines",
        loc(ADAPTER)
    );
    // and it is the only place the two vocabularies meet
    assert!(ADAPTER.contains("crate::fixture") && ADAPTER.contains("composition_ir"));
}

/// The dependency direction is the proof that the IR does not know its
/// frontends. A cycle here would make the boundary claim unfalsifiable.
#[test]
fn the_ir_has_never_heard_of_a_frontend() {
    let deps = IR_MANIFEST
        .split("[dependencies]")
        .nth(1)
        .expect("declares dependencies");
    for forbidden in ["frontend", "adapter", "markdown", "tex"] {
        assert!(
            !deps.contains(forbidden),
            "composition-ir depends on {forbidden}"
        );
    }
    let frontend = include_str!("../src/fixture.rs");
    assert!(
        !frontend.contains("composition_ir"),
        "the frontend reaches into the IR; the adapter is the only seam"
    );
}

/// Without a frontend delta, adaptation is linear in the document on every
/// commit. This is the cost that makes the delta a requirement rather than a
/// convenience — and both routes must reach the same state.
#[test]
fn a_frontend_delta_is_what_makes_adaptation_proportional() {
    const N: usize = 2000;
    let mut md = MdSession::new();
    let doc: String = (0..N).map(|i| format!("line {i}\n")).collect();
    let (ast, delta) = md.set_source(&doc);
    let base = adapter::adapt(&Snapshot::empty(dom(1), rev1()), &ast, &delta)
        .commit
        .snapshot;

    let mut edited: String = doc.clone();
    edited = edited.replace("line 900\n", "line 900 edited\n");
    let (ast, delta) = md.set_source(&edited);

    let incremental = adapter::adapt(&base, &ast, &delta);
    let full = adapter::adapt_full(&base, &ast);

    assert!(
        incremental.records_touched <= 4,
        "one edited line made the adapter touch {} records",
        incremental.records_touched
    );
    assert!(
        full.records_touched >= N,
        "the delta-free route is linear, as expected"
    );

    // and the two routes agree, which is what lets the delta stay optional
    let a: Vec<_> = incremental.commit.snapshot.addresses().collect();
    let b: Vec<_> = full.commit.snapshot.addresses().collect();
    assert_eq!(a.len(), b.len());
    for addr in a {
        assert_eq!(
            incremental.commit.snapshot.get(addr).unwrap().as_ref(),
            full.commit.snapshot.get(addr).unwrap().as_ref(),
            "incremental and full adaptation disagree at {addr:?}"
        );
    }
    assert_eq!(
        incremental.commit.delta.diffs.len(),
        full.commit.delta.diffs.len()
    );
}

/// One edit, end to end, with the IR naming only its own addresses and parts.
#[test]
fn an_edit_crosses_the_boundary_as_parts_not_as_frontend_concepts() {
    let mut md = MdSession::new();
    let (a0, d0) = md.set_source("# Title\nbody one\nbody two\n");
    let c1 = adapter::adapt(&Snapshot::empty(dom(1), rev1()), &a0, &d0).commit;
    let (a1, d1) = md.set_source("# Title\n# body one\nbody two\n");
    let c2 = adapter::adapt(&c1.snapshot, &a1, &d1).commit;

    assert!(!c2.delta.is_empty());
    for d in &c2.delta.diffs {
        assert_eq!(d.address.space, Space::Node);
        assert_eq!(d.address.domain(), dom(1));
    }
    let third = adapter::address(dom(1), 3);
    assert!(
        !c2.delta.diffs.iter().any(|d| d.address == third),
        "an unrelated line was reported as changed"
    );
}

/// Side-by-side: from one IR entry, reach the source span in the frontend's own
/// coordinate space and the preview placement, without the IR owning bytes.
#[test]
fn side_by_side_joins_source_and_preview_across_the_boundary() {
    let mut md = MdSession::new();
    let (a1, d1) = md.set_source("# Title\nalpha\nbravo\n");
    let c1 = adapter::adapt(&Snapshot::empty(dom(1), rev1()), &a1, &d1).commit;
    let (a2, d2) = md.set_source("# Title\nalpha edited\nbravo\n");
    let c2 = adapter::adapt(&c1.snapshot, &a2, &d2).commit;

    let changed: Vec<_> = c2
        .delta
        .diffs
        .iter()
        .filter(|d| d.parts.contains(Part::Text))
        .collect();
    assert_eq!(changed.len(), 1, "one line changed, one entry carries Text");

    // the IR carries a join key, not a coordinate; the frontend owns source
    let node = c2.snapshot.get(changed[0].address).unwrap();
    let md_id: u64 = node
        .source_link
        .strip_prefix("md:")
        .unwrap()
        .parse()
        .unwrap();
    let (s, e) = md
        .ast()
        .nodes
        .iter()
        .find(|n| n.id == md_id)
        .map(|n| n.span)
        .expect("the frontend resolves its own coordinates");
    assert_eq!(&md.ast().source[s..e], "alpha edited");
    assert!(c2.snapshot.placement().rect(changed[0].address).is_some());
}

/// Two units, two domains. Composing them is the consumer's layer.
#[test]
fn a_target_commit_never_touches_its_host() {
    let mut host_md = MdSession::new();
    let mut target_md = MdSession::new();
    let (ha, hd) = host_md.set_source("# Host\n![[target]]\ntail\n");
    let host = adapter::adapt(&Snapshot::empty(dom(1), rev1()), &ha, &hd).commit;
    let (ta, td) = target_md.set_source("# Target\nbody\n");
    let mut t = adapter::adapt(&Snapshot::empty(dom(2), rev1()), &ta, &td).commit;

    let embed = host
        .snapshot
        .addresses()
        .find(|a| host.snapshot.get(*a).unwrap().interaction == "target")
        .expect("the embed carries its authored reference");
    assert!(host.snapshot.get(embed).unwrap().children.is_empty());
    assert!(!host.snapshot.get(embed).unwrap().text.contains("Target"));

    let host_version = host.snapshot.version();
    for i in 0..5 {
        let (a, d) = target_md.set_source(&format!("# Target\nbody {i}\n"));
        t = adapter::adapt(&t.snapshot, &a, &d).commit;
    }
    assert_eq!(
        host.snapshot.version(),
        host_version,
        "a target commit moved the host"
    );
    assert_ne!(host.snapshot.version().domain, t.snapshot.version().domain);
}

/// Path A: a consumer that never reads a delta reaches the same state.
#[test]
fn a_consumer_that_ignores_every_delta_agrees_with_one_that_does_not() {
    let mut md = MdSession::new();
    let mut snap = Snapshot::empty(dom(1), rev1());
    let mut applied: std::collections::BTreeMap<String, String> = Default::default();

    for i in 0..12 {
        let (ast, d) = md.set_source(&format!("# Title\nline {i}\nconstant\n"));
        let commit = adapter::adapt(&snap, &ast, &d).commit;
        for diff in &commit.delta.diffs {
            match commit.snapshot.get(diff.address) {
                None => {
                    applied.remove(&format!("{:?}", diff.address));
                }
                Some(n) => {
                    applied.insert(format!("{:?}", diff.address), n.text.clone());
                }
            }
        }
        snap = commit.snapshot;
    }
    let derived: std::collections::BTreeMap<String, String> = snap
        .addresses()
        .map(|a| (format!("{a:?}"), snap.get(a).unwrap().text.clone()))
        .collect();
    assert_eq!(applied, derived);
}

/// The frontend contract cites a gate for every normative rule; this checks the
/// citations resolve.
#[test]
fn every_gate_the_spec_cites_exists() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/frontend-contract.md"
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
    assert!(cited >= 6, "the contract cites only {cited} gates");
}
