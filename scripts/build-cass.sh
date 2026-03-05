#!/usr/bin/env bash
set -euo pipefail

# Build CASS search backend from source (does NOT modify PATH).
# Binary output: search-backend/cass/target/release/cass

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASS_DIR="$REPO_ROOT/search-backend/cass"

# Ensure submodules are initialized
if [ ! -f "$CASS_DIR/Cargo.toml" ]; then
  echo "→ Initializing git submodules..."
  git -C "$REPO_ROOT" submodule update --init --recursive
fi

echo "→ Building CASS (release mode)..."
cargo build --release --manifest-path "$CASS_DIR/Cargo.toml"

echo ""
echo "✓ Binary: $CASS_DIR/target/release/cass"
echo "  Run: $CASS_DIR/target/release/cass --version"
