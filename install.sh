#!/bin/bash
# RTK-MCP Installer for macOS
# Downloads pre-built binaries — no Rust needed
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

INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"

# Detect architecture
ARCH=$(uname -m)
if [[ "$ARCH" == "arm64" ]]; then
    RTK_TARBALL="rtk-aarch64-apple-darwin.tar.gz"
else
    RTK_TARBALL="rtk-x86_64-apple-darwin.tar.gz"
fi

# 1. Install RTK (pre-built binary from GitHub releases)
if command -v rtk &>/dev/null && rtk --version 2>/dev/null | grep -q "^rtk "; then
    echo "✓ RTK $(rtk --version 2>/dev/null | head -1) already installed"
else
    echo "Downloading RTK..."
    RTK_VERSION=$(curl -sL https://api.github.com/repos/rtk-ai/rtk/releases/latest | grep '"tag_name"' | cut -d'"' -f4)
    curl -sL "https://github.com/rtk-ai/rtk/releases/download/${RTK_VERSION}/${RTK_TARBALL}" | tar xz -C "$INSTALL_DIR"
    echo "✓ RTK ${RTK_VERSION} installed to $INSTALL_DIR/rtk"
fi

# 2. Build rtk-mcp (needs Rust >= 1.85 — small project, ~30s build)
if ! command -v cargo &>/dev/null; then
    echo ""
    echo "Rust is needed to build rtk-mcp (one-time, ~30s)."
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet
    source "$HOME/.cargo/env"
else
    # Ensure Rust is recent enough (edition2024 requires >= 1.85)
    RUST_MINOR=$(rustc --version | sed 's/rustc [0-9]*\.\([0-9]*\)\..*/\1/')
    if [[ "$RUST_MINOR" -lt 85 ]]; then
        echo "Rust too old (1.${RUST_MINOR}), updating to latest..."
        rustup update stable
        hash -r
    fi
fi
# Reload PATH to pick up updated cargo/rustc
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
hash -r
echo "✓ Rust $(rustc --version)"

MCP_DIR="$HOME/.local/share/rtk-mcp"
if [[ -d "$MCP_DIR" ]]; then
    echo "Updating rtk-mcp..."
    cd "$MCP_DIR" && git pull --ff-only --quiet
else
    echo "Cloning rtk-mcp..."
    git clone --quiet https://github.com/ousamabenyounes/rtk-mcp.git "$MCP_DIR"
    cd "$MCP_DIR"
fi

echo "Building rtk-mcp..."
cargo build --release --quiet
BINARY="$MCP_DIR/target/release/rtk-mcp"
echo "✓ rtk-mcp built"

# 3. Configure Claude Desktop
CONFIG_DIR="$HOME/Library/Application Support/Claude"
CONFIG_FILE="$CONFIG_DIR/claude_desktop_config.json"
mkdir -p "$CONFIG_DIR"

if [[ -f "$CONFIG_FILE" ]]; then
    if grep -q "rtk" "$CONFIG_FILE" 2>/dev/null; then
        echo "✓ Claude Desktop already configured"
    else
        python3 -c "
import json
with open('$CONFIG_FILE') as f:
    config = json.load(f)
config.setdefault('mcpServers', {})
config['mcpServers']['rtk'] = {'command': '$BINARY'}
with open('$CONFIG_FILE', 'w') as f:
    json.dump(config, f, indent=2)
print('✓ Claude Desktop config updated')
"
    fi
else
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

# 4. Add to PATH if needed
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$HOME/.zshrc"
    echo "✓ Added $INSTALL_DIR to PATH (restart terminal)"
fi

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  Done! Restart Claude Desktop (Cmd+Q)       ║"
echo "║  Then ask Claude: 'run ls -la'              ║"
echo "╚══════════════════════════════════════════════╝"
