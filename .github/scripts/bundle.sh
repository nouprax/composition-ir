#!/usr/bin/env bash
#
# Build the artifacts a binding links, for one target, into dist/.
#
# This is what a consumer downloads: the libraries, the header a C compiler
# reads, and the layout manifest a binding that reads fields by offset reads.
# The two artifacts are not interchangeable and neither is optional --
# docs/specs/composition-ir.md §9.1 and §9.2 say why.
#
# One script rather than two copies, because the release job and the pull
# request dry run must run the same thing. A release path that is only ever
# exercised by a tag is a hypothesis until the worst possible moment.
#
# The manifest shipped here is the committed one, and it is the right one by
# construction rather than by inspection: layout.rs refuses at compile time to
# build for an ABI it does not describe, so a target that would need its own
# manifest cannot reach this script's output.
set -euo pipefail

target="${1:?usage: bundle.sh <rust-target-triple>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
[ -n "$version" ] || { echo "::error::no workspace version in Cargo.toml"; exit 1; }

name="composition-ir-${version}-${target}"
stage="dist/${name}"
built="target/${target}/release"

rm -rf "$stage"
mkdir -p "$stage/lib" "$stage/include" "$stage/abi" dist

# Both kinds, deliberately. A Swift or Kotlin consumer links the archive into
# its own binary; a JVM or a plugin host loads the shared library at run time.
# Shipping one and not the other picks a consumer's linkage model for it.
#
# The build reports what it produced, and that report is what gets bundled.
# Reading the target directory instead looks equivalent and is not: it also
# finds whatever an earlier build left there, and CI restores that directory
# from a cache. Dropping `--crate-type cdylib` here was tried, and a stale
# `.dylib` from the previous run was picked up and bundled with no complaint
# from any check.
buildlog=$(mktemp)
trap 'rm -f "$buildlog"' EXIT
cargo rustc -p composition-ir-ffi --release --target "$target" --lib \
  --crate-type staticlib --crate-type cdylib --message-format=json > "$buildlog"

# Command substitution and a here-string, rather than `mapfile` or a heredoc
# inside a process substitution. macOS ships bash 3.2: it has neither, and the
# second fails as a *parse* error that leaves the script exiting 0 having
# bundled nothing. This is the third time 3.2 has cost this repository a check
# that appeared to run.
produced=$(python3 - "$buildlog" <<'PYEOF'
import json, sys

found = set()
for line in open(sys.argv[1]):
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("reason") != "compiler-artifact":
        continue
    # Cargo names a lib target after the crate, so underscores rather than the
    # package's hyphens. Getting this wrong reports zero libraries, which the
    # caller treats as a failure rather than as an empty bundle.
    if message.get("target", {}).get("name") != "composition_ir_ffi":
        continue
    found.update(f for f in message.get("filenames", []) if f.endswith((".a", ".dylib", ".so")))
print("\n".join(sorted(found)))
PYEOF
)

libs=()
while IFS= read -r line; do
  [ -n "$line" ] && libs+=("$line")
done <<< "$produced"

if [ "${#libs[@]}" -lt 2 ]; then
  echo "::error::${target} produced ${#libs[@]} libraries (${libs[*]:-none}); expected an archive and a shared library"
  exit 1
fi
cp "${libs[@]}" "$stage/lib/"

cp packages/composition-ir-ffi/include/composition_ir.h "$stage/include/"
cp packages/composition-ir-ffi/abi/layout.json "$stage/abi/"
cp LICENSE "$stage/"

# Every file a consumer was promised, checked before the archive is sealed
# rather than after it is published.
for required in \
  "include/composition_ir.h" \
  "abi/layout.json" \
  "lib/libcomposition_ir_ffi.a" \
  "LICENSE"
do
  [ -s "$stage/$required" ] || { echo "::error::${name} is missing ${required}"; exit 1; }
done

tar -czf "dist/${name}.tar.gz" -C dist "$name"
rm -rf "$stage"
echo "built dist/${name}.tar.gz"
