# Skill And MCP Runtime

`src/skill_mcp.rs` is the runtime layer that turns a routed agent profile into concrete skills and MCP tools.

## Skills

The registry loads:

- built-in skills: `rust-diagnostic-repair`, `test-failure-triage`, and `repo-explainer`
- workspace skills under `.marvis/skills/**/SKILL.md`
- Codex-compatible workspace skills under `.agents/skills/**/SKILL.md`

Each `SKILL.md` must start with YAML-style frontmatter containing at least `name` and `description`. An optional `id` overrides the normalized skill id.

Optional metadata can live at `agents/openai.json` or `agents/openai.yaml` next to the skill file. Marvis reads:

- `capabilities`: local tool names the skill can use
- `dependencies.tools`: MCP dependencies with `type: mcp`, `value`, `transport: stdio`, `command`, and optional `args`

Agent profiles name skills through `skills: ["skill-id"]`. The active local tool list is capped by both the agent profile allowlist and the selected skill capabilities.

## MCP

The MCP runtime reads stdio server config from `.marvis/mcp.json` and `.mcp.json`.

```json
{
  "mcpServers": {
    "local-docs": {
      "transport": "stdio",
      "command": "uvx",
      "args": ["mcp-server-docs"],
      "enabled": true,
      "enabled_tools": []
    }
  }
}
```

Before an agent turn starts, Marvis initializes each selected MCP server, sends `tools/list`, and exposes only discovered tools. Because stdio MCP startup launches a local process, selected MCP servers require shell approval for that accepted turn. Tool names are qualified as `mcp__server__tool` and capped to the Responses API name length. Tool calls go back through the same stdio JSON-RPC path with `tools/call`.

Missing or failing MCP servers fail closed for selected agents. Marvis does not synthesize MCP tools.
