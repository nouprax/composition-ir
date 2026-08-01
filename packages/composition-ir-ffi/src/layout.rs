//! The ABI layout manifest.
//!
//! Every non-Rust binding reads the value types out of foreign memory by
//! offset: a Swift `UnsafeBufferPointer`, a Kotlin `ByteBuffer` from
//! `NewDirectByteBuffer`, a JavaScript `DataView` over `WebAssembly.Memory`.
//! Each of those needs to know where a field starts, and none of them can ask
//! the compiler.
//!
//! Left to itself that means every binding hand-computes its own offset table,
//! and the first time a field is reordered they all keep reading -- silently,
//! at the wrong addresses. A size assertion does not catch it, because
//! reordering two fields of a `#[repr(C)]` struct changes no size.
//!
//! So the offsets are generated here from the compiled types and written to
//! `abi/layout.json`, which is the artifact bindings are generated from. A
//! reorder changes that file, and a gate fails until it is regenerated and the
//! bindings with it.

use std::mem::{align_of, offset_of, size_of};

use composition_ir::{
    Address, Diff, Domain, Id, Part, Parts, Rect, ResourceRole, Revision, Rgba, SnapshotVersion,
    Space,
};

/// Bumped when the meaning of the manifest itself changes -- a new field on an
/// entry, a new `kind`. Not a version of the ABI it describes: that is the
/// content, and its changes show up as a diff on `abi/layout.json`.
pub const MANIFEST_FORMAT: u32 = 1;

/// Every `Space`, in discriminant order.
pub const ALL_SPACES: [Space; 6] = [
    Space::Node,
    Space::Frame,
    Space::Source,
    Space::Origin,
    Space::Destination,
    Space::Resource,
];

/// Every `ResourceRole`, in discriminant order.
pub const ALL_RESOURCE_ROLES: [ResourceRole; 4] = [
    ResourceRole::Font,
    ResourceRole::Image,
    ResourceRole::Gradient,
    ResourceRole::ColorProfile,
];

/// Every `Part`, in discriminant order.
pub const ALL_PARTS: [Part; 12] = [
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
    Part::Descendant,
];

/// The variant's own identifier. Exhaustive on purpose: adding a variant to any
/// of these enums stops this crate compiling until the manifest accounts for
/// it, which is a better failure than a binding that silently cannot name it.
fn space_name(s: Space) -> &'static str {
    match s {
        Space::Node => "Node",
        Space::Frame => "Frame",
        Space::Source => "Source",
        Space::Origin => "Origin",
        Space::Destination => "Destination",
        Space::Resource => "Resource",
    }
}

fn part_name(p: Part) -> &'static str {
    match p {
        Part::Structure => "Structure",
        Part::Text => "Text",
        Part::TextMap => "TextMap",
        Part::Shaping => "Shaping",
        Part::IntrinsicLayout => "IntrinsicLayout",
        Part::LineLayout => "LineLayout",
        Part::Fragmentation => "Fragmentation",
        Part::Paint => "Paint",
        Part::Interaction => "Interaction",
        Part::SourceLink => "SourceLink",
        Part::Validation => "Validation",
        Part::Descendant => "Descendant",
    }
}

fn role_name(r: ResourceRole) -> &'static str {
    match r {
        ResourceRole::Font => "Font",
        ResourceRole::Image => "Image",
        ResourceRole::Gradient => "Gradient",
        ResourceRole::ColorProfile => "ColorProfile",
    }
}

// A small JSON writer rather than a serialization dependency. Every string in
// this manifest is a Rust identifier or a fixed keyword, so there is nothing to
// escape -- which `no_escaping_is_needed_because_every_name_is_an_identifier`
// asserts rather than assumes.
struct Json {
    out: String,
    indent: usize,
}

impl Json {
    fn new() -> Self {
        Json {
            out: String::new(),
            indent: 0,
        }
    }
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }
    fn open(&mut self, s: &str) {
        self.line(s);
        self.indent += 1;
    }
    fn close(&mut self, s: &str) {
        self.indent -= 1;
        self.line(s);
    }
}

/// One field of a `#[repr(C)]` aggregate. Size is deliberately absent: a
/// field's size is its type's size, and that type is itself in the manifest.
/// Recording it twice would let the two disagree.
struct Field {
    name: &'static str,
    ty: &'static str,
    offset: usize,
}

fn emit_struct(j: &mut Json, name: &str, size: usize, align: usize, fields: &[Field], last: bool) {
    j.open("{");
    j.line(&format!("\"name\": \"{name}\","));
    j.line("\"kind\": \"struct\",");
    j.line(&format!("\"size\": {size},"));
    j.line(&format!("\"align\": {align},"));
    j.open("\"fields\": [");
    for (i, f) in fields.iter().enumerate() {
        let comma = if i + 1 == fields.len() { "" } else { "," };
        j.line(&format!(
            "{{ \"name\": \"{}\", \"type\": \"{}\", \"offset\": {} }}{comma}",
            f.name, f.ty, f.offset
        ));
    }
    j.close("]");
    j.close(if last { "}" } else { "}," });
}

fn emit_scalar(j: &mut Json, name: &str, size: usize, align: usize, repr: &str, nonzero: bool) {
    j.open("{");
    j.line(&format!("\"name\": \"{name}\","));
    j.line("\"kind\": \"scalar\",");
    j.line(&format!("\"size\": {size},"));
    j.line(&format!("\"align\": {align},"));
    j.line(&format!("\"repr\": \"{repr}\","));
    // A binding that writes 0 into a niche-optimized scalar has produced a
    // value Rust considers impossible. This is the one validity rule an
    // offset table cannot express, so it is stated.
    j.line(&format!("\"nonzero\": {nonzero}"));
    j.close("},");
}

fn emit_enum(j: &mut Json, name: &str, size: usize, align: usize, variants: &[(&str, u8)]) {
    j.open("{");
    j.line(&format!("\"name\": \"{name}\","));
    j.line("\"kind\": \"enum\",");
    j.line(&format!("\"size\": {size},"));
    j.line(&format!("\"align\": {align},"));
    j.line("\"repr\": \"u8\",");
    j.open("\"variants\": [");
    for (i, (vn, v)) in variants.iter().enumerate() {
        let comma = if i + 1 == variants.len() { "" } else { "," };
        j.line(&format!("{{ \"name\": \"{vn}\", \"value\": {v} }}{comma}"));
    }
    j.close("]");
    j.close("},");
}

/// The manifest, generated from the compiled types.
pub fn manifest_json() -> String {
    let mut j = Json::new();
    j.open("{");
    j.line(&format!("\"manifest_format\": {MANIFEST_FORMAT},"));
    j.line(
        "\"generated_by\": \"composition-ir-ffi: cargo test -p composition-ir-ffi, UPDATE_ABI=1\",",
    );
    j.open("\"types\": [");

    emit_scalar(
        &mut j,
        "Domain",
        size_of::<Domain>(),
        align_of::<Domain>(),
        "u64",
        true,
    );
    emit_scalar(
        &mut j,
        "Revision",
        size_of::<Revision>(),
        align_of::<Revision>(),
        "u64",
        true,
    );

    emit_enum(
        &mut j,
        "Space",
        size_of::<Space>(),
        align_of::<Space>(),
        &ALL_SPACES
            .iter()
            .map(|s| (space_name(*s), *s as u8))
            .collect::<Vec<_>>(),
    );
    emit_enum(
        &mut j,
        "Part",
        size_of::<Part>(),
        align_of::<Part>(),
        &ALL_PARTS
            .iter()
            .map(|p| (part_name(*p), *p as u8))
            .collect::<Vec<_>>(),
    );
    emit_enum(
        &mut j,
        "ResourceRole",
        size_of::<ResourceRole>(),
        align_of::<ResourceRole>(),
        &ALL_RESOURCE_ROLES
            .iter()
            .map(|r| (role_name(*r), *r as u8))
            .collect::<Vec<_>>(),
    );

    // `Parts` is a bitset over `Part`, and the flag values are emitted rather
    // than left to the binding to derive. The shift rule is one line and every
    // binding would reimplement it; `parts_flags_agree_with_part_discriminants`
    // is what keeps this from becoming a second source of truth.
    j.open("{");
    j.line("\"name\": \"Parts\",");
    j.line("\"kind\": \"bitset\",");
    j.line(&format!("\"size\": {},", size_of::<Parts>()));
    j.line(&format!("\"align\": {},", align_of::<Parts>()));
    j.line("\"repr\": \"u16\",");
    j.line("\"of\": \"Part\",");
    j.open("\"flags\": [");
    for (i, p) in ALL_PARTS.iter().enumerate() {
        let comma = if i + 1 == ALL_PARTS.len() { "" } else { "," };
        j.line(&format!(
            "{{ \"name\": \"{}\", \"bit\": {} }}{comma}",
            part_name(*p),
            Parts::of(*p).bits()
        ));
    }
    j.close("]");
    j.close("},");

    emit_struct(
        &mut j,
        "SnapshotVersion",
        size_of::<SnapshotVersion>(),
        align_of::<SnapshotVersion>(),
        &[
            Field {
                name: "domain",
                ty: "Domain",
                offset: offset_of!(SnapshotVersion, domain),
            },
            Field {
                name: "revision",
                ty: "Revision",
                offset: offset_of!(SnapshotVersion, revision),
            },
        ],
        false,
    );
    emit_struct(
        &mut j,
        "Id",
        size_of::<Id>(),
        align_of::<Id>(),
        &[
            Field {
                name: "domain",
                ty: "Domain",
                offset: offset_of!(Id, domain),
            },
            Field {
                name: "ordinal",
                ty: "NonZeroU64",
                offset: offset_of!(Id, ordinal),
            },
        ],
        false,
    );
    emit_struct(
        &mut j,
        "Address",
        size_of::<Address>(),
        align_of::<Address>(),
        &[
            Field {
                name: "space",
                ty: "Space",
                offset: offset_of!(Address, space),
            },
            Field {
                name: "id",
                ty: "Id",
                offset: offset_of!(Address, id),
            },
        ],
        false,
    );
    emit_struct(
        &mut j,
        "Diff",
        size_of::<Diff>(),
        align_of::<Diff>(),
        &[
            Field {
                name: "address",
                ty: "Address",
                offset: offset_of!(Diff, address),
            },
            Field {
                name: "parts",
                ty: "Parts",
                offset: offset_of!(Diff, parts),
            },
        ],
        false,
    );
    emit_struct(
        &mut j,
        "Rect",
        size_of::<Rect>(),
        align_of::<Rect>(),
        &[
            Field {
                name: "x",
                ty: "i64",
                offset: offset_of!(Rect, x),
            },
            Field {
                name: "y",
                ty: "i64",
                offset: offset_of!(Rect, y),
            },
            Field {
                name: "width",
                ty: "i64",
                offset: offset_of!(Rect, width),
            },
            Field {
                name: "height",
                ty: "i64",
                offset: offset_of!(Rect, height),
            },
        ],
        false,
    );
    emit_struct(
        &mut j,
        "Rgba",
        size_of::<Rgba>(),
        align_of::<Rgba>(),
        &[
            Field {
                name: "r",
                ty: "u8",
                offset: offset_of!(Rgba, r),
            },
            Field {
                name: "g",
                ty: "u8",
                offset: offset_of!(Rgba, g),
            },
            Field {
                name: "b",
                ty: "u8",
                offset: offset_of!(Rgba, b),
            },
            Field {
                name: "a",
                ty: "u8",
                offset: offset_of!(Rgba, a),
            },
        ],
        true,
    );

    j.close("]");
    j.close("}");
    j.out
}

/// The manifest as committed. Bindings are generated from this file, and
/// `the_committed_layout_matches_the_compiled_one` fails when it drifts.
pub const COMMITTED_MANIFEST: &str = include_str!("../abi/layout.json");
