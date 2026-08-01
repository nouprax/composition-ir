//! Conformance gates for the layout manifest, `docs/specs/composition-ir.md` §9.

use composition_ir::{Part, Parts};
use composition_ir_ffi::layout::{ALL_PARTS, COMMITTED_MANIFEST, manifest_json};

const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/abi/layout.json");

/// The offsets bindings are generated from must be the offsets the compiler
/// produced. A reorder of two fields in a `#[repr(C)]` struct changes no size,
/// so a size assertion cannot see it, and every binding would go on reading at
/// the old offsets without a single error anywhere.
#[test]
fn the_committed_layout_matches_the_compiled_one() {
    let compiled = manifest_json();
    if COMMITTED_MANIFEST == compiled {
        return;
    }
    if std::env::var("UPDATE_ABI").is_ok() {
        std::fs::write(MANIFEST_PATH, &compiled).expect("rewrite the manifest");
        panic!("manifest rewritten; review the diff -- every binding must be regenerated with it");
    }
    // Show the first divergence rather than two files.
    let (a, b) = (COMMITTED_MANIFEST.lines(), compiled.lines());
    let first = a
        .zip(b)
        .enumerate()
        .find(|(_, (x, y))| x != y)
        .map(|(i, (x, y))| format!("line {}:\n  committed: {x}\n  compiled:  {y}", i + 1))
        .unwrap_or_else(|| "one is a prefix of the other".to_string());
    panic!(
        "the ABI changed and abi/layout.json did not.\n{first}\n\n\
         Regenerate with UPDATE_ABI=1 cargo test -p composition-ir-ffi, and update every \
         binding in the same change -- that is what this file exists for."
    );
}

/// The manifest going stale is the failure this whole mechanism exists to
/// prevent, and it goes stale by the ABI *growing*: a type added to the IR that
/// nobody thought to describe. Reading the IR's own source is what makes that
/// impossible to miss, because it does not depend on anyone remembering.
#[test]
fn every_abi_type_the_ir_publishes_is_in_the_manifest() {
    const SOURCES: [(&str, &str); 4] = [
        (
            "address.rs",
            include_str!("../../composition-ir/src/address.rs"),
        ),
        ("node.rs", include_str!("../../composition-ir/src/node.rs")),
        (
            "delta.rs",
            include_str!("../../composition-ir/src/delta.rs"),
        ),
        (
            "placement.rs",
            include_str!("../../composition-ir/src/placement.rs"),
        ),
    ];
    let manifest = manifest_json();
    let mut checked = 0;

    for (file, src) in SOURCES {
        for chunk in src.split("#[repr(").skip(1) {
            // Whichever of the two comes first -- not `pub struct` by
            // preference. Preferring one made this scan walk past a `pub enum`
            // and check the next unrelated struct instead: it reported a type
            // that is not in the ABI at all while silently not checking the one
            // that is.
            let decl_at = [chunk.find("pub struct "), chunk.find("pub enum ")]
                .into_iter()
                .flatten()
                .min();
            let Some(decl_at) = decl_at else { continue };
            // ...and it must be the item this attribute is on. Attributes and
            // doc comments run contiguously into their item, so a blank line in
            // between means the scan has run off the end of one declaration and
            // into a later one.
            if chunk[..decl_at].contains("\n\n") {
                continue;
            }
            let decl = &chunk[decl_at..];
            let is_enum = decl.starts_with("pub enum ");
            let head: String = decl
                .trim_start_matches("pub struct ")
                .trim_start_matches("pub enum ")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                manifest.contains(&format!("\"name\": \"{head}\"")),
                "{file}: `{head}` is #[repr]'d and public but absent from the manifest, \
                 so no binding can read it"
            );
            checked += 1;

            // Its members, too: a field added to an existing type is the same
            // failure one level down.
            let Some(open) = decl.find('{') else { continue };
            let body = &decl[open + 1..];
            let end = body.find("\n}").unwrap_or(body.len());
            for line in body[..end].lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") || line.starts_with("#[") {
                    continue;
                }
                let member = if is_enum {
                    line.trim_end_matches(',').to_string()
                } else {
                    let Some(name) = line.strip_prefix("pub ") else {
                        continue;
                    };
                    name.split(':')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                if member.is_empty() || !member.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                assert!(
                    manifest.contains(&format!("\"name\": \"{member}\"")),
                    "{file}: {head}.{member} is public but absent from the manifest"
                );
            }
        }
    }
    assert!(
        checked >= 10,
        "found only {checked} ABI types by reading the source; the scan stopped working"
    );
}

/// `Parts` publishes its flag values so no binding has to rederive the shift.
/// Emitting them makes a second place the truth could live, and this is what
/// keeps the two from disagreeing.
#[test]
fn parts_flags_agree_with_part_discriminants() {
    let manifest = manifest_json();
    for p in ALL_PARTS {
        let expected = 1u16 << (p as u16);
        assert_eq!(
            Parts::of(p).bits(),
            expected,
            "Parts::of has stopped being 1 << discriminant"
        );
        assert!(
            manifest.contains(&format!("\"bit\": {expected} }}")),
            "the manifest does not publish the flag value {expected}"
        );
    }
    // and every part is distinct, so a binding masking one never matches another
    let mut seen = 0u16;
    for p in ALL_PARTS {
        assert_eq!(seen & Parts::of(p).bits(), 0, "two parts share a bit");
        seen |= Parts::of(p).bits();
    }
    assert_eq!(seen.count_ones() as usize, ALL_PARTS.len());
}

/// The manifest is written by a small emitter rather than a serialization
/// dependency, which is sound only while nothing in it needs escaping.
#[test]
fn no_escaping_is_needed_because_every_name_is_an_identifier() {
    let manifest = manifest_json();
    for (i, line) in manifest.lines().enumerate() {
        for value in line.split('"').skip(1).step_by(2) {
            assert!(
                value
                    .chars()
                    .all(|c| c.is_alphanumeric() || " _-.,:/=!".contains(c)),
                "line {} carries a string needing JSON escaping: {value:?}",
                i + 1
            );
        }
    }
    assert!(
        !manifest.contains('\\'),
        "an escape appeared in the manifest"
    );
}

/// `Descendant` is the one part a consumer may need to mask, so a binding has
/// to be able to name it. It has been dropped from a published vocabulary once
/// already, in an earlier design.
#[test]
fn the_manifest_names_descendant() {
    let manifest = manifest_json();
    assert!(manifest.contains("\"name\": \"Descendant\""));
    assert!(manifest.contains(&format!(
        "\"bit\": {} }}",
        Parts::of(Part::Descendant).bits()
    )));
}
