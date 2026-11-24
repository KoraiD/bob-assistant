# BOB for AI agents (MCP)

BOB includes a [Model Context Protocol](https://modelcontextprotocol.io) server, so Kiro, Claude Desktop or any MCP
client on this Mac can use BOB's brains and writing settings. The agent-facing guide is
[`skills/bob/SKILL.md`](../skills/bob/SKILL.md): a Skill file other tools can load to learn when and how to use BOB.

## Turn it on

1. In BOB: **Settings > AI agents (MCP) > Allow AI agents to use BOB**. It is off by default.
2. Add BOB to the client. Settings shows the exact path and a Copy button.
   ```json
   { "mcpServers": { "bob": { "command": "/Applications/BOB.app/Contents/MacOS/bob-mcp" } } }
   ```
   In this repository `.kiro/settings/mcp.json` has a `bob` entry for the development build (disabled until you enable
   it, because the build must exist first).

## Tools

| Tool | What it does |
|---|---|
| `bob_status` | Features on or off, pause state, brains (this Mac, local, cloud) and roles |
| `bob_list_actions` | The Selection Actions labels, including the user's own |
| `bob_rewrite` | Rewrite `text` with an `action` label or a free `instruction` (Selection Actions brain, tone, languages, spelling, quotes, per-app style) |
| `bob_complete` | BOB's Ghost Text suggestion for what follows `text` |
| `bob_pause` / `bob_resume` | Pause every feature for an hour, until tomorrow, or until resumed |
| `bob_set_feature` | Turn Ghost Text or Voice Dictation on or off |
| `bob_usage` | Words BOB typed for the user (counts only) |

Tools that change settings carry `readOnlyHint: false`, so clients can ask the user before calling them.

## How it works

```
MCP client ──stdio, JSON-RPC 2.0──▶ bob-mcp ──control.sock (JSON lines)──▶ BOB app ──▶ engines, brains
```

- `bob-mcp` (`src-tauri/src/bin/bob-mcp.rs`, logic in `src-tauri/src/mcp.rs`) implements `initialize`, `ping`,
  `tools/list` and `tools/call` for MCP revisions 2024-11-05, 2025-03-26 and 2025-06-18. Tool failures come back as
  results with `isError: true`, so the agent can read and relay them.
- The app listens on `control.sock` next to the helper socket **only while the setting is on**. The socket is 0600
  in a 0700 per-user folder, and every connection is checked with `getpeereid`: other users are refused and audited.
- Every call runs through the same code as the hotkeys: the same brain, redaction, cloud rule (no credential-like
  text to cloud brains), 8,000-character limit, language check and writing style.
- Limits: 60 calls per minute per connection, 256 KB per request, 120 s per call.
- Every call is audited (tool, characters, milliseconds, outcome; never the text) and shown in Settings > Activity.

## Verified

- Unit tests: the JSON-RPC handshake, tool list and schemas, errors (`mcp::tests`), the rate limit, and a real-socket
  round trip from `bob-mcp`'s client code to the control server, including file permissions and the audit entry.
- End to end on the build machine (2026-11-24): `bob_status`, `bob_rewrite` ("teh results was realy good..." →
  "The results were really good, and we should organize a party.", 152 ms) and `bob_complete` (" as soon as
  possible.", 33 ms) against the running app and the local model; an unknown action returns a readable error.
