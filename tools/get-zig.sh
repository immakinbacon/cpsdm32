#!/usr/bin/env bash
# Fetch the zig used to link the GUI release build.
#
#   tools/get-zig.sh          installs into tools/toolchain/zig/
#
# Linking on a modern distribution records that distribution's glibc as a hard
# requirement; zig carries stubs for older versions, so linking through it puts
# the floor where we choose (see GLIBC_FLOOR in build-release.sh) regardless of
# the machine that builds. Nothing is installed system-wide; a zig already on
# PATH is used instead and this is never called.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="$HERE/toolchain/zig"

ZIG_VERSION="${ZIG_VERSION:-0.15.2}"
ARCH="$(uname -m)"

# The published SHA-256 of the tarball: a toolchain that links every shipped
# binary is worth verifying.
case "$ARCH-$ZIG_VERSION" in
    x86_64-0.15.2)
        SHA256=02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239 ;;
    *)
        echo "get-zig.sh has no checksum for zig $ZIG_VERSION on $ARCH." >&2
        echo "Add one from https://ziglang.org/download/index.json, or put zig on PATH." >&2
        exit 1 ;;
esac

if [[ -x "$DEST/zig" ]]; then
    echo "==> zig $("$DEST/zig" version) already present"
    exit 0
fi

NAME="zig-$ARCH-linux-$ZIG_VERSION"
URL="https://ziglang.org/download/$ZIG_VERSION/$NAME.tar.xz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "==> downloading zig $ZIG_VERSION ($ARCH)"
curl -sSLo "$tmp/zig.tar.xz" "$URL"
echo "$SHA256  $tmp/zig.tar.xz" | sha256sum -c - >/dev/null
mkdir -p "$HERE/toolchain"
tar -xJf "$tmp/zig.tar.xz" -C "$tmp"
rm -rf "$DEST"
mv "$tmp/$NAME" "$DEST"
echo "==> zig $("$DEST/zig" version) ready"
