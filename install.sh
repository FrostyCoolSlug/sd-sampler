#!/usr/bin/env bash

# This probably needs a little more, but for now should be sufficient

set -euo pipefail
cd "$(dirname "$0")"

BIN=sd-sampler
ID=com.frostycoolslug.sd-sampler.sdPlugin
DEST="$HOME/.config/opendeck/plugins"

cargo build --release

cd web
npm install
npm run build
cd ..

mkdir -p "$DEST/$ID"

cp -r plugin/$ID/. "$DEST/$ID/"

install -m 755 "target/release/$BIN" "$DEST/$ID/$BIN"
echo "Installed to $DEST/$ID - restart OpenDeck."
