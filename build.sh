#!/usr/bin/env bash

# This probably needs a little more, but for now should be sufficient
set -euo pipefail
cd "$(dirname "$0")"

WD=$(pwd)
BIN=sd-sampler
ID=com.frostycoolslug.sd-sampler.sdPlugin
DEST="./build"

# Build the Rust Binary
cargo build --release

# Build the PI
cd web
npm install
npm run build
cd ..

# Create a temporary directory to assemble things into
rm -rf $DEST
mkdir -p "$DEST/$ID"

# AVENGERS, ASSEMBLE
cp -r plugin/$ID/. "$DEST/$ID/"
install -m 755 "target/release/$BIN" "$DEST/$ID/$BIN"

# Remove any old builds
rm "$WD"/"$ID"

# Drop into the build dir, and zip it up into a .sdPlugin
cd $DEST
zip -r "$WD"/"$ID" $ID
cd "$WD"
rm -rf $DEST

# Done.

