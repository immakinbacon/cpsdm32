#!/usr/bin/env bash
# Build the shippable binaries into dist/:
#
#   dist/dm32      command-line tool, fully static (musl): runs on any x86_64 Linux
#   dist/dm32cps   GUI, linked against glibc $GLIBC_FLOOR through zig, so it runs on
#                  distributions back to that glibc (the GUI needs the system's
#                  OpenGL/X11/Wayland libraries, so it cannot be static)
#   dist/SHA256SUMS
#
# The musl target and cargo-zigbuild are added for the current user if missing;
# nothing is installed system-wide.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
GLIBC_FLOOR="${GLIBC_FLOOR:-2.28}"
export PATH="$HOME/.cargo/bin:$PATH"

rustup target list --installed | grep -qx x86_64-unknown-linux-musl \
    || rustup target add x86_64-unknown-linux-musl
command -v cargo-zigbuild >/dev/null || cargo install --locked cargo-zigbuild

if ! command -v zig >/dev/null; then
    tools/get-zig.sh
    export PATH="$ROOT/tools/toolchain/zig:$PATH"
fi
export ZIG_GLOBAL_CACHE_DIR="$ROOT/tools/toolchain/zig-cache"
export ZIG_LOCAL_CACHE_DIR="$ZIG_GLOBAL_CACHE_DIR"

echo "==> dm32 (static, musl)"
cargo build --release --locked -p dm32-cli --target x86_64-unknown-linux-musl
echo "==> dm32cps (glibc $GLIBC_FLOOR)"
cargo zigbuild --release --locked -p dm32-gui --target "x86_64-unknown-linux-gnu.$GLIBC_FLOOR"

rm -rf dist && mkdir dist
cp target/x86_64-unknown-linux-musl/release/dm32 dist/
cp target/x86_64-unknown-linux-gnu/release/dm32cps dist/

# Fail rather than ship a GUI that quietly needs a newer glibc than promised.
need="$(objdump -T dist/dm32cps | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1)"
if [[ "$(printf '%s\n%s\n' "$need" "$GLIBC_FLOOR" | sort -V | tail -1)" != "$GLIBC_FLOOR" ]]; then
    echo "dm32cps requires glibc $need, above the $GLIBC_FLOOR floor" >&2
    exit 1
fi

(cd dist && sha256sum dm32 dm32cps > SHA256SUMS)
echo "==> dist/:"
ls -l dist
