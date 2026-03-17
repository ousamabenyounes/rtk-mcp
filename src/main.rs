use std::process::Command;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_router,
    transport::stdio,
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone)]
pub struct RtkMcpServer {
    tool_router: ToolRouter<Self>,
}

impl RtkMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RunCommandRequest {
    #[schemars(description = "The command to execute, e.g. 'git status', 'cargo test', 'ls -la src/'")]
    command: String,
}

#[tool_router]
impl RtkMcpServer {
    #[tool(
        name = "run_command",
        description = "Execute a shell command through RTK for token-optimized output. \
            Supports git, cargo, npm, pnpm, pytest, go, docker, grep, find, ls, cat, and 25+ \
            other command families. Output is filtered to reduce token consumption by 60-90% \
            while preserving all essential information (errors, summaries, key data). \
            Falls back to raw command execution if RTK is not available."
    )]
    fn run_command(
        &self,
        Parameters(RunCommandRequest { command }): Parameters<RunCommandRequest>,
    ) -> Result<String, String> {
        let command = command.trim().to_string();
        if command.is_empty() {
            return Err("Error: empty command".to_string());
        }

        let parts: Vec<&str> = command.split_whitespace().collect();

        // Try rtk first, fall back to raw command
        match run_via_rtk(&parts) {
            Ok(out) => Ok(out),
            Err(rtk_err) => {
                tracing::warn!("rtk unavailable ({}), falling back to raw command", rtk_err);
                run_raw(&parts)
            }
        }
    }
}

impl ServerHandler for RtkMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "RTK-MCP provides token-optimized command execution. \
                 Use the run_command tool to execute shell commands with \
                 60-90% token reduction via RTK filtering. \
                 Powered by RTK (https://github.com/rtk-ai/rtk).",
            )
    }
}

/// Execute a command through rtk for filtered output.
fn run_via_rtk(parts: &[&str]) -> Result<String, String> {
    let output = Command::new("rtk")
        .args(parts)
        .output()
        .map_err(|e| format!("rtk not found: {}", e))?;

    Ok(collect_output(&output))
}

/// Execute a command directly (fallback when rtk is not available).
fn run_raw(parts: &[&str]) -> Result<String, String> {
    if parts.is_empty() {
        return Err("empty command".to_string());
    }

    let output = Command::new(parts[0])
        .args(&parts[1..])
        .output()
        .map_err(|e| format!("failed to execute '{}': {}", parts[0], e))?;

    Ok(collect_output(&output))
}

/// Combine stdout and stderr into a single string.
fn collect_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&stderr);
    }
    if result.is_empty() {
        result.push_str("(no output)");
    }
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting RTK-MCP server v{}", env!("CARGO_PKG_VERSION"));

    let service = RtkMcpServer::new()
        .serve(stdio())
        .await
        .inspect_err(|e| {
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
        assert_eq!(collect_output(&output), "out\nerr");
    }

    #[test]
    fn test_run_raw_empty() {
        assert!(run_raw(&[]).is_err());
    }

    #[test]
    fn test_run_raw_echo() {
        let result = run_raw(&["echo", "hello"]).unwrap();
        assert!(result.contains("hello"));
    }
}
