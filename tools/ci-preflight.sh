#!/usr/bin/env bash
# Check what the build needs from the machine it runs on, before compiling
# anything: "linker `cc` not found" minutes into a job is a worse way to learn
# the runner is missing a compiler.
#
#   tools/ci-preflight.sh            test build
#   tools/ci-preflight.sh --release  release build (adds musl target, zigbuild)
set -euo pipefail

missing=()
need() { command -v "$1" >/dev/null 2>&1 || missing+=("$2"); }

need cargo "cargo/rustc  - a Rust toolchain for the runner user (https://rustup.rs)"
need cc    "cc           - a C compiler (build-essential)"
if [[ "${1:-}" == "--release" ]]; then
    need curl   "curl         - curl (fetches zig)"
    need tar    "tar          - tar"
    need xz     "xz           - xz-utils (zig is a .tar.xz)"
    need rustup "rustup       - needed to add the musl target"
    need objdump "objdump      - binutils (checks the GUI's glibc floor)"
fi

if ((${#missing[@]})); then
    echo "This machine cannot build dm32 yet. Missing:" >&2
    printf '  %s\n' "${missing[@]}" >&2
    echo >&2
    echo "On Debian/Ubuntu: apt install build-essential binutils curl xz-utils" >&2
    echo "and, as the runner user, a Rust toolchain: https://rustup.rs" >&2
    exit 1
fi
echo "==> $(cargo --version)"
