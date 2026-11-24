//! `bob-mcp`: BOB's MCP server for AI agents (stdio). It forwards tool calls to the running BOB app, which
//! must have Settings > AI agents (MCP) turned on. See skills/bob/SKILL.md and docs/MCP.md.
//!
//! Kiro: add to `.kiro/settings/mcp.json`
//! ```json
//! { "mcpServers": { "bob": { "command": "/Applications/BOB.app/Contents/MacOS/bob-mcp" } } }
//! ```

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!(
            "bob-mcp {} (MCP {})",
            env!("CARGO_PKG_VERSION"),
            bob_lib::mcp::PROTOCOL_VERSION
        );
        return;
    }
    bob_lib::mcp::serve_stdio().await;
}
