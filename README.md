# rtk-mcp

MCP server bridge for [RTK (Rust Token Killer)](https://github.com/rtk-ai/rtk) — token-optimized CLI output for any MCP-compatible client.

## What it does

`rtk-mcp` exposes a single MCP tool (`run_command`) that routes shell commands through [RTK](https://github.com/rtk-ai/rtk) for **60-90% token reduction** before the output reaches your LLM's context window.

```
MCP Client (Claude Desktop, Cursor, Windsurf, ...)
  → rtk-mcp (this server)
    → rtk git status     ← filtered output (78% fewer tokens)
    → returns to LLM
```

Without RTK installed, commands execute normally (no filtering, no token savings).

## Why

[RTK](https://github.com/rtk-ai/rtk) already saves tokens for **Claude Code** and **Gemini CLI** via hooks. But hooks are client-specific — each new AI tool needs its own integration.

MCP is a universal protocol. One server, every client:

| Client | MCP Support |
|--------|-------------|
| Claude Desktop | Yes |
| Cursor | Yes |
| Windsurf | Yes |
| Cline (VS Code) | Yes |
| Continue | Yes |
| Zed | Yes |
| VS Code (native) | Yes |
| GitHub Copilot | Yes |

## Real-world savings

Measured over 25 days of daily usage with RTK:

| Filter | Token reduction |
|--------|----------------|
| `cargo test` | 97.8% |
| `env` | 99.3% |
| `cargo clippy` | 92.5% |
| `find` | 79.2% |
| `ls` | 67-78% |
| `grep` | 64.4% |

Total: **5.3M tokens saved** across 4,876 commands.

## Install

### Prerequisites

Install [RTK](https://github.com/rtk-ai/rtk) first:

```bash
cargo install --git https://github.com/rtk-ai/rtk

# Verify
rtk --version   # Should show "rtk X.Y.Z"
rtk gain         # Should work (not "command not found")
```

### Build rtk-mcp

```bash
git clone https://github.com/ousamabenyounes/rtk-mcp.git
cd rtk-mcp
cargo build --release
```

The binary is at `target/release/rtk-mcp`.

## Configure your MCP client

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "rtk": {
      "command": "/path/to/rtk-mcp"
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json` in your project:

```json
{
  "mcpServers": {
    "rtk": {
      "command": "/path/to/rtk-mcp"
    }
  }
}
```

### VS Code / Windsurf / Cline

Add to your MCP settings (check each client's documentation for the exact config file path):

```json
{
  "mcpServers": {
    "rtk": {
      "command": "/path/to/rtk-mcp"
    }
  }
}
```

## Usage

Once configured, the `run_command` tool is available in your AI assistant. It accepts:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `command` | string | Yes | The command to execute (e.g. `git status`, `cargo test`) |
| `cwd` | string | No | Working directory (defaults to server cwd) |

Example tool calls from the LLM:

```json
{"name": "run_command", "arguments": {"command": "git log --oneline -5", "cwd": "/my/project"}}
{"name": "run_command", "arguments": {"command": "cargo test"}}
{"name": "run_command", "arguments": {"command": "ls -la src/"}}
```

## Security

### Command allowlist

Only allowlisted commands are accepted. Dangerous commands like `bash`, `sh`, `rm`, `sudo` are blocked:

**Allowed**: `git`, `cargo`, `npm`, `npx`, `pnpm`, `pytest`, `ruff`, `mypy`, `pip`, `uv`, `go`, `golangci-lint`, `docker`, `grep`, `find`, `ls`, `cat`, `head`, `tail`, `wc`, `env`, `echo`, `pwd`, `gh`, `curl`, `wget`, `node`, `tsc`, `next`, `prettier`, `eslint`, `biome`, `playwright`, `prisma`, `vitest`, `dotnet`, `psql`, `make`, `tree`

**Blocked**: Everything else (`bash`, `sh`, `rm`, `sudo`, `chmod`, `python`, etc.)

### Other protections

- **Shell parsing**: Uses `shlex` for proper quoted argument handling (no `split_whitespace` injection)
- **Length limit**: Commands capped at 4096 characters
- **No shell invocation**: Uses `Command::new()` directly, never spawns a shell
- **RTK validation**: Verifies the correct RTK binary is installed at startup (avoids [name collision](https://github.com/rtk-ai/rtk#name-collision))
- **Exit code propagation**: Failed commands return `isError: true` in MCP response

## How it works

```
┌──────────────┐     stdio (JSON-RPC)     ┌──────────────┐
│  MCP Client  │ ◄──────────────────────► │   rtk-mcp    │
│  (Cursor,    │                          │              │
│   Claude     │                          │  1. Parse    │
│   Desktop)   │                          │  2. Validate │
│              │                          │  3. Execute  │
└──────────────┘                          └──────┬───────┘
                                                 │
                                          ┌──────▼───────┐
                                          │     rtk      │
                                          │  (filtering) │
                                          │              │
                                          │ git status   │
                                          │ → 78% less   │
                                          │   tokens     │
                                          └──────────────┘
```

1. MCP client sends a `tools/call` request with a command string
2. `rtk-mcp` validates the command against the allowlist
3. Parses arguments with `shlex` (handles quotes correctly)
4. Executes via `rtk <command>` for filtered output
5. If RTK is unavailable, falls back to raw command execution
6. Returns output with exit code information

## Development

```bash
# Run tests
cargo test

# Build
cargo build --release

# Test MCP protocol manually
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}' | ./target/release/rtk-mcp
```

## Credits

- [RTK (Rust Token Killer)](https://github.com/rtk-ai/rtk) by [rtk-ai](https://github.com/rtk-ai) — all filtering logic
- [rmcp](https://github.com/4t145/rmcp) — Rust MCP SDK
- Built with [Claude Code](https://claude.ai/code)

## License

MIT
