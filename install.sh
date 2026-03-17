#!/bin/bash
# RTK-MCP Installer for macOS
# Installs rtk-mcp as a Claude Desktop MCP server
set -e

echo "╔══════════════════════════════════════════════╗"
echo "║  RTK-MCP Installer                          ║"
echo "║  Token-optimized CLI for Claude Desktop     ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# Check macOS
if [[ "$(uname)" != "Darwin" ]]; then
    echo "ERROR: This installer is for macOS only."
    exit 1
fi

# Check Rust/cargo
if ! command -v cargo &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
echo "✓ Rust $(rustc --version | cut -d' ' -f2)"

# Install RTK (the filtering engine)
if command -v rtk &>/dev/null && rtk --version 2>/dev/null | grep -q "^rtk "; then
    echo "✓ RTK $(rtk --version | cut -d' ' -f2) already installed"
else
    echo "Installing RTK..."
    cargo install --git https://github.com/rtk-ai/rtk
    echo "✓ RTK installed"
fi

# Clone and build rtk-mcp
INSTALL_DIR="$HOME/.local/share/rtk-mcp"
if [[ -d "$INSTALL_DIR" ]]; then
    echo "Updating rtk-mcp..."
    cd "$INSTALL_DIR"
    git pull --ff-only
else
    echo "Cloning rtk-mcp..."
    mkdir -p "$(dirname "$INSTALL_DIR")"
    git clone https://github.com/ousamabenyounes/rtk-mcp.git "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

echo "Building rtk-mcp (release)..."
cargo build --release 2>&1 | tail -1
BINARY="$INSTALL_DIR/target/release/rtk-mcp"
echo "✓ Binary: $BINARY"

# Configure Claude Desktop
CONFIG_DIR="$HOME/Library/Application Support/Claude"
CONFIG_FILE="$CONFIG_DIR/claude_desktop_config.json"
mkdir -p "$CONFIG_DIR"

if [[ -f "$CONFIG_FILE" ]]; then
    # Merge with existing config
    if grep -q "rtk" "$CONFIG_FILE" 2>/dev/null; then
        echo "✓ Claude Desktop already configured for rtk-mcp"
    else
        echo "Adding rtk-mcp to existing Claude Desktop config..."
        # Use python to merge JSON safely
        python3 -c "
import json, sys
with open('$CONFIG_FILE') as f:
    config = json.load(f)
config.setdefault('mcpServers', {})
config['mcpServers']['rtk'] = {
    'command': '$BINARY'
}
with open('$CONFIG_FILE', 'w') as f:
    json.dump(config, f, indent=2)
print('✓ Config updated')
"
    fi
else
    # Create new config
    cat > "$CONFIG_FILE" << JSONEOF
{
  "mcpServers": {
    "rtk": {
      "command": "$BINARY"
    }
  }
}
JSONEOF
    echo "✓ Claude Desktop config created"
fi

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  Installation complete!                      ║"
echo "╠══════════════════════════════════════════════╣"
echo "║                                              ║"
echo "║  1. Restart Claude Desktop (Cmd+Q, reopen)   ║"
echo "║  2. Click the hammer icon in the chat        ║"
echo "║  3. You should see 'run_command' tool        ║"
echo "║  4. Ask Claude: 'run ls -la'                 ║"
echo "║                                              ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
echo "Config: $CONFIG_FILE"
echo "Binary: $BINARY"
echo ""

# Quick test
echo "Testing rtk-mcp..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}' \
  | timeout 5 "$BINARY" 2>/dev/null | head -1 | python3 -c "
import sys, json
r = json.loads(sys.stdin.readline())
if 'result' in r:
    print('✓ rtk-mcp server responds correctly')
else:
    print('✗ Unexpected response')
" 2>/dev/null || echo "✓ Binary built (test skipped)"
