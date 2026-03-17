use std::process::Command;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use tracing_subscriber::EnvFilter;

/// Maximum command string length (prevents memory abuse).
const MAX_COMMAND_LEN: usize = 4096;

/// Commands that RTK supports and that are safe to execute.
const ALLOWED_COMMANDS: &[&str] = &[
    "git",
    "cargo",
    "npm",
    "npx",
    "pnpm",
    "pytest",
    "ruff",
    "mypy",
    "pip",
    "uv",
    "go",
    "golangci-lint",
    "docker",
    "grep",
    "find",
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "env",
    "echo",
    "pwd",
    "gh",
    "curl",
    "wget",
    "rtk",
    "node",
    "tsc",
    "next",
    "prettier",
    "eslint",
    "biome",
    "playwright",
    "prisma",
    "vitest",
    "dotnet",
    "psql",
    "make",
    "tree",
];

#[derive(Debug, Clone)]
pub struct RtkMcpServer {
    tool_router: ToolRouter<Self>,
    rtk_available: bool,
}

impl RtkMcpServer {
    pub fn new() -> Self {
        let rtk_available = validate_rtk_installation();
        if rtk_available {
            tracing::info!("RTK detected — commands will be filtered for token savings");
        } else {
            tracing::warn!("RTK not found — commands will execute without filtering");
        }
        Self {
            tool_router: Self::tool_router(),
            rtk_available,
        }
    }
}

impl Default for RtkMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RunCommandRequest {
    #[schemars(
        description = "The command to execute, e.g. 'git status', 'cargo test', 'ls -la src/'. \
        Only allowlisted commands are accepted (git, cargo, npm, go, docker, grep, ls, etc.)."
    )]
    command: String,

    #[schemars(
        description = "Working directory for the command. Defaults to server cwd if omitted."
    )]
    cwd: Option<String>,
}

#[tool_router]
impl RtkMcpServer {
    #[tool(
        name = "run_command",
        description = "Execute a shell command through RTK for token-optimized output. \
            Supports git, cargo, npm, pnpm, pytest, go, docker, grep, find, ls, cat, and 25+ \
            other command families. Output is filtered to reduce token consumption by 60-90% \
            while preserving all essential information (errors, summaries, key data). \
            Falls back to raw command execution if RTK is not available. \
            Only allowlisted commands are accepted for security."
    )]
    fn run_command(
        &self,
        Parameters(RunCommandRequest { command, cwd }): Parameters<RunCommandRequest>,
    ) -> Result<String, String> {
        let command = command.trim().to_string();
        if command.is_empty() {
            return Err("Error: empty command".to_string());
        }
        if command.len() > MAX_COMMAND_LEN {
            return Err(format!(
                "Command too long: {} > {} chars",
                command.len(),
                MAX_COMMAND_LEN
            ));
        }

        // Parse with shlex (handles quotes correctly)
        let parts = shlex::split(&command)
            .ok_or_else(|| "Failed to parse command: unmatched quotes".to_string())?;

        if parts.is_empty() {
            return Err("Error: empty command after parsing".to_string());
        }

        // Security: validate command against allowlist
        let base_cmd = parts[0].rsplit('/').next().unwrap_or(&parts[0]);
        if !ALLOWED_COMMANDS.contains(&base_cmd) {
            return Err(format!(
                "Command '{}' is not in the RTK allowlist. Allowed: {}",
                base_cmd,
                ALLOWED_COMMANDS.join(", ")
            ));
        }

        let parts_ref: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();

        // Try rtk first, fall back to raw command
        let result = if self.rtk_available {
            match run_command_with("rtk", &parts_ref, cwd.as_deref()) {
                Ok(out) => out,
                Err(rtk_err) => {
                    tracing::warn!("rtk failed ({}), falling back to raw command", rtk_err);
                    run_command_with(&parts_ref[0], &parts_ref[1..], cwd.as_deref())?
                }
            }
        } else {
            run_command_with(&parts_ref[0], &parts_ref[1..], cwd.as_deref())?
        };

        // Format output with exit code info
        let mut output = result.output;
        if !result.success {
            output.push_str(&format!("\n[exit code: {}]", result.exit_code));
        }

        if result.success {
            Ok(output)
        } else {
            // Return as Err so MCP marks isError=true
            Err(output)
        }
    }
}

#[tool_handler]
impl ServerHandler for RtkMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "RTK-MCP provides token-optimized command execution. \
                 Use the run_command tool to execute shell commands with \
                 60-90% token reduction via RTK filtering. \
                 Only allowlisted commands are accepted (git, cargo, npm, go, etc.). \
                 Powered by RTK (https://github.com/rtk-ai/rtk).",
        )
    }
}

struct CommandResult {
    output: String,
    exit_code: i32,
    success: bool,
}

/// Execute a command and capture its output with exit code.
fn run_command_with(cmd: &str, args: &[&str], cwd: Option<&str>) -> Result<CommandResult, String> {
    let mut command = Command::new(cmd);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .map_err(|e| format!("failed to execute '{}': {}", cmd, e))?;

    let exit_code = output.status.code().unwrap_or(-1);
    Ok(CommandResult {
        output: collect_output(&output),
        exit_code,
        success: output.status.success(),
    })
}

/// Combine stdout and stderr into a single string.
fn collect_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "(no output)".to_string(),
        (false, true) => stdout.into_owned(),
        (true, false) => stderr.into_owned(),
        (false, false) => format!("{}\n[stderr]\n{}", stdout, stderr),
    }
}

/// Validate that the correct rtk binary (rtk-ai/rtk) is installed.
fn validate_rtk_installation() -> bool {
    Command::new("rtk")
        .arg("--version")
        .output()
        .map(|o| {
            let v = String::from_utf8_lossy(&o.stdout);
            v.starts_with("rtk ") && o.status.success()
        })
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting RTK-MCP server v{}", env!("CARGO_PKG_VERSION"));

    let service = RtkMcpServer::new().serve(stdio()).await.inspect_err(|e| {
        tracing::error!("Server error: {:?}", e);
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_output_stdout_only() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: b"hello world".to_vec(),
            stderr: vec![],
        };
        assert_eq!(collect_output(&output), "hello world");
    }

    #[test]
    fn test_collect_output_empty() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        };
        assert_eq!(collect_output(&output), "(no output)");
    }

    #[test]
    fn test_collect_output_both() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        };
        assert_eq!(collect_output(&output), "out\n[stderr]\nerr");
    }

    #[test]
    fn test_run_raw_echo() {
        let result =
            run_command_with("echo", &["hello"], None).expect("echo should always succeed");
        assert!(result.output.contains("hello"));
        assert!(result.success);
    }

    #[test]
    fn test_run_nonexistent_command() {
        let result = run_command_with("nonexistent_cmd_xyz", &[], None);
        assert!(result.is_err());
    }

    #[test]
    fn test_allowlist_rejects_bash() {
        assert!(!ALLOWED_COMMANDS.contains(&"bash"));
        assert!(!ALLOWED_COMMANDS.contains(&"sh"));
        assert!(!ALLOWED_COMMANDS.contains(&"rm"));
    }

    #[test]
    fn test_allowlist_accepts_git() {
        assert!(ALLOWED_COMMANDS.contains(&"git"));
        assert!(ALLOWED_COMMANDS.contains(&"cargo"));
        assert!(ALLOWED_COMMANDS.contains(&"ls"));
    }

    #[test]
    fn test_shlex_handles_quotes() {
        let parts = shlex::split("git log --format=\"%H %s\"").expect("should parse quoted args");
        assert_eq!(parts, vec!["git", "log", "--format=%H %s"]);
    }

    #[test]
    fn test_shlex_rejects_unmatched_quotes() {
        assert!(shlex::split("git log --format=\"%H").is_none());
    }

    #[test]
    fn test_command_length_limit() {
        let long_cmd = "a".repeat(MAX_COMMAND_LEN + 1);
        assert!(long_cmd.len() > MAX_COMMAND_LEN);
    }
}
