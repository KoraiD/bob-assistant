---
name: bob
description: Use BOB, the user's on-device writing assistant for macOS, through its MCP server. Rewrite, fix, shorten, translate or restyle text with the brain and writing style the user chose; get next-words suggestions; check status; pause BOB while screen recording. Use when the user asks to polish or rewrite text "with BOB", in their own style, or on their own Mac, or asks about BOB's state.
---

# BOB (Bring Your Own Brain)

BOB is a privacy-first macOS writing assistant: Ghost Text (inline suggestions), Selection Actions (rewrite
selected text) and Voice Dictation, powered by a brain the user picks (BOB's own local model, Ollama,
LM Studio or a cloud API). Its MCP server lets you use the same brains and settings from an agent.

## Setup (once)

1. The user opens BOB and turns on **Settings > AI agents (MCP)**. It is off by default; without it every
   tool returns "BOB is not reachable".
2. Add the server to the client. The path is shown in that Settings section.
   - Kiro (`.kiro/settings/mcp.json` or `~/.kiro/settings/mcp.json`):
     ```json
     { "mcpServers": { "bob": { "command": "/Applications/BOB.app/Contents/MacOS/bob-mcp" } } }
     ```
   - Claude Desktop, Cursor and other MCP clients: the same `command`, stdio transport, no arguments.
3. Check with `bob_status`.

## Tools

| Tool | Use it to | Notes |
|---|---|---|
| `bob_status` | See features, pause state, brains (this Mac, local, cloud) and roles | Call first when unsure |
| `bob_list_actions` | List Selection Actions ("Fix grammar", "Make shorter", the user's own) | Labels for `bob_rewrite` |
| `bob_rewrite` | Rewrite `text` with an `action` label or a free `instruction` | Uses the Selection Actions brain, tone, language, spelling, quotes and per-app style |
| `bob_complete` | Get BOB's suggestion for what follows `text` | May return `null` when BOB is unsure |
| `bob_pause` | Pause BOB (`for`: hour, tomorrow, indefinite) | Ask before pausing |
| `bob_resume` | End a pause | |
| `bob_set_feature` | Turn `ghost_text` or `voice_dictation` on or off | Ask before changing the user's settings |
| `bob_usage` | Words BOB typed for the user (today, week, all time) | Counts only |

## How to use it well

- Prefer an existing action (`bob_list_actions`) over a free instruction: actions are tuned for small models.
- Send only the text that needs work, at most 8,000 characters per call. Split longer documents by section.
- The result replaces nothing by itself: show it to the user or apply it with your own file tools.
- If `bob_status` shows a **cloud** Selection brain, the text leaves the Mac. Tell the user when the text looks
  sensitive. BOB refuses to send credential-like text to cloud brains; do not try to work around that.
- When BOB is paused, tools that call a brain return an error. Do not resume BOB without asking.
- Errors are plain sentences meant for the user (for example "No brain is ready for Selection Actions").
  Relay them instead of retrying in a loop. The server allows 60 calls per minute.

## Examples

Fix a paragraph:
```json
{ "name": "bob_rewrite", "arguments": { "text": "teh results was good", "action": "Fix grammar" } }
```

Restyle with an instruction:
```json
{ "name": "bob_rewrite", "arguments": { "text": "We cannot ship on Friday.", "instruction": "Make it friendlier, keep it short" } }
```

Pause for a recording:
```json
{ "name": "bob_pause", "arguments": { "for": "hour" } }
```

## What BOB records

Every MCP connection and call is written to BOB's audit trail (tool name, character count, duration, outcome;
never the text), visible in BOB under Settings > Activity.
