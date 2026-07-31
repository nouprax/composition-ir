//! Everything that stands between a frontend AST and Composition IR.
//!
//! Gate `the_adapter_is_the_whole_frontend_contract` keeps this small: if
//! adapting a frontend needs a large surface, the IR is specifying the frontend
//! rather than accepting one.

use std::num::NonZeroU64;

use crate::fixture::{Kind, MdAst, MdDelta, MdNode, MdSession};
use composition_ir::{Address, Commit, Domain, Id, Node, Rect, Rgba, Snapshot, Space};

pub fn address(domain: Domain, md_id: u64) -> Address {
    Address::new(
        Space::Node,
        Id {
            domain,
            ordinal: NonZeroU64::new(md_id).expect("frontend ids are positive"),
        },
    )
}

/// How much of the document the adapter had to touch. The whole point of
/// requiring a frontend delta is that this stays proportional to the edit.
#[derive(Debug)]
pub struct Adaptation {
    pub commit: Commit,
    pub records_touched: usize,
}

fn project(n: &MdNode) -> Node {
    Node {
        text: n.text.clone(),
        // A stable join key, never a coordinate. Carrying the frontend's
        // absolute span here would make every later record change on any
        // earlier edit: the IR would be republishing a derived position as if
        // it were a value. The frontend owns source and answers span queries.
        source_link: format!("md:{}", n.id),
        paint: match n.kind {
            Kind::Heading => Rgba {
                r: 0x11,
                g: 0x11,
                b: 0x11,
                a: 0xff,
            },
            Kind::Para => Rgba {
                r: 0x33,
                g: 0x33,
                b: 0x33,
                a: 0xff,
            },
            Kind::Embed => Rgba {
                r: 0x00,
                g: 0x66,
                b: 0xcc,
                a: 0xff,
            },
        },
        // an embed is a leaf naming a reference; nothing target-derived
        interaction: MdSession::embed_reference(n)
            .unwrap_or_default()
            .to_string(),
        intrinsic: n.text.len() as i64,
        ..Node::default()
    }
}

/// Adapt without a frontend delta: every record is rewritten, so cost is linear
/// in the document on every commit. Correct, and the fallback when a frontend
/// cannot report what it changed.
pub fn adapt_full(base: &Snapshot, ast: &MdAst) -> Adaptation {
    let domain = base.version().domain;
    let live: std::collections::HashSet<Address> =
        ast.nodes.iter().map(|n| address(domain, n.id)).collect();
    let mut edit = base.edit();
    let mut touched = 0;
    for stale in base.addresses().collect::<Vec<_>>() {
        if !live.contains(&stale) {
            edit.remove(stale);
            touched += 1;
        }
    }
    for (i, n) in ast.nodes.iter().enumerate() {
        let a = address(domain, n.id);
        edit.put(a, project(n));
        edit.place(
            a,
            Rect {
                x: 0,
                y: 0,
                width: n.text.len() as i64,
                height: 16,
            },
        );
        touched += 1;
        if i == 0 {
            edit.root(a);
        }
    }
    Adaptation {
        commit: edit.commit(),
        records_touched: touched,
    }
}

/// Adapt using what the frontend reported. Cost is proportional to the edit.
pub fn adapt(base: &Snapshot, ast: &MdAst, delta: &MdDelta) -> Adaptation {
    let domain = base.version().domain;
    let mut edit = base.edit();
    let mut touched = 0;
    for id in &delta.retired {
        edit.remove(address(domain, *id));
        touched += 1;
    }
    for id in &delta.touched {
        if let Some(n) = ast.nodes.iter().find(|n| n.id == *id) {
            edit.put(address(domain, *id), project(n));
            touched += 1;
        }
    }
    // placement is rebuilt from the current order; it publishes no diff entry
    edit.reset_placement();
    for n in &ast.nodes {
        edit.place(
            address(domain, n.id),
            Rect {
                x: 0,
                y: 0,
                width: n.text.len() as i64,
                height: 16,
            },
        );
    }
    if let Some(first) = ast.nodes.first() {
        edit.root(address(domain, first.id));
    }
    Adaptation {
        commit: edit.commit(),
        records_touched: touched,
    }
}
