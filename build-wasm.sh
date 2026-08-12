#!/bin/bash
# Build WASM for font-maker-cli
set -e

cd "$(dirname "$0")/font-maker-cli"

VERSION="${1:-$(date +%s)}"
echo "Building WASM with version=${VERSION}..."

wasm-pack build --target web --out-dir wasm/pkg --release

# Add cache-busting version param to WASM URL
sed -i '' "s|font_maker_cli_bg.wasm'|font_maker_cli_bg.wasm?v=${VERSION}'|g" wasm/pkg/font_maker_cli.js

echo "Done. Updated WASM URL to include ?v=${VERSION}"
ls -lh wasm/pkg/font_maker_cli_bg.wasm
