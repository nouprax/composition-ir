//! A stand-in for markdown-core: its own AST, its own identities, its own
//! delta. It has never heard of Composition IR, which is the point.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Heading,
    Para,
    Embed,
    /// A span written in another grammar -- a formula, a fenced block destined
    /// for another parser. Classified here and parsed nowhere: this frontend
    /// does not know the grammar and must not learn it.
    Formula,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdNode {
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    /// byte span in this frontend's own source, its own coordinate space
    pub span: (usize, usize),
}

#[derive(Debug, Clone, Default)]
pub struct MdAst {
    pub nodes: Vec<MdNode>,
    pub source: String,
}

/// What one reparse changed, in the frontend's own identity space.
///
/// This is the only thing an adapter needs beyond the AST itself, and without
/// it adaptation is linear in the whole document on every commit.
#[derive(Debug, Clone, Default)]
pub struct MdDelta {
    pub touched: Vec<u64>,
    pub retired: Vec<u64>,
}

#[derive(Debug, Default)]
pub struct MdSession {
    next_id: u64,
    ast: MdAst,
}

impl MdSession {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            ast: MdAst::default(),
        }
    }
    pub fn ast(&self) -> &MdAst {
        &self.ast
    }

    /// Reparse, preserving an id when a line keeps its kind and position.
    pub fn set_source(&mut self, source: &str) -> (MdAst, MdDelta) {
        let old = std::mem::take(&mut self.ast.nodes);
        let mut nodes = Vec::new();
        let mut at = 0usize;
        for (i, line) in source.lines().enumerate() {
            let start = at;
            at += line.len() + 1;
            if line.trim().is_empty() {
                continue;
            }
            let kind = if line.starts_with("![[") {
                Kind::Embed
            } else if line.starts_with("$$") {
                Kind::Formula
            } else if line.starts_with('#') {
                Kind::Heading
            } else {
                Kind::Para
            };
            let id = match old.get(i) {
                Some(prev) if prev.kind == kind => prev.id,
                _ => {
                    self.next_id += 1;
                    self.next_id
                }
            };
            nodes.push(MdNode {
                id,
                kind,
                text: line.to_string(),
                span: (start, start + line.len()),
            });
        }
        let mut delta = MdDelta::default();
        for n in &nodes {
            match old.iter().find(|o| o.id == n.id) {
                Some(prev) if prev.text == n.text && prev.kind == n.kind => {}
                _ => delta.touched.push(n.id),
            }
        }
        for o in &old {
            if !nodes.iter().any(|n| n.id == o.id) {
                delta.retired.push(o.id);
            }
        }
        self.ast = MdAst {
            nodes,
            source: source.to_string(),
        };
        (self.ast.clone(), delta)
    }

    /// The reference this embed names, unresolved. The frontend classifies it
    /// and stops: it never opens the target.
    pub fn embed_reference(node: &MdNode) -> Option<&str> {
        node.text.strip_prefix("![[")?.strip_suffix("]]")
    }

    /// The bytes of a foreign-grammar span, unparsed. Same discipline as
    /// `embed_reference` and for the same reason -- the difference is only that
    /// the unit is named by inclusion rather than by path.
    pub fn formula_source(node: &MdNode) -> Option<&str> {
        node.text.strip_prefix("$$")?.strip_suffix("$$")
    }
}
