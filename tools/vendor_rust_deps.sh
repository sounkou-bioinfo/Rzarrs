#!/usr/bin/env sh
# Regenerate src/rust/vendor.tar.xz from Cargo.lock and apply repository-owned
# patches to vendored crates before packaging.
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
RUST_DIR="$ROOT_DIR/src/rust"
PATCH_DIR="$ROOT_DIR/tools/vendor-patches"
KEEP_VENDOR=0

case "${1:-}" in
  --keep-vendor)
    KEEP_VENDOR=1
    ;;
  "")
    ;;
  *)
    echo "usage: $0 [--keep-vendor]" >&2
    exit 2
    ;;
esac

cd "$RUST_DIR"
rm -rf vendor
cargo vendor vendor >/dev/null

PATCHES=""
if [ -d "$PATCH_DIR" ]; then
  for patch in "$PATCH_DIR"/*.patch; do
    [ -e "$patch" ] || continue
    echo "applying vendor patch: ${patch#$ROOT_DIR/}"
    git apply "$patch"
    PATCHES="$PATCHES $patch"
  done
fi

if [ -n "$PATCHES" ]; then
  python3 - "$RUST_DIR" $PATCHES <<'PY'
import hashlib
import json
import pathlib
import re
import sys

rust_dir = pathlib.Path(sys.argv[1])
patch_paths = [pathlib.Path(p) for p in sys.argv[2:]]
touched = set()
for patch_path in patch_paths:
    for line in patch_path.read_text(encoding="utf-8").splitlines():
        match = re.match(r"\+\+\+\s+(?:b/)?(vendor/[^\t ]+)", line)
        if match and match.group(1) != "/dev/null":
            touched.add(match.group(1))

for rel in sorted(touched):
    parts = pathlib.PurePosixPath(rel).parts
    if len(parts) < 3 or parts[0] != "vendor":
        continue
    crate_rel = pathlib.PurePosixPath(*parts[:2])
    file_rel = pathlib.PurePosixPath(*parts[2:]).as_posix()
    checksum_path = rust_dir / crate_rel.as_posix() / ".cargo-checksum.json"
    file_path = rust_dir / rel
    if not checksum_path.exists():
        raise SystemExit(f"missing checksum file for patched vendored crate: {checksum_path}")
    with checksum_path.open(encoding="utf-8") as f:
        checksum = json.load(f)
    files = checksum.setdefault("files", {})
    if file_path.exists():
        files[file_rel] = hashlib.sha256(file_path.read_bytes()).hexdigest()
    else:
        files.pop(file_rel, None)
    with checksum_path.open("w", encoding="utf-8") as f:
        json.dump(checksum, f, separators=(",", ":"))
    print(f"updated cargo checksum: {crate_rel}/{file_rel}")
PY
fi

rm -f vendor.tar.xz
# Normalize the tarball metadata so repeated runs from the same Cargo.lock and
# patch set produce stable bytes on GNU tar.
tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -cJf vendor.tar.xz vendor

if [ "$KEEP_VENDOR" -eq 0 ]; then
  rm -rf vendor
fi

echo "wrote src/rust/vendor.tar.xz"
