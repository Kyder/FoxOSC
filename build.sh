#!/bin/bash
# Builds Fox OSC and every plugin in plugins/*, then installs the plugins.
set -euo pipefail

cd "$(dirname "$0")"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo "🔧 Building Fox OSC with WASM plugins"
echo "======================================"

# egui 0.37 needs Rust 1.95 or newer
RUSTC_VER=$(rustc --version | awk '{print $2}')
if [ "$(printf '%s\n' "1.95.0" "$RUSTC_VER" | sort -V | head -n1)" != "1.95.0" ]; then
    echo -e "${RED}✗ Rust $RUSTC_VER is too old, 1.95+ needed. Update your rust package (or: rustup update)${NC}"
    exit 1
fi

# The WASM standard library must be installed. Check the sysroot directly so
# this works with distro Rust as well as rustup.
if [ ! -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown" ]; then
    if command -v rustup >/dev/null 2>&1; then
        echo -e "${BLUE}Installing wasm32-unknown-unknown target...${NC}"
        rustup target add wasm32-unknown-unknown
    else
        echo -e "${RED}✗ WASM target for Rust is not installed. Install your distro's package, e.g.:${NC}"
        echo "    Arch:   sudo pacman -S rust-wasm"
        echo "    Fedora: sudo dnf install rust-std-static-wasm32-unknown-unknown"
        exit 1
    fi
fi

# Core app
echo -e "${BLUE}Building core application...${NC}"
cargo build --release -p fox-osc
echo -e "${GREEN}✓ Core app built${NC}"

# Plugins: one workspace build. Output lands in the workspace's target/,
# not in plugins/<name>/target/.
echo -e "\n${BLUE}Building plugins...${NC}"
PLUGIN_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/fox-osc/plugins"
mkdir -p "$PLUGIN_DIR"
OUT_DIR="target/wasm32-unknown-unknown/plugin"

packages=()
for manifest in plugins/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    packages+=("$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$manifest" | head -n1)")
done

if [ ${#packages[@]} -eq 0 ]; then
    echo "  (no plugins found in plugins/)"
else
    pkg_args=()
    for p in "${packages[@]}"; do pkg_args+=(-p "$p"); done
    cargo build --profile plugin --target wasm32-unknown-unknown "${pkg_args[@]}"

    for p in "${packages[@]}"; do
        wasm="$OUT_DIR/${p//-/_}.wasm"
        if [ -f "$wasm" ]; then
            cp "$wasm" "$PLUGIN_DIR/"
            echo -e "${GREEN}  ✓ $p ($(du -h "$wasm" | cut -f1)) → $PLUGIN_DIR${NC}"
        else
            echo -e "${RED}  ✗ $p: $wasm not found${NC}"
        fi
    done
fi

echo -e "\n${GREEN}======================================"
echo -e "Build complete!${NC}"
echo ""
echo "Run the app:"
echo "  ./target/release/fox-osc"
