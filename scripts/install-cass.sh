#!/usr/bin/env bash
set -euo pipefail

# ─── CASS Search Backend Installer ───────────────────────────────────────────
# Usage:
#   ./scripts/install-cass.sh            # build + add to PATH via shell profile
#   ./scripts/install-cass.sh --system   # build + copy to /usr/local/bin (sudo)
#   ./scripts/install-cass.sh --help     # show this help
# ─────────────────────────────────────────────────────────────────────────────

SYSTEM_INSTALL=false

for arg in "$@"; do
  case "$arg" in
    --system) SYSTEM_INSTALL=true ;;
    --help|-h)
      sed -n '3,7p' "$0"
      exit 0
      ;;
    *) echo "Unknown option: $arg"; exit 1 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASS_DIR="$REPO_ROOT/search-backend/cass"

echo "══════════════════════════════════════════════"
echo "  CASS Search Backend Installer"
echo "══════════════════════════════════════════════"

# ── 1. Check Rust ────────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  echo ""
  echo "→ Rust not found. Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  echo "  ✓ Rust $(rustc --version | awk '{print $2}') installed"
else
  echo "→ Rust found: $(rustc --version | awk '{print $2}')"
fi

# ── 2. Init submodules ──────────────────────────────────────────────────────
if [ ! -f "$CASS_DIR/Cargo.toml" ]; then
  echo ""
  echo "→ Initializing git submodules..."
  git -C "$REPO_ROOT" submodule update --init --recursive
  echo "  ✓ Submodules ready"
else
  echo "→ Submodules already initialized"
fi

# ── 3. Build release ────────────────────────────────────────────────────────
echo ""
echo "→ Building CASS (release mode with LTO — this takes 3-8 minutes)..."
# Must cd to CASS_DIR so rust-toolchain.toml is picked up
cd "$CASS_DIR"
cargo build --release
echo "  ✓ Build complete"

BINARY="$CASS_DIR/target/release/cass"
if [ ! -f "$BINARY" ]; then
  echo "ERROR: Binary not found at $BINARY"
  exit 1
fi

# ── 4. Install ──────────────────────────────────────────────────────────────
echo ""
if $SYSTEM_INSTALL; then
  echo "→ Installing to /usr/local/bin (requires sudo)..."
  sudo cp "$BINARY" /usr/local/bin/cass
  echo "  ✓ Installed: $(cass --version 2>/dev/null || echo '/usr/local/bin/cass')"
else
  # Add to shell profile
  BIN_DIR="$CASS_DIR/target/release"
  EXPORT_LINE="export PATH=\"$BIN_DIR:\$PATH\""

  PROFILE=""
  if [ -f "$HOME/.zshrc" ]; then
    PROFILE="$HOME/.zshrc"
  elif [ -f "$HOME/.bashrc" ]; then
    PROFILE="$HOME/.bashrc"
  elif [ -f "$HOME/.bash_profile" ]; then
    PROFILE="$HOME/.bash_profile"
  fi

  if [ -n "$PROFILE" ]; then
    if ! grep -qF "$BIN_DIR" "$PROFILE" 2>/dev/null; then
      echo "" >> "$PROFILE"
      echo "# CASS search backend (AgentRoom)" >> "$PROFILE"
      echo "$EXPORT_LINE" >> "$PROFILE"
      echo "  ✓ Added to $PROFILE"
    else
      echo "  ✓ PATH already configured in $PROFILE"
    fi
    echo ""
    echo "  Run:  source $PROFILE"
    echo "  Then: cass --version"
  else
    echo "  ⚠ No shell profile found. Add this to your shell config:"
    echo "    $EXPORT_LINE"
  fi
fi

echo ""
echo "══════════════════════════════════════════════"
echo "  Done! Next steps:"
echo "    cass index --full    # index your agent sessions"
echo "    cass health --json   # verify index"
echo "══════════════════════════════════════════════"
