//! A document that stresses what the proposed queries touch.
//!
//! The frontend conformance adapter places every record at the origin, which is
//! all its own gates need and is useless here: a hit test over a stack of
//! zero-offset boxes has nothing to resolve. So this module keeps the frontend
//! fixture and its identities and does the layout itself, giving the document
//! the four properties the workloads actually exercise:
//!
//! * **more than one fragment**, because a paginated target has no single
//!   coordinate space and one of the records deliberately spans a page break;
//! * **overlap**, because a point can land on two records and something has to
//!   decide which one a click means;
//! * **a viewport smaller than the document**, because culling is not
//!   observable otherwise; and
//! * **text that is not ASCII**, because a UTF-16 host counts differently from
//!   the frontend that owns the bytes.

use std::num::NonZeroU64;

use composition_ir::{Address, Domain, Id, Node, Rect, Revision, Rgba, Snapshot, Space};
use frontend_conformance::fixture::{Kind, MdAst, MdNode};

/// Where the page break falls, in the document's own vertical units.
pub const PAGE_HEIGHT: i64 = 100;

/// A viewport the size of one screen, which is smaller than the document.
pub const VIEWPORT: Rect = Rect {
    x: 0,
    y: 0,
    width: 200,
    height: 60,
};

pub fn address(domain: Domain, md_id: u64) -> Address {
    Address::new(
        Space::Node,
        Id {
            domain,
            ordinal: NonZeroU64::new(md_id).expect("frontend ids are positive"),
        },
    )
}

/// One laid-out record, as a frontend that ran layout would report it.
#[derive(Debug)]
pub struct Placed {
    pub id: u64,
    pub rect: Rect,
    /// The fragments it occupies. Two entries means it spans a page break.
    pub fragments: Vec<u32>,
}

/// The fixture AST, with a byte span per node in the frontend's own source.
///
/// Built by hand rather than through `MdSession` so the source can carry
/// multi-byte text at a known offset; the identities and the shape are the
/// fixture's own.
pub fn ast() -> MdAst {
    let bodies = [
        (Kind::Heading, "Title"),
        (Kind::Para, "first paragraph"),
        // Non-ASCII, and deliberately not at the start: a UTF-16 host's offsets
        // agree with the frontend's up to here and diverge after.
        (Kind::Para, "第二段落 with tail"),
        (Kind::Formula, "$x^2$"),
        (Kind::Para, "spans the page break"),
        (Kind::Para, "last paragraph"),
    ];
    let mut nodes = Vec::new();
    let mut source = String::new();
    for (i, (kind, text)) in bodies.iter().enumerate() {
        let start = source.len();
        source.push_str(text);
        source.push('\n');
        nodes.push(MdNode {
            id: i as u64 + 1,
            kind: kind.clone(),
            text: (*text).to_string(),
            span: (start, start + text.len()),
        });
    }
    MdAst { nodes, source }
}

/// The layout a frontend would have produced. Record 5 straddles the break, and
/// records 2 and 3 overlap.
pub fn layout() -> Vec<Placed> {
    let mut placed = Vec::new();
    let boxes: [(u64, i64, i64, i64, i64); 6] = [
        (1, 0, 0, 120, 20),
        (2, 0, 24, 160, 20),
        // Overlaps record 2 by half its height: a point in the overlap is a
        // question the IR currently has no rule for.
        (3, 40, 34, 160, 20),
        (4, 0, 58, 60, 20),
        // Straddles PAGE_HEIGHT, so it is on both pages.
        (5, 0, 90, 180, 24),
        (6, 0, 130, 140, 20),
    ];
    for (id, x, y, width, height) in boxes {
        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        let first = (rect.y / PAGE_HEIGHT) as u32;
        let last = ((rect.y + rect.height - 1) / PAGE_HEIGHT) as u32;
        placed.push(Placed {
            id,
            rect,
            fragments: (first..=last).collect(),
        });
    }
    placed
}

fn project(n: &MdNode, fragments: Vec<u32>) -> Node {
    Node {
        text: n.text.clone(),
        source_link: format!("md:{}", n.id),
        fragments,
        paint: match n.kind {
            Kind::Heading => Rgba {
                r: 0x11,
                g: 0x11,
                b: 0x11,
                a: 0xff,
            },
            Kind::Formula => Rgba {
                r: 0x66,
                g: 0x00,
                b: 0xcc,
                a: 0xff,
            },
            _ => Rgba {
                r: 0x33,
                g: 0x33,
                b: 0x33,
                a: 0xff,
            },
        },
        intrinsic: n.text.len() as i64,
        ..Node::default()
    }
}

/// The whole chain's front half: a frontend AST and its layout, adapted into a
/// published snapshot.
pub fn snapshot(domain: Domain) -> Snapshot {
    let ast = ast();
    let layout = layout();
    let mut edit = Snapshot::empty(domain, Revision(NonZeroU64::new(1).unwrap())).edit();
    for (i, placed) in layout.iter().enumerate() {
        let node = ast
            .nodes
            .iter()
            .find(|n| n.id == placed.id)
            .expect("every laid-out record is in the AST");
        let a = address(domain, placed.id);
        edit.put(a, project(node, placed.fragments.clone()));
        edit.place(a, placed.rect);
        if i == 0 {
            edit.root(a);
        }
    }
    edit.commit().snapshot
}
