#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"
ROOT=$PWD

BIN=sd-sampler
ID=com.frostycoolslug.sd-sampler.sdPlugin
SRC="$ROOT/plugin/$ID"
OUT="$ROOT/$ID"
PLUGINS_DIR="${OPENDECK_PLUGINS_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/opendeck/plugins}"

die() { echo "error: $*" >&2; exit 1; }

usage() {
    cat <<EOF
Usage: ${0##*/} [--package | --install]

  -p, --package   Build and create $ID (zip) in the project root (default)
  -i, --install   Build and install straight into OpenDeck's plugin directory
  -h, --help      Show this help
EOF
}

MODE=package
case "${1:-}" in
    ""|-p|--package) MODE=package ;;
    -i|--install)    MODE=install ;;
    -h|--help)       usage; exit 0 ;;
    *)               usage >&2; exit 1 ;;
esac
[[ $# -le 1 ]] || { usage >&2; exit 1; }

# Fail early if something we need is missing
needed=(cargo npm install)
[[ $MODE == package ]] && needed+=(zip)
for cmd in "${needed[@]}"; do
    command -v "$cmd" >/dev/null || die "'$cmd' not found in PATH"
done
[[ -d $SRC ]] || die "plugin source not found: $SRC"

# Stage in a throwaway directory, removed on any exit path
STAGE=$(mktemp -d)
trap 'rm -rf -- "${STAGE:?}"' EXIT

# Packaging wants exact lockfile builds, local installs can be looser
if [[ $MODE == package ]]; then
    CARGO_ARGS=(--release --locked)
    NPM_DEPS=ci
else
    CARGO_ARGS=(--release)
    NPM_DEPS=install
fi

echo "==> Building plugin binary"
cargo build "${CARGO_ARGS[@]}"

echo "==> Building property inspector"
(cd web && npm "$NPM_DEPS" && npm run build)

echo "==> Assembling"
mkdir -p "$STAGE/$ID"
cp -r "$SRC/." "$STAGE/$ID/"
install -m 755 "target/release/$BIN" "$STAGE/$ID/$BIN"

case $MODE in
    package)
        echo "==> Packaging"
        (cd "$STAGE" && zip -qrX "$STAGE/out.zip" "$ID")
        mv -f -- "$STAGE/out.zip" "$OUT"
        echo "==> Done: $OUT"
        ;;
    install)
        echo "==> Installing"
        mkdir -p "$PLUGINS_DIR"
        # Replace the old install wholesale so deleted files don't linger
        rm -rf -- "${PLUGINS_DIR:?}/$ID"
        cp -r "$STAGE/$ID" "$PLUGINS_DIR/$ID"
        echo "==> Installed to $PLUGINS_DIR/$ID - restart OpenDeck."
        ;;
esac