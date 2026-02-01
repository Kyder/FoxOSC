#!/bin/bash

set -e

echo "🔧 Building Fox OSC with WASM Plugins"
echo "======================================"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Build core app
echo -e "${BLUE}Building core application...${NC}"
cargo build --release
echo -e "${GREEN}âœ“ Core app built${NC}"

# Build plugins
echo -e "\n${BLUE}Building plugins...${NC}"

PLUGIN_DIR="$HOME/.config/fox-osc/plugins"
mkdir -p "$PLUGIN_DIR"

for plugin in plugins/*/; do
    if [ -d "$plugin" ]; then
        plugin_name=$(basename "$plugin")
        echo -e "${BLUE}  Building ${plugin_name}...${NC}"
        
        cd "$plugin"
        cargo build --target wasm32-unknown-unknown --release
        
        # Find the .wasm file
        wasm_file=$(find target/wasm32-unknown-unknown/release -name "*.wasm" -type f | head -n 1)
        
        if [ -n "$wasm_file" ]; then
            cp "$wasm_file" "$PLUGIN_DIR/"
            echo -e "${GREEN}  âœ“ ${plugin_name} installed to ${PLUGIN_DIR}${NC}"
        else
            echo -e "  âœ— Failed to find .wasm file for ${plugin_name}"
        fi
        
        cd - > /dev/null
    fi
done

echo -e "\n${GREEN}======================================"
echo -e "Build complete!${NC}"
echo ""
echo "Run the app:"
echo "  ./target/release/fox-osc"
echo ""
echo "Plugins installed to:"
echo "  $PLUGIN_DIR"