//! Conformance gates for the C ABI section of `docs/specs/composition-ir.md`.

use std::mem::{align_of, size_of};
use std::num::NonZeroU64;

use composition_ir::{
    Address, Diff, Domain, Id, Node, Parts, Rect, Revision, Rgba, Snapshot, Space,
};
use composition_ir_ffi::record::{
    CirBytes, CirNode, cir_node_children, cir_node_font, cir_node_fragments, cir_node_interaction,
    cir_node_intrinsic, cir_node_lines, cir_node_paint, cir_node_resources, cir_node_source_link,
    cir_node_text, cir_node_text_map, cir_node_valid, cir_snapshot_addresses, cir_snapshot_len,
    cir_snapshot_node, cir_snapshot_rect, cir_snapshot_roots,
};
use composition_ir_ffi::{
    CirDelta, CirSnapshot, cir_delta_entries, cir_delta_len, cir_delta_release,
    cir_snapshot_contains, cir_snapshot_release, cir_snapshot_retain, cir_snapshot_version,
};

fn dom() -> Domain {
    Domain(NonZeroU64::new(1).unwrap())
}
fn addr(n: u64) -> Address {
    addr_in(Space::Node, n)
}
fn addr_in(space: Space, n: u64) -> Address {
    Address::new(
        space,
        Id {
            domain: dom(),
            ordinal: NonZeroU64::new(n).unwrap(),
        },
    )
}

/// The bytes behind a borrowed view. A zero-length `String` borrows a dangling
/// but aligned pointer, which is exactly what an empty slice wants.
unsafe fn bytes<'a>(b: CirBytes) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts(b.ptr, b.len) }
}

/// The value types cross by layout, not by conversion. If any of these grows a
/// niche, a discriminant, or a pointer, the boundary silently starts costing a
/// per-entry copy.
#[test]
fn the_value_types_are_abi_stable_and_pod() {
    assert_eq!(size_of::<Domain>(), 8);
    assert_eq!(size_of::<Revision>(), 8);
    assert_eq!(size_of::<Parts>(), 2);
    assert_eq!(size_of::<Space>(), 1);
    // Address is space + id; Diff is address + parts. Both fixed, both without
    // padding surprises that would need a translation struct.
    assert_eq!(size_of::<Id>(), 16);
    assert_eq!(align_of::<Diff>(), 8);
    assert!(
        size_of::<Diff>() <= 32,
        "Diff is {} bytes",
        size_of::<Diff>()
    );
    assert!(!std::mem::needs_drop::<Diff>(), "a Diff must be plain data");
}

/// A C consumer reads the IR's own allocation.
#[test]
fn crossing_the_boundary_allocates_nothing_per_entry() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    for i in 1..=100u64 {
        e.put(
            addr(i),
            Node {
                text: format!("t{i}"),
                ..Node::default()
            },
        );
    }
    let commit = e.commit();
    let rust_ptr = commit.delta.as_slice().as_ptr();
    let expected_len = commit.delta.diffs.len();

    let delta = CirDelta::into_raw(commit.delta.clone());
    unsafe {
        assert_eq!(cir_delta_len(delta), expected_len);
        let c_ptr = cir_delta_entries(delta);
        // the same bytes, read through both vocabularies
        let via_c = std::slice::from_raw_parts(c_ptr, expected_len);
        assert_eq!(via_c, commit.delta.as_slice());
        assert_ne!(rust_ptr, std::ptr::null());
        cir_delta_release(delta);
    }
}

/// Retention is explicit and reference-counted; releasing one handle does not
/// invalidate another.
#[test]
fn handles_are_retained_and_released_independently() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    e.put(addr(1), Node::default());
    let commit = e.commit();
    let version = commit.snapshot.version();

    let a = CirSnapshot::into_raw(commit.snapshot);
    unsafe {
        let b = cir_snapshot_retain(a);
        cir_snapshot_release(a);
        // b is still usable after a is gone
        assert_eq!(cir_snapshot_version(b), version);
        assert!(cir_snapshot_contains(b, addr(1)));
        assert!(!cir_snapshot_contains(b, addr(2)));
        cir_snapshot_release(b);
    }
}

/// The gate that says the boundary is sufficient rather than merely cheap.
///
/// This walks and renders a whole document using nothing but `cir_*` calls and
/// raw pointers -- no `Snapshot`, no `Node`, no Rust field access anywhere in
/// the walk. Before the record accessors existed this could not be written at
/// all: a C consumer could learn that an address was live and nothing about
/// what was there, so every binding was blocked on it.
#[test]
fn a_consumer_can_render_from_the_c_boundary_alone() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    let (root, a, b) = (addr(1), addr(2), addr(3));
    e.put(
        root,
        Node {
            children: vec![a, b],
            text: "doc".into(),
            ..Node::default()
        },
    );
    e.put(
        a,
        Node {
            text: "alpha".into(),
            intrinsic: 5,
            paint: Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            source_link: "md:7".into(),
            ..Node::default()
        },
    );
    e.put(
        b,
        Node {
            text: "beta".into(),
            intrinsic: 4,
            valid: false,
            ..Node::default()
        },
    );
    e.root(root);
    e.place(
        a,
        Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        },
    );
    let snapshot = CirSnapshot::into_raw(e.commit().snapshot);

    // Everything below is what a Swift, Kotlin, or JavaScript binding does.
    unsafe fn text_of(node: *const CirNode) -> String {
        let t = unsafe { cir_node_text(node) };
        String::from_utf8(unsafe { std::slice::from_raw_parts(t.ptr, t.len) }.to_vec()).unwrap()
    }

    let mut rendered = String::new();
    unsafe {
        let roots = cir_snapshot_roots(snapshot);
        assert_eq!(roots.len, 1, "one declared entry point");

        let mut stack = vec![*roots.ptr];
        while let Some(address) = stack.pop() {
            let node = cir_snapshot_node(snapshot, address);
            assert!(!node.is_null(), "walked to an address with nothing live");

            rendered.push_str(&text_of(node));
            rendered.push(':');
            rendered.push_str(&cir_node_intrinsic(node).to_string());
            if !cir_node_valid(node) {
                rendered.push_str("!invalid");
            }
            let mut rect = Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            if cir_snapshot_rect(snapshot, address, &mut rect) {
                rendered.push_str(&format!("@{},{}", rect.x, rect.y));
            }
            rendered.push(' ');

            let kids = cir_node_children(node);
            for i in (0..kids.len).rev() {
                stack.push(*kids.ptr.add(i));
            }
        }

        // the record-level detail a backend and an SBS editor each need
        let node_a = cir_snapshot_node(snapshot, a);
        assert_eq!(
            cir_node_paint(node_a),
            Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }
        );
        let link = cir_node_source_link(node_a);
        assert_eq!(
            std::str::from_utf8(std::slice::from_raw_parts(link.ptr, link.len)).unwrap(),
            "md:7"
        );

        cir_snapshot_release(snapshot);
    }

    assert_eq!(rendered, "doc:0 alpha:5@10,20 beta:4!invalid ");
}

/// Walking from roots is not the same claim as seeing the document.
///
/// The fixture here is the one every backend conformance gate already renders:
/// records put and placed, `root` never called. All three reference renderers
/// enumerate `Snapshot::addresses`, so a C consumer restricted to
/// `cir_snapshot_roots` would have rendered an empty page where they render the
/// whole document -- and would never reach a `Resource` at all, since a
/// resource is named by role rather than held in `children`.
#[test]
fn a_consumer_enumerates_the_live_set_not_only_what_was_rooted() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    let mut expected = Vec::new();
    for i in 1..=40u64 {
        // unrooted Node records, exactly as `document(n)` builds them
        let a = addr(i);
        e.put(
            a,
            Node {
                text: format!("word {i}"),
                fragments: vec![(i as u32 - 1) / 20],
                ..Node::default()
            },
        );
        expected.push(a);
    }
    // and a resource, which no children walk can reach by construction
    let font = addr_in(Space::Resource, 1);
    e.put(font, Node::default());
    expected.push(font);
    let snapshot = CirSnapshot::into_raw(e.commit().snapshot);

    unsafe {
        assert_eq!(
            cir_snapshot_roots(snapshot).len,
            0,
            "the fixture must be unrooted, or this gate proves nothing"
        );

        // the two-call sizing pattern: ask, allocate, fill
        let total = cir_snapshot_addresses(snapshot, std::ptr::null_mut(), 0);
        assert_eq!(total, cir_snapshot_len(snapshot));
        assert_eq!(total, expected.len());

        let mut buf = vec![addr(1); total];
        let written = cir_snapshot_addresses(snapshot, buf.as_mut_ptr(), buf.len());
        assert_eq!(written, total);

        // order is unspecified, so the claim is on the set
        let mut got = buf.clone();
        got.sort();
        expected.sort();
        assert_eq!(got, expected, "enumeration did not report the live set");

        // and every address it reported resolves to a readable record
        for a in &buf {
            let node = cir_snapshot_node(snapshot, *a);
            assert!(!node.is_null(), "enumerated an address with nothing live");
        }

        // a short buffer truncates rather than overruns, and still reports the
        // true total so a caller can grow and retry
        let mut short = vec![addr(1); 3];
        assert_eq!(
            cir_snapshot_addresses(snapshot, short.as_mut_ptr(), 3),
            total
        );
        assert!(short.iter().all(|a| expected.binary_search(a).is_ok()));

        cir_snapshot_release(snapshot);
    }
}

/// Sufficiency of the fields, which the hand-written walk cannot carry.
///
/// Five accessors return `CirBytes` and four return a scalar, so crossing any
/// two of a kind compiles and renders plausibly; a gate that reads a subset of
/// them on one fixed tree stays green through it. `AGENTS.md` asks for
/// randomized equivalence against a from-scratch computation wherever one is
/// possible, and here it is: the Rust projection is the second computation, and
/// the claim is that no exported field can disagree with it.
///
/// Every generated value is distinct per field *and* per record -- disjoint
/// integer ranges, differently prefixed strings -- because equal values are how
/// a swap gets to pass.
#[test]
fn the_c_projection_of_every_field_equals_the_rust_one() {
    let mut seed = 0x5eed_u64;
    let mut rnd = move |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };

    for round in 0..200u64 {
        let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
        let count = rnd(12) + 1;
        let mut live: Vec<Address> = Vec::new();

        for i in 1..=count {
            let space = match rnd(4) {
                0 => Space::Resource,
                1 => Space::Frame,
                2 => Space::Source,
                _ => Space::Node,
            };
            let a = addr_in(space, i);
            if live.contains(&a) {
                continue;
            }
            let tag = round * 1000 + i;

            // children only from records already put, so no walk can cycle
            let children: Vec<Address> = (0..rnd(4))
                .filter_map(|_| live.get(rnd(live.len().max(1) as u64) as usize).copied())
                .collect();
            let resources: Vec<composition_ir::ResourceRef> = (0..rnd(4))
                .map(|k| composition_ir::ResourceRef {
                    role: match rnd(4) {
                        0 => composition_ir::ResourceRole::Font,
                        1 => composition_ir::ResourceRole::Image,
                        2 => composition_ir::ResourceRole::Gradient,
                        _ => composition_ir::ResourceRole::ColorProfile,
                    },
                    address: addr_in(Space::Resource, tag * 8 + k + 1),
                })
                .collect();

            e.put(
                a,
                Node {
                    children,
                    resources,
                    fragments: (0..rnd(4)).map(|k| (tag * 4 + k) as u32).collect(),
                    // one prefix per field: a swapped pair cannot match
                    text: format!("text-{tag}-{}", "x".repeat(rnd(9) as usize)),
                    text_map: format!("map-{tag}-{}", "y".repeat(rnd(9) as usize)),
                    font: format!("font-{tag}-{}", "z".repeat(rnd(9) as usize)),
                    interaction: format!("act-{tag}"),
                    source_link: format!("src-{tag}"),
                    // disjoint ranges: intrinsic can never be a plausible lines
                    intrinsic: 100_000 + rnd(1000) as i64,
                    lines: rnd(100) as i64,
                    paint: Rgba {
                        r: rnd(256) as u8,
                        g: rnd(256) as u8,
                        b: rnd(256) as u8,
                        a: rnd(256) as u8,
                    },
                    valid: rnd(2) == 0,
                },
            );
            live.push(a);
            if rnd(2) == 0 {
                e.root(a);
            }
            if rnd(3) > 0 {
                e.place(
                    a,
                    Rect {
                        x: rnd(500) as i64,
                        y: 1000 + rnd(500) as i64,
                        width: 2000 + rnd(500) as i64,
                        height: 3000 + rnd(500) as i64,
                    },
                );
            }
        }
        if rnd(2) == 0 && !live.is_empty() {
            e.translate(live.clone(), rnd(50) as i64, rnd(50) as i64);
        }

        let rust = e.commit().snapshot;
        let handle = CirSnapshot::into_raw(rust.clone());

        unsafe {
            // the C side discovers the live set for itself rather than being
            // handed the Rust one, so a wrong enumeration cannot hide here
            let total = cir_snapshot_len(handle);
            let mut buf = vec![addr(1); total];
            assert_eq!(
                cir_snapshot_addresses(handle, buf.as_mut_ptr(), total),
                total
            );
            let mut seen = buf.clone();
            seen.sort();
            let mut want: Vec<Address> = rust.addresses().collect();
            want.sort();
            assert_eq!(seen, want);

            for a in &buf {
                let n = rust.get(*a).expect("enumerated address must be live");
                let node = cir_snapshot_node(handle, *a);
                assert!(!node.is_null());

                assert_eq!(bytes(cir_node_text(node)), n.text.as_bytes(), "text @{a:?}");
                assert_eq!(
                    bytes(cir_node_text_map(node)),
                    n.text_map.as_bytes(),
                    "text_map @{a:?}"
                );
                assert_eq!(bytes(cir_node_font(node)), n.font.as_bytes(), "font @{a:?}");
                assert_eq!(
                    bytes(cir_node_interaction(node)),
                    n.interaction.as_bytes(),
                    "interaction @{a:?}"
                );
                assert_eq!(
                    bytes(cir_node_source_link(node)),
                    n.source_link.as_bytes(),
                    "source_link @{a:?}"
                );

                let kids = cir_node_children(node);
                assert_eq!(
                    std::slice::from_raw_parts(kids.ptr, kids.len),
                    n.children.as_slice(),
                    "children @{a:?}"
                );
                let res = cir_node_resources(node);
                assert_eq!(
                    std::slice::from_raw_parts(res.ptr, res.len),
                    n.resources.as_slice(),
                    "resources @{a:?}"
                );
                let frags = cir_node_fragments(node);
                assert_eq!(
                    std::slice::from_raw_parts(frags.ptr, frags.len),
                    n.fragments.as_slice(),
                    "fragments @{a:?}"
                );

                assert_eq!(cir_node_intrinsic(node), n.intrinsic, "intrinsic @{a:?}");
                assert_eq!(cir_node_lines(node), n.lines, "lines @{a:?}");
                assert_eq!(cir_node_paint(node), n.paint, "paint @{a:?}");
                assert_eq!(cir_node_valid(node), n.valid, "valid @{a:?}");

                let mut rect = Rect {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                };
                let got = cir_snapshot_rect(handle, *a, &mut rect);
                match rust.placement().rect(*a) {
                    Some(r) => {
                        assert!(got, "placement @{a:?}");
                        assert_eq!(rect, r, "placement @{a:?}");
                    }
                    None => assert!(!got, "placement invented for @{a:?}"),
                }
            }

            let roots = cir_snapshot_roots(handle);
            assert_eq!(
                std::slice::from_raw_parts(roots.ptr, roots.len),
                rust.roots()
            );

            cir_snapshot_release(handle);
        }
    }
}

/// Borrowed, not copied: the pointers a consumer reads are the IR's own.
#[test]
fn reading_a_record_borrows_the_irs_own_bytes() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    let kid = addr(2);
    e.put(
        addr(1),
        Node {
            text: "the quick brown fox".repeat(64),
            children: vec![kid],
            ..Node::default()
        },
    );
    e.put(kid, Node::default());
    let snapshot = e.commit().snapshot;

    let rust_text = snapshot.get(addr(1)).unwrap().text.as_ptr();
    let rust_children = snapshot.get(addr(1)).unwrap().children.as_ptr();

    let handle = CirSnapshot::into_raw(snapshot);
    unsafe {
        let node = cir_snapshot_node(handle, addr(1));
        assert_eq!(
            cir_node_text(node).ptr,
            rust_text,
            "text was copied out rather than borrowed"
        );
        assert_eq!(
            cir_node_children(node).ptr,
            rust_children,
            "children were copied out rather than borrowed"
        );
        // and reading twice hands back the same allocation, so a consumer that
        // reads in a loop is not paying per read
        assert_eq!(cir_node_text(node).ptr, cir_node_text(node).ptr);
        cir_snapshot_release(handle);
    }
}

/// Absence is answered once, at the lookup, rather than by every accessor
/// inventing a sentinel a caller would have to tell apart from empty text.
#[test]
fn an_absent_address_is_null_rather_than_an_empty_record() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    e.put(addr(1), Node::default());
    let handle = CirSnapshot::into_raw(e.commit().snapshot);
    unsafe {
        assert!(!cir_snapshot_node(handle, addr(1)).is_null());
        assert!(cir_snapshot_node(handle, addr(9)).is_null());
        // a live record with empty text is not the same answer as an absent one
        let live = cir_snapshot_node(handle, addr(1));
        assert_eq!(cir_node_text(live).len, 0);
        let mut rect = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert!(!cir_snapshot_rect(handle, addr(9), &mut rect));
        cir_snapshot_release(handle);
    }
}

/// A move publishes nothing and is answered by the query, across the boundary
/// as much as inside it.
#[test]
fn a_move_is_answered_by_the_placement_query_not_by_the_delta() {
    let mut e = Snapshot::empty(dom(), Revision(NonZeroU64::new(1).unwrap())).edit();
    e.put(addr(1), Node::default());
    e.place(
        addr(1),
        Rect {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        },
    );
    let base = e.commit().snapshot;

    let mut e = base.edit();
    e.translate(vec![addr(1)], 100, 200);
    let commit = e.commit();
    assert!(commit.delta.is_empty(), "a move published a diff entry");

    let handle = CirSnapshot::into_raw(commit.snapshot);
    unsafe {
        let mut rect = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert!(cir_snapshot_rect(handle, addr(1), &mut rect));
        assert_eq!((rect.x, rect.y), (101, 202));
        cir_snapshot_release(handle);
    }
}
